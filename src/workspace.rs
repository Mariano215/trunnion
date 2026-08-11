//! The registry that lets one gantry install answer for more than one
//! repository.
//!
//! Every command before this one takes a path and reads it: `gantry scan
//! <dir>`, `gantry console <ledger-dir>`. That is the right shape for a
//! harness sitting inside the repository it governs, and the wrong shape for
//! the question a review actually asks, which is about a set of repositories
//! and how they compare. The workspace is a list of those repositories and
//! nothing else. It calls the same `scan()` the single-repository command
//! calls and prints the same report, because a second report format would be a
//! second thing to keep true.
//!
//! What the registry deliberately does not hold: credentials. A git project is
//! cloned by shelling out to `git`, which resolves the user's own credential
//! helper; gantry stores no token, reads none, and runs the clone with
//! terminal prompting switched off, so a private URL fails with git's own
//! message rather than blocking on a password nobody typed.
//!
//! The clone is pinned. `rev` is the commit `git rev-parse HEAD` resolved at
//! add time, so the scan of a git project is a scan of a named commit and the
//! number it produced can be pointed back at one.

use crate::Fault;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

/// How much a bad answer costs here. The three values come from harness-kit
/// and are what remediation ranks by, so they are spelled its way and not
/// this file's way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    #[default]
    Internal,
    ClientFacing,
    Regulated,
}

impl Risk {
    pub fn parse(s: &str) -> Result<Risk, Fault> {
        match s {
            "internal" => Ok(Risk::Internal),
            "client_facing" => Ok(Risk::ClientFacing),
            "regulated" => Ok(Risk::Regulated),
            other => Err(Fault::new(
                format!("{other} is not a risk level"),
                "pass --risk internal, --risk client_facing or --risk regulated; remediation ranks by these three and nothing else",
            )),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Risk::Internal => "internal",
            Risk::ClientFacing => "client_facing",
            Risk::Regulated => "regulated",
        }
    }
}

/// Where a project's tree comes from. A local path is read where it sits; a
/// git URL is cloned into the cache once and read there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    Local { path: String },
    Git { url: String, rev: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub source: Source,
    /// The ledger this project's runs are recorded on, once it has one. A
    /// registered project has no ledger until something runs, and a scan is
    /// not a run: it appends nothing.
    #[serde(default)]
    pub ledger: Option<String>,
    #[serde(default)]
    pub last_scan: Option<String>,
    #[serde(default)]
    pub risk: Risk,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub version: u32,
    #[serde(default)]
    pub projects: Vec<Project>,
}

/// The current registry format. A file carrying a version this build does not
/// know is refused rather than read as if the fields meant the same thing.
pub const VERSION: u32 = 1;

/// `$GANTRY_HOME`, or `~/.gantry`. The variable exists because a test that
/// wrote to the operator's real registry would be a test that costs something
/// to run.
pub fn home() -> Result<PathBuf, Fault> {
    if let Some(dir) = std::env::var_os("GANTRY_HOME") {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME").ok_or_else(|| {
        Fault::new(
            "neither GANTRY_HOME nor HOME is set, so there is nowhere to keep the workspace registry",
            "set GANTRY_HOME to the directory the registry should live in, for example GANTRY_HOME=/var/lib/gantry",
        )
    })?;
    Ok(PathBuf::from(home).join(".gantry"))
}

pub fn registry_path(home: &Path) -> PathBuf {
    home.join("workspace.json")
}

