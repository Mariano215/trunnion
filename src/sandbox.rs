//! Slice 04: per-run isolation. Every broker-executed command runs inside a
//! profile derived from the policy, never hand-edited per call, that denies
//! all network except the policy's egress allowlist and denies all writes
//! outside the run's own workdir. The active backend is recorded on every
//! `tool.request` so the declaration in `profile_requirements.isolation` is
//! observable rather than asserted.
//!
//! Two backends, because the claim has to hold on the machine the claim is
//! made on. macOS is seatbelt (`/usr/bin/sandbox-exec`). Linux is Landlock,
//! and it is Landlock rather than namespaces or bubblewrap because Landlock
//! is unprivileged and kernel-native: it applies identically on bare metal
//! and inside a plain `docker run`, where a user-namespace sandbox needs
//! privileges it will not get and would no-op. It is Landlock rather than
//! Landlock plus seccomp because ABI v4 restricts TCP bind and connect, so
//! the egress half needs no second mechanism.
//!
//! What the backend string is for. It names the level actually in force, not
//! the mechanism chosen: a kernel offering only ABI v3 enforces the
//! filesystem half and nothing about the network, and calling that
//! `landlock` full stop would put a claim on the ledger that the kernel is
//! not keeping. `active_backend` is what drift reads and what `run.open`
//! records, so overstating it here is the exact failure this project exists
//! to catch.

use crate::Fault;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(target_os = "linux")]
use std::os::unix::process::CommandExt;

const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

static SANDBOX_SEQ: AtomicU64 = AtomicU64::new(0);

/// The isolation backend a run gets on this host. This is what
/// `profile_requirements.isolation.observed_by: sandbox.active_backend`
/// names, and it is the same expression `Sandbox::per_run` stamps on every
/// `tool.request`, so the drift check and the event cannot disagree. It is
/// readable without building a sandbox, because run open has to answer
/// whether the declared isolation is available at all before it appends its
/// first event.
#[cfg(not(target_os = "linux"))]
pub fn active_backend() -> &'static str {
    if Path::new(SANDBOX_EXEC).exists() {
        "seatbelt"
    } else {
        "none"
    }
}

/// The Linux answer, negotiated against the running kernel rather than
/// assumed from the crate version: ABI v1 arrived in 5.13 and v4 in 6.7, so a
/// build that enforced v4 unconditionally would fail to apply anything at all
/// on a kernel in between. `landlock-v4` is filesystem and network,
/// `landlock-v1` through `-v3` are filesystem only, and the caller can tell
/// the difference from the string.
#[cfg(target_os = "linux")]
pub fn active_backend() -> &'static str {
    use landlock::{LandlockStatus, RestrictSelf, ABI};
    // A probe, not a restriction. With no flags requested and no_new_privs
    // off, `apply` performs the kernel version query and returns before any
    // restricting syscall, so asking what is available cannot sandbox the
    // process that asked. Nothing else in the crate exposes the number,
    // deliberately, to stop callers building a ruleset out of it.
    match RestrictSelf::default().no_new_privs(false).apply() {
        Ok(status) => match status.landlock {
            LandlockStatus::Available { effective_abi, .. } => match effective_abi {
                ABI::Unsupported => "none",
                ABI::V1 => "landlock-v1",
                ABI::V2 => "landlock-v2",
                ABI::V3 => "landlock-v3",
                // v5 and later add rights this profile does not ask for; what
                // is in force is still the v4 profile, and the string says so.
                _ => "landlock-v4",
            },
            // Landlock compiled out, or built in and not on the boot LSM list.
            LandlockStatus::NotEnabled | LandlockStatus::NotImplemented => "none",
        },
        Err(_) => "none",
    }
}

/// The property a profile declares when it asks for isolation without naming
/// the mechanism that provides it: a per-run sandbox confining both the
/// filesystem and the network. This is the name `profile_requirements`
/// declares, and `Providable::for_this_build` adds it to what the host
/// provides only when the backend in force actually holds both halves.
///
/// The field used to name a mechanism, `seatbelt`, and the comparison was
/// string equality, so a Linux host confining a run with Landlock v4 recorded
/// a shortfall it did not have and a `regulated` profile under
/// `on_unavailable: refuse` could not start there at all. Naming the property
/// fixes that without weakening it: a mechanism name still works as a
/// declaration for anyone who needs to pin one, and a host with neither
/// backend still provides nothing but `none`.
pub const CONFINEMENT: &str = "per_run_confinement";

