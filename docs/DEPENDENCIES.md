# Dependencies

Rule from CLAUDE.md: anything with a network or process capability needs a
note here. ureq is the first dependency that does; see below.

| Crate | Why it is here | Network | Process |
|---|---|---|---|
| serde, serde_json | envelope serialisation | no | no |
| serde_jcs | RFC 8785 canonical JSON, EVENT-SCHEMA.md constraint 2 | no | no |
| sha2 | RFC 6962 leaf and node hashing | no | no |
| ed25519-dalek | signed tree heads, actor attestations | no | no |
| getrandom | OS entropy for ledger key generation (syscall only) | no | no |
| hex | hash and key encoding | no | no |
| ureq | gateway adapter HTTP client (rustls) | yes | no |

## ureq

Network capability: yes, and it is the point. The gateway adapter is the one
chokepoint allowed to reach a model provider (architecture invariant one).
Blocking client, rustls, no tokio tree. Tests never use it against a real
host; the suite talks to loopback stubs only.

## site/vendor (not a crate either, noted for the same reason)

The page published to GitHub Pages ships React and ReactDOM 18.3.1, MIT, as
the UMD production builds with their licence headers intact, under
`site/vendor`. They are there so the page fetches nothing at view time: as the
design tool exported it, the page pulled both from unpkg and three font
families from Google Fonts, on a page whose own text says this project fetches
nothing from any host.

Network capability at view time: none, which is the point, and
`ci/site-offline.sh` is what says so rather than this paragraph. The build
(`dev/build-site.py`) copies them from a node_modules directory already on the
build machine and fetches nothing itself; it refuses rather than downloading if
the version `site/support.js` names is not there. Nothing in the Rust binary
links against any of this: it is a static page, served by GitHub, and the
console the binary serves is unrelated and still has no dependency at all.

## std::process (not a crate, noted anyway)

Since slice 03 the broker executes shell commands strictly after an allow
verdict from the policy. Since slice 04 it does so through
`/usr/bin/sandbox-exec` (seatbelt), a platform binary, not a crate: a
per-run profile denies all non-allowlisted network and every write outside
the run's own workdir, and the child's environment is cleared to PATH, HOME,
TMPDIR plus the credential handles the policy granted. This is the crate's
only process capability, and it sits behind the same chokepoint that records
the call. The backend actually in force is recorded on every `tool.request`
(`sandbox`) and on `run.open` (`isolation.active_backend`), so a missing
backend degrades to `none` visibly rather than silently.

`sandbox-exec` is macOS-specific. On a host without it and without Landlock
the backend records `none` and the isolation claim is honestly unmet.

## landlock

The Linux isolation backend, and the reason `none` is no longer what trunnion
reports on every non-macOS host. Until this crate arrived the binary compiled
and ran on Linux with no containment at all, in a tool whose entire pitch is
measuring whether other people's agents are contained.

Network capability: none. Process capability: it restricts the calling thread
between fork and exec, which is the same `std::process` capability noted above
and narrows it rather than widening it.

Landlock rather than seccomp, namespaces or bubblewrap, because it is
unprivileged and kernel-native. It behaves identically on bare metal and
inside a plain unprivileged `docker run`; bubblewrap needs user namespaces or
CAP_SYS_ADMIN and would silently no-op under default Docker, which is the
class of silent non-enforcement this project exists to catch. ABI v4 added TCP
bind and connect restrictions, so the egress half needs no second mechanism
and this is one dependency rather than two.

The crate rather than the raw syscalls, which is the part worth arguing. The
Landlock ABI has changed in every kernel from 5.13 to 6.12: v1 in 5.13, v2 in
5.19, v3 in 6.2, v4 in 6.7, and access rights added after that. A hand-rolled
`landlock_create_ruleset` has to negotiate the running kernel's version and
mask the access rights it asks for down to what that version knows, or the
call returns EINVAL and applies nothing. Getting that wrong does not fail
loudly, it ships a sandbox that silently enforces nothing on a kernel nobody
tested, which is precisely the defect the backend was added to remove. The
crate is maintained by the kernel feature's author and does the negotiation
correctly, and `src/sandbox.rs` sets `CompatLevel::HardRequirement` and
refuses anything short of `RulesetStatus::FullyEnforced` so a downgrade is a
failed spawn rather than a quiet one.