impl Workspace {
    /// The registry, or an empty one. A missing file is an install that has
    /// registered nothing yet, which is a state and not an error; anything
    /// else that stops the read is a fault, because silently starting over
    /// would drop the projects the operator added.
    pub fn load(home: &Path) -> Result<Workspace, Fault> {
        let path = registry_path(home);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Workspace {
                    version: VERSION,
                    projects: Vec::new(),
                })
            }
            Err(e) => {
                return Err(Fault::new(
                    format!("cannot read the workspace registry {}: {e}", path.display()),
                    format!("check the permissions on {}; gantry reads the registry from GANTRY_HOME, which defaults to ~/.gantry", path.display()),
                ))
            }
        };
        let workspace: Workspace = serde_json::from_str(&text).map_err(|e| {
            Fault::new(
                format!("{} does not parse as a workspace registry: {e}", path.display()),
                format!("the registry is {{ \"version\": {VERSION}, \"projects\": [ {{ \"id\", \"source\", \"ledger\", \"last_scan\", \"risk\" }} ] }}; fix {} by hand or move it aside and re-add the projects with gantry project add", path.display()),
            )
        })?;
        if workspace.version != VERSION {
            return Err(Fault::new(
                format!(
                    "{} declares registry version {}, and this build reads version {VERSION}",
                    path.display(),
                    workspace.version
                ),
                format!("upgrade gantry, or move {} aside and re-add the projects with gantry project add", path.display()),
            ));
        }
        Ok(workspace)
    }

    pub fn save(&self, home: &Path) -> Result<(), Fault> {
        std::fs::create_dir_all(home).map_err(|e| {
            Fault::new(
                format!("cannot create the gantry home {}: {e}", home.display()),
                format!("check the permissions on {}, or point GANTRY_HOME at a directory this user can write", home.display()),
            )
        })?;
        let path = registry_path(home);
        let text = serde_json::to_string_pretty(self).map_err(|e| {
            Fault::new(
                format!("the workspace registry does not serialise: {e}"),
                "report this as a bug; Workspace is serialisable by construction",
            )
        })?;
        std::fs::write(&path, format!("{text}\n")).map_err(|e| {
            Fault::new(
                format!(
                    "cannot write the workspace registry {}: {e}",
                    path.display()
                ),
                format!("check the permissions on {}", home.display()),
            )
        })
    }

    pub fn find(&self, id: &str) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }

    /// The directory a scan reads for this project: the local path as given,
    /// or the pinned clone under the cache.
    pub fn checkout(&self, home: &Path, project: &Project) -> PathBuf {
        match &project.source {
            Source::Local { path } => PathBuf::from(path),
            Source::Git { .. } => cache_dir(home, &project.id),
        }
    }

    /// Register a local directory or a git URL. The id is the directory
    /// basename or the repository name unless one is passed.
    pub fn add(
        &mut self,
        home: &Path,
        target: &str,
        id: Option<&str>,
        risk: Risk,
    ) -> Result<Project, Fault> {
        let id = match id {
            Some(id) => id.to_string(),
            None => default_id(target)?,
        };
        check_id(&id)?;
        if let Some(existing) = self.find(&id) {
            return Err(Fault::new(
                format!(
                    "the workspace already has a project called {id}, registered from {}",
                    source_text(&existing.source)
                ),
                format!("pass --id <id> to register {target} under a different name, or remove the existing one with gantry project remove {id}"),
            ));
        }
        let source = if is_url(target) {
            let dest = cache_dir(home, &id);
            // An orphaned tree can sit here two ways: a project removed from
            // the registry left its clone behind, and a clone killed part way
            // left a partial one. Both are gantry's own writes under its own
            // cache, under an id the duplicate check just proved no project
            // holds, so neither is anyone's working copy and neither is worth
            // making the operator delete by hand before re-adding.
            discard_orphan(&dest)?;
            clone(target, &dest)?;
            Source::Git {
                url: target.to_string(),
                rev: head_rev(&dest)?,
            }
        } else {
            Source::Local {
                path: local_path(target)?,
            }
        };
        let project = Project {
            id,
            source,
            // ponytail: a project gets a ledger when something records a run
            // against it, and a scan records nothing. Nothing writes this
            // field yet; the field exists because the registry format is what
            // remediation reads.
            ledger: None,
            last_scan: None,
            risk,
        };
        self.projects.push(project.clone());
        Ok(project)
    }

    pub fn remove(&mut self, id: &str) -> Result<Project, Fault> {
        let Some(at) = self.projects.iter().position(|p| p.id == id) else {
            return Err(Fault::new(
                format!("the workspace has no project called {id}"),
                match self.projects.is_empty() {
                    true => "the registry is empty; add one with gantry project add <path-or-url>"
                        .to_string(),
                    false => format!(
                        "registered ids are {}; see them with gantry project list",
                        self.ids().join(", ")
                    ),
                },
            ));
        };
        // The clone under the cache is left where it is: removing a tree is
        // not something a registry edit should do on the way past, and an
        // operator who removed a project by mistake still has the checkout.
        // Re-adding the same id does not trip over it, because `add` discards
        // an orphan before it clones.
        Ok(self.projects.remove(at))
    }

    pub fn ids(&self) -> Vec<String> {
        self.projects.iter().map(|p| p.id.clone()).collect()
    }

    pub fn mark_scanned(&mut self, id: &str, ts: &str) {
        if let Some(p) = self.projects.iter_mut().find(|p| p.id == id) {
            p.last_scan = Some(ts.to_string());
        }
    }
}