/// Whether a backend string confines the network as well as the filesystem.
/// This is the whole substance of the property above, and it is why the
/// property is not a rename of the mechanism: Landlock added TCP bind and
/// connect restrictions in ABI v4, so `landlock-v1` through `-v3` enforce the
/// filesystem half and nothing about egress. A v3 kernel therefore does not
/// satisfy `per_run_confinement` and degrades honestly, which is the same
/// answer the old string comparison gave for the right reason rather than by
/// accident.
pub fn confines_filesystem_and_network(backend: &str) -> bool {
    backend == "seatbelt" || backend == "landlock-v4"
}

/// A process-unique scratch directory under TMPDIR. Run ids are millisecond
/// timestamps, so two runs opened in the same millisecond (parallel tests,
/// tight loops) would otherwise share a sandbox workdir and each other's
/// staged files; the atomic suffix makes the path unique regardless.
pub fn unique_run_dir(prefix: &str) -> PathBuf {
    let n = SANDBOX_SEQ.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("{prefix}-{}-{n}", std::process::id()))
}

/// The ABI a backend string promises to enforce, and `None` for a host with
/// no Landlock. Going back through the string rather than carrying an `ABI`
/// on the struct keeps one source of truth: what is enforced is what
/// `active_backend` said, so the ledger cannot record one level and the
/// kernel apply another.
#[cfg(target_os = "linux")]
fn enforced_abi(kind: &str) -> Option<landlock::ABI> {
    use landlock::ABI;
    match kind {
        "landlock-v1" => Some(ABI::V1),
        "landlock-v2" => Some(ABI::V2),
        "landlock-v3" => Some(ABI::V3),
        "landlock-v4" => Some(ABI::V4),
        _ => None,
    }
}

/// Applies the run's ruleset to the calling thread. Called between fork and
/// exec, so it must not fail quietly: every step is a hard requirement and
/// anything short of a fully enforced ruleset returns an error, which makes
/// the spawn fail rather than letting an unrestricted child run.
///
/// The rule set mirrors the seatbelt profile rather than inventing a second
/// policy: read and execute everywhere, write only beneath the run's workdir,
/// and TCP connect only to an allowlisted port.
#[cfg(target_os = "linux")]
fn restrict(
    abi: landlock::ABI,
    workdir: &Path,
    ports: &[u16],
    network: bool,
) -> std::io::Result<()> {
    use landlock::{
        path_beneath_rules, Access, AccessFs, AccessNet, CompatLevel, Compatible, NetPort, Ruleset,
        RulesetAttr, RulesetCreatedAttr, RulesetStatus,
    };

    let fail = |e: landlock::RulesetError| {
        std::io::Error::other(format!("landlock ruleset could not be applied: {e}"))
    };

    let mut ruleset = Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(AccessFs::from_all(abi))
        .map_err(fail)?;
    if network {
        ruleset = ruleset
            .handle_access(AccessNet::from_all(abi))
            .map_err(fail)?;
    }
    let status = ruleset
        .create()
        .map_err(fail)?
        // Read and execute stay open, exactly as `(allow default)` leaves
        // them under seatbelt. Narrowing reads here would make the two
        // platforms enforce different policies under one declaration.
        .add_rules(path_beneath_rules(["/"], AccessFs::from_read(abi)))
        .map_err(fail)?
        .add_rules(path_beneath_rules([workdir], AccessFs::from_all(abi)))
        .map_err(fail)?
        // The shell itself needs the null device and its tty.
        .add_rules(path_beneath_rules(
            ["/dev/null", "/dev/tty"],
            AccessFs::from_all(abi),
        ))
        .map_err(fail)?
        .add_rules(
            ports
                .iter()
                .map(|p| Ok(NetPort::new(*p, AccessNet::ConnectTcp))),
        )
        .map_err(fail)?
        .restrict_self()
        .map_err(fail)?;
    if status.ruleset != RulesetStatus::FullyEnforced {
        return Err(std::io::Error::other(format!(
            "landlock reported {:?} rather than a fully enforced ruleset",
            status.ruleset
        )));
    }
    Ok(())
}

pub struct Sandbox {
    profile: String,
    workdir: PathBuf,
    kind: &'static str,
    /// Parsed from the egress allowlist at build time so a malformed entry is
    /// a fault the caller sees, not a rule silently dropped after fork.
    #[cfg(target_os = "linux")]
    egress_ports: Vec<u16>,
}