/// Where a clone lives. One directory per project id, under the gantry home,
/// so nothing is written next to the repository being read.
pub fn cache_dir(home: &Path, id: &str) -> PathBuf {
    home.join("cache").join(id)
}

/// One line naming where a project came from, for a fault or a list row.
pub fn source_text(source: &Source) -> String {
    match source {
        Source::Local { path } => path.clone(),
        Source::Git { url, rev } => format!("{url} at {rev}"),
    }
}

/// A target is a URL when it carries a transport or is an scp-style git
/// address. Everything else is a path, including a relative one.
fn is_url(target: &str) -> bool {
    target.contains("://") || target.starts_with("git@")
}

/// The id a target gets when none is passed: the directory basename, or the
/// repository name out of the URL.
fn default_id(target: &str) -> Result<String, Fault> {
    let trimmed = target.trim_end_matches('/');
    let name = trimmed
        .rsplit(['/', ':'])
        .next()
        .unwrap_or("")
        .trim_end_matches(".git");
    if name.is_empty() || name == "." || name == ".." {
        return Err(Fault::new(
            format!("cannot read a project id out of {target}"),
            format!("pass --id <id>, for example gantry project add {target} --id my-project"),
        ));
    }
    Ok(name.to_string())
}

/// An id is one path segment, because it becomes one: `cache/<id>` is where a
/// clone is written. Left unchecked, `--id ../../thing` puts a clone outside
/// the cache and `--id .` puts it on top of it. The operator can already write
/// anywhere they like, so this is not a privilege boundary; it is the
/// difference between a typo that is refused and a typo that unpacks a
/// repository somewhere nothing will ever look for it again.
fn check_id(id: &str) -> Result<(), Fault> {
    let ok = !id.is_empty()
        && id != "."
        && id != ".."
        && !id.contains('/')
        && !id.contains('\\')
        && !id.starts_with('-')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    if ok {
        return Ok(());
    }
    Err(Fault::new(
        format!("{id} is not usable as a project id"),
        "an id is one path segment, because a clone is written to GANTRY_HOME/cache/<id>: use ASCII letters, digits, dash, underscore and dot, with no slash and no leading dash",
    ))
}

/// The absolute path of a local project, refusing anything that is not a
/// directory that exists. The registry is read again days later, so a
/// relative path stored as given would resolve against whatever directory the
/// next command ran in.
fn local_path(target: &str) -> Result<String, Fault> {
    let canonical = std::fs::canonicalize(target).map_err(|e| {
        Fault::new(
            format!("cannot register {target}: {e}"),
            format!("check that {target} exists and is a directory; gantry project add takes the repository root, or a git URL when the repository is not on this machine"),
        )
    })?;
    if !canonical.is_dir() {
        return Err(Fault::new(
            format!("{} is not a directory", canonical.display()),
            format!("pass the repository root rather than a file inside it; {target} is a file"),
        ));
    }
    Ok(canonical.display().to_string())
}

/// Drop a leftover checkout under the cache so a clone can be written there.
///
/// The path is `$GANTRY_HOME/cache/<id>` and the caller has already proved no
/// registered project holds that id, so this only ever removes a tree gantry
/// wrote for a project that no longer exists. It removes nothing outside the
/// cache: a path that is not under `home/cache` is a bug in the caller and is
/// refused rather than deleted.
fn discard_orphan(dest: &Path) -> Result<(), Fault> {
    if !dest.exists() {
        return Ok(());
    }
    if dest.parent().and_then(|p| p.file_name()) != Some(std::ffi::OsStr::new("cache")) {
        return Err(Fault::new(
            format!("{} is not inside the clone cache", dest.display()),
            "report this as a bug; gantry only ever discards a tree it wrote under GANTRY_HOME/cache/<id>",
        ));
    }
    std::fs::remove_dir_all(dest).map_err(|e| {
        Fault::new(
            format!(
                "cannot discard the leftover checkout at {}: {e}",
                dest.display()
            ),
            format!(
                "remove {} by hand and add the project again",
                dest.display()
            ),
        )
    })
}

/// Shallow-clone a repository into the cache.
///
/// `GIT_TERMINAL_PROMPT=0` is the whole credential story: git resolves the
/// user's own helper as it always does, and a URL it has no credential for
/// fails with git's message instead of blocking on a password prompt inside
/// whatever invoked gantry. Nothing here reads, stores or forwards a token.
///
/// The URL is passed after `--` and refused if it begins with a dash, because
/// otherwise it is not a URL, it is argv. `is_url` only asks for a transport,
/// and `--upload-pack=ssh://x` carries one: git would read it as the option it
/// looks like and run the command it names. The operator typing that at
/// themselves is not the case worth defending against; a URL arriving from an
/// issue, a README or a colleague is, and that is the normal way a repository
/// address is obtained.
fn clone(url: &str, dest: &Path) -> Result<(), Fault> {
    if url.starts_with('-') {
        return Err(Fault::new(
            format!("{url} begins with a dash, so git would read it as an option rather than a repository"),
            "pass the repository address itself; an argument like --upload-pack=... names a command for git to run and is not a URL, however much it looks like one",
        ));
    }
    if dest.exists() {
        return Err(Fault::new(
            format!("{} already exists", dest.display()),
            format!("remove {} and add the project again, or pass --id <id> to clone under a different name; a leftover directory here is a clone that failed part way", dest.display()),
        ));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            Fault::new(
                format!("cannot create the clone cache {}: {e}", parent.display()),
                format!("check the permissions on {}, or point GANTRY_HOME at a directory this user can write", parent.display()),
            )
        })?;
    }
    let out = Command::new("git")
        .args(["clone", "--depth", "1", "--"])
        .arg(url)
        .arg(dest)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| {
            Fault::new(
                format!("cannot run git to clone {url}: {e}"),
                "install git and put it on PATH; gantry clones by running git so that your existing credential helper is the only thing holding a token",
            )
        })?;
    if !out.status.success() {
        return Err(Fault::new(
            format!(
                "git clone of {url} failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
            format!("check the URL and that this machine can reach it; a private repository needs a git credential helper that already answers for {url}, because gantry stores no token and never prompts for one"),
        ));
    }
    Ok(())
}

/// The commit a clone landed on, which is what pins the scan.
fn head_rev(dest: &Path) -> Result<String, Fault> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dest)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|e| {
            Fault::new(
                format!("cannot run git to resolve HEAD in {}: {e}", dest.display()),
                "install git and put it on PATH",
            )
        })?;
    if !out.status.success() {
        return Err(Fault::new(
            format!(
                "cannot resolve HEAD in the clone at {}: {}",
                dest.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            ),
            format!("remove {} and add the project again; a clone whose commit cannot be read pins nothing, and an unpinned scan cannot be pointed back at a revision", dest.display()),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_defaults_to_the_directory_or_the_repository_name() {
        assert_eq!(default_id("/Users/x/code/foo").unwrap(), "foo");
        assert_eq!(default_id("/Users/x/code/foo/").unwrap(), "foo");
        assert_eq!(default_id("https://github.com/org/bar.git").unwrap(), "bar");
        assert_eq!(default_id("git@github.com:org/bar.git").unwrap(), "bar");
        // A path that names no directory of its own gets no guessed id: the
        // fault says to pass one rather than registering a project called ".".
        assert!(default_id(".").is_err());
    }

    #[test]
    fn an_id_that_would_leave_the_cache_is_refused() {
        assert!(check_id("gantry").is_ok());
        assert!(check_id("claude-harness-core").is_ok());
        assert!(check_id("v0.2.0_draft").is_ok());
        // Every one of these is a path segment somewhere it should not be.
        for bad in ["../../etc", "a/b", "..", ".", "", "a\\b", "--risk"] {
            assert!(check_id(bad).is_err(), "{bad} was accepted as an id");
        }
        // The fault names the id, because the operator is looking at a typo.
        let fault = check_id("a/b").unwrap_err();
        assert!(fault.cause.contains("a/b"), "{fault}");
    }

    #[test]
    fn a_transport_or_an_scp_address_is_a_url_and_a_path_is_not() {
        assert!(is_url("https://github.com/org/bar"));
        assert!(is_url("file:///tmp/remote"));
        assert!(is_url("git@github.com:org/bar.git"));
        assert!(!is_url("/Users/x/code/foo"));
        assert!(!is_url("./foo"));
    }
}