impl Sandbox {
    /// Builds the per-run sandbox. `egress_allow` comes from
    /// `profile_requirements.egress.allow`; each entry is `ip:port` in
    /// seatbelt remote-ip syntax (`localhost:11434` is the loopback form).
    pub fn per_run(workdir: &Path, egress_allow: &[String]) -> Result<Sandbox, Fault> {
        std::fs::create_dir_all(workdir).map_err(|e| {
            Fault::new(
                format!("cannot create run workdir {}: {e}", workdir.display()),
                "check TMPDIR is writable; every run needs its own scratch directory",
            )
        })?;
        // Symlinks (macOS /tmp -> /private/tmp) would silently widen or
        // narrow the subpath scope, so resolve before writing the profile.
        let workdir = workdir.canonicalize().map_err(|e| {
            Fault::new(
                format!("cannot canonicalise workdir {}: {e}", workdir.display()),
                "the workdir must exist and be readable at sandbox build time",
            )
        })?;
        let mut profile = String::from("(version 1)\n(allow default)\n(deny network*)\n");
        for host in egress_allow {
            profile.push_str(&format!("(allow network* (remote ip \"{host}\"))\n"));
        }
        profile.push_str("(deny file-write*)\n");
        profile.push_str(&format!(
            "(allow file-write* (subpath \"{}\"))\n",
            workdir.display()
        ));
        // The shell itself needs the null device and its tty.
        profile
            .push_str("(allow file-write-data (literal \"/dev/null\") (literal \"/dev/tty\"))\n");
        let kind = active_backend();
        // Landlock scopes a TCP connect by port and not by address, which is
        // narrower than seatbelt's `remote ip` in one direction and wider in
        // the other: an allowlisted port is reachable on any host. The
        // tracked laptop policy allows nothing, so this widens nothing today,
        // and a policy that does allow an entry gets port scope rather than
        // address scope on Linux.
        #[cfg(target_os = "linux")]
        let egress_ports = egress_allow
            .iter()
            .map(|entry| {
                entry
                    .rsplit_once(':')
                    .and_then(|(_, port)| port.parse::<u16>().ok())
                    .ok_or_else(|| {
                        Fault::new(
                            format!("egress allowlist entry {entry} names no port"),
                            "write every profile_requirements.egress.allow entry as host:port; the landlock backend scopes a connect by port, so an entry with no port names nothing it can enforce",
                        )
                    })
            })
            .collect::<Result<Vec<_>, Fault>>()?;
        Ok(Sandbox {
            profile,
            workdir,
            kind,
            #[cfg(target_os = "linux")]
            egress_ports,
        })
    }

    /// What `tool.request.sandbox` records. "none" means the backend binary
    /// is missing and the profile is not being enforced; the laptop profile
    /// degrades rather than refuses, and the record says so.
    pub fn kind(&self) -> &'static str {
        self.kind
    }

    pub fn workdir(&self) -> &Path {
        &self.workdir
    }

    /// One command inside the sandbox with a cleaned environment. The child
    /// sees PATH, HOME and TMPDIR pointed at the workdir, plus exactly the
    /// `inject` pairs (credential handles the policy granted), and nothing
    /// else from the parent, which is what keeps a hostile `env` empty.
    pub fn command(&self, shell_command: &str, inject: &[(String, String)]) -> Command {
        let mut cmd = if self.kind == "seatbelt" {
            let mut c = Command::new(SANDBOX_EXEC);
            c.arg("-p")
                .arg(&self.profile)
                .arg("sh")
                .arg("-c")
                .arg(shell_command);
            c
        } else {
            let mut c = Command::new("sh");
            c.arg("-c").arg(shell_command);
            c
        };
        // Landlock restricts the calling thread and the restriction survives
        // exec, so it is applied in the child between fork and exec rather
        // than by wrapping the command in a helper binary. The ruleset is
        // built inside the closure rather than handed in ready-made, because
        // `restrict_self` consumes the ruleset and a Command may be spawned
        // more than once; a ruleset that could be applied only to the first
        // child is how an unsandboxed second one gets shipped.
        // ponytail: allocates after fork, which is the standard pattern the
        // crate documents and is safe here because the child execs
        // immediately; a fully async-signal-safe path would need the ruleset
        // fd built in the parent, which the crate does not expose.
        #[cfg(target_os = "linux")]
        if let Some(abi) = enforced_abi(self.kind) {
            let workdir = self.workdir.clone();
            let ports = self.egress_ports.clone();
            let network = self.kind == "landlock-v4";
            // SAFETY: the closure calls only landlock and prctl syscalls and
            // allocates; it spawns no thread and touches no shared state.
            unsafe {
                cmd.pre_exec(move || restrict(abi, &workdir, &ports, network));
            }
        }
        cmd.env_clear();
        cmd.env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
        cmd.env("HOME", &self.workdir);
        cmd.env("TMPDIR", &self.workdir);
        for (k, v) in inject {
            cmd.env(k, v);
        }
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox(name: &str, egress: &[String]) -> Sandbox {
        let dir = std::env::temp_dir().join(format!("trunnion-sbx-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Sandbox::per_run(&dir, egress).unwrap()
    }

    #[test]
    fn profile_denies_network_and_foreign_writes() {
        let s = sandbox("profile", &[]);
        assert!(s.profile.contains("(deny network*)"));
        assert!(s.profile.contains("(deny file-write*)"));
        assert!(s.profile.contains(&format!(
            "(allow file-write* (subpath \"{}\"))",
            s.workdir().display()
        )));
        #[cfg(not(target_os = "linux"))]
        assert_eq!(s.kind(), "seatbelt");
        #[cfg(target_os = "linux")]
        assert!(
            s.kind().starts_with("landlock-"),
            "linux backend is {}, so nothing is being enforced",
            s.kind()
        );
    }

    /// The containment claim stated as a difference rather than as an
    /// assertion about a backend string. The same command must fail inside
    /// the sandbox and succeed outside it: a test that only asserts the
    /// failure passes just as well when the command was broken for some
    /// unrelated reason, which is a check that has never been observed able
    /// to fail.
    #[test]
    fn the_foreign_write_that_fails_inside_succeeds_outside() {
        let s = sandbox("contain", &[]);
        let foreign =
            std::env::temp_dir().join(format!("trunnion-sbx-contain-{}", std::process::id()));
        let _ = std::fs::remove_file(&foreign);
        let shell = format!("echo pwned > {}", foreign.display());

        let inside = s.command(&shell, &[]).output().unwrap();
        assert!(
            !inside.status.success(),
            "the sandbox did not contain the write: {}",
            String::from_utf8_lossy(&inside.stderr)
        );
        assert!(!foreign.exists(), "the contained write reached the disk");

        let outside = Command::new("sh").arg("-c").arg(&shell).output().unwrap();
        assert!(
            outside.status.success(),
            "the same command fails with no sandbox too, so the assertion above proves nothing"
        );
        assert!(foreign.exists());
        let _ = std::fs::remove_file(&foreign);
    }

    #[test]
    fn allowlist_entries_become_remote_ip_allows() {
        let s = sandbox("egress", &["localhost:11434".to_string()]);
        assert!(s
            .profile
            .contains("(allow network* (remote ip \"localhost:11434\"))"));
    }

    #[test]
    fn environment_is_cleaned_and_injection_is_explicit() {
        std::env::set_var("TRUNNION_SBX_CANARY", "leak-me");
        let s = sandbox("env", &[]);
        let out = s
            .command(
                "env",
                &[("GRANTED".to_string(), "handle-value".to_string())],
            )
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        std::env::remove_var("TRUNNION_SBX_CANARY");
        assert!(!text.contains("leak-me"), "parent env leaked: {text}");
        assert!(
            text.contains("GRANTED=handle-value"),
            "injection missing: {text}"
        );
    }

    #[test]
    fn writes_outside_the_workdir_fail_inside() {
        let s = sandbox("writes", &[]);
        let foreign =
            std::env::temp_dir().join(format!("trunnion-sbx-foreign-{}", std::process::id()));
        let _ = std::fs::remove_file(&foreign);
        let out = s
            .command(&format!("touch {}", foreign.display()), &[])
            .output()
            .unwrap();
        assert!(!out.status.success(), "foreign write succeeded");
        assert!(!foreign.exists());
        let inside = s.workdir().join("mine");
        let out = s
            .command(&format!("touch {}", inside.display()), &[])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "workdir write failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(inside.exists());
    }

    /// Loopback is network too; an empty allowlist denies it. This is the
    /// no-network-in-tests invariant used as a fixture: the connection is
    /// attempted at a loopback listener and must die at the sandbox.
    ///
    /// The unsandboxed leg is not decoration. On a host with no `nc` the
    /// sandboxed command exits 127 and the denial assertion passes without a
    /// packet having been attempted, which is a dead check reporting green;
    /// this was the state of the Linux backend's first run. Asserting the
    /// same command succeeds outside is what makes the failure mean the
    /// sandbox.
    #[test]
    fn loopback_is_denied_when_allowlist_is_empty() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let shell = format!("nc -w 1 127.0.0.1 {port} < /dev/null");
        let s = sandbox("loopback", &[]);
        let out = s.command(&shell, &[]).output().unwrap();
        assert!(!out.status.success(), "sandboxed nc reached loopback");

        let outside = Command::new("sh").arg("-c").arg(&shell).output().unwrap();
        assert!(
            outside.status.success(),
            "nc cannot reach a loopback listener on this host even with no sandbox, so the denial above proves nothing. Fix: install netcat (netcat-openbsd), or the egress half of this backend has no check behind it"
        );
    }
}
