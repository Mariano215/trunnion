//! `gantry scan`: a read-only static read of a repository's harness surface.
//!
//! The scan reads files. It never runs the repository, never reaches the
//! network, and never writes to the tree it is pointed at: every filesystem
//! call in this module goes through [`RepoRead`], which exposes reads only, so
//! "the scan writes nothing" is the shape of the code rather than a promise in
//! a document. `tests/scan.rs` asserts both halves, the shape (no
//! write-capable `std::fs` call in this file) and the behaviour (the scanned
//! tree is byte-identical afterwards).
//!
//! # What a static read can and cannot resolve
//!
//! Three states per primitive, and the ceiling is deliberate:
//!
//! * `0`, absent. Nothing at any of the paths looked in.
//! * `2`, an artifact exists and nothing found here enforces it.
//! * `3`, an artifact exists and a check file (CI config, hook config, task
//!   runner) names it.
//!
//! It cannot award `1`, because "one person's habits, no artifact" leaves
//! nothing in a file tree to read. It cannot award `4` or `5`, because a file
//! says a check is wired and only a run says the check fired and could have
//! failed. That distinction is the whole thesis of this repository: a layer
//! carried only by a guide caps at 3, and anchor 4 in `docs/PRIMITIVES.md`
//! requires enforcement by the system rather than by discipline.
//!
//! `gantry score` is the other number. It reads a ledger, every predicate is a
//! statement about events that happened, and it is the only one of the two
//! that can award 4. The two are not averaged and are not expected to agree:
//! the static scan under-reads a running system (a recorder is runtime state,
//! so primitive 11 can read 0 on disk while telemetry reads 3) and it can
//! over-read a dead one (a check file naming a check says the check is wired,
//! never that it works). Where they disagree, the telemetry number is the one
//! that measured something running.

use crate::Fault;
use std::path::{Path, PathBuf};

/// The highest score a static read is allowed to produce. Anything above this
/// would be a claim about a control running, which needs telemetry.
pub const STATIC_CEILING: u8 = 3;

/// The only filesystem access this module has. Every method reads; there is no
/// write, create, rename or remove anywhere on this type, which is what makes
/// the read-only property structural.
pub struct RepoRead {
    root: PathBuf,
}

impl RepoRead {
    pub fn open(root: &Path) -> Result<RepoRead, Fault> {
        let canonical = std::fs::canonicalize(root).map_err(|e| {
            Fault::new(
                format!("cannot read {} to scan it: {e}", root.display()),
                "pass the path of a repository directory that exists; the scan only reads, so a path it cannot open is the one thing that stops it",
            )
        })?;
        if !canonical.is_dir() {
            return Err(Fault::new(
                format!("{} is not a directory", canonical.display()),
                "pass the repository root, not a file inside it",
            ));
        }
        Ok(RepoRead { root: canonical })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// File contents, or None when the path is absent or unreadable.
    fn text(&self, rel: &str) -> Option<String> {
        std::fs::read_to_string(self.root.join(rel)).ok()
    }

    /// The names of the files directly under a directory, sorted. Empty when
    /// the directory is absent, unreadable, or holds no files.
    fn files_in(&self, rel: &str) -> Vec<String> {
        let mut names: Vec<String> = match std::fs::read_dir(self.root.join(rel)) {
            Ok(entries) => entries
                .flatten()
                .filter(|e| e.path().is_file())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect(),
            Err(_) => Vec::new(),
        };
        names.sort();
        names
    }

    /// Every file under the root, relative to it, sorted, skipping
    /// [`UNWALKED`]. Reads directory entries and nothing else.
    fn walk(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![self.root.clone()];
        while let Some(dir) = stack.pop() {
            let entries = match std::fs::read_dir(&dir) {
                Ok(entries) => entries,
                // An unreadable directory is one gap in the walk, never the
                // end of it: the same rule the drift walk follows.
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                if path.is_dir() {
                    // A directory holding its own .git is a separate
                    // repository: a submodule, or one of the worktrees an
                    // agent session leaves under .claude. Walking into it
                    // reports the same fixture once per checkout, which
                    // buries the tree actually being scanned. It gets scanned
                    // by pointing this at its own root.
                    if !UNWALKED.contains(&name.as_str()) && !path.join(".git").exists() {
                        stack.push(path);
                    }
                } else if let Ok(rel) = path.strip_prefix(&self.root) {
                    out.push(rel.display().to_string());
                }
            }
        }
        out.sort();
        out
    }

    /// True when the path holds something: a file, or a directory with at
    /// least one file in it. An empty directory is not an artifact.
    fn present(&self, rel: &str) -> bool {
        match rel.strip_suffix('/') {
            Some(dir) => !self.files_in(dir).is_empty(),
            None => self.root.join(rel).is_file(),
        }
    }
}

/// A PKCS8 ed25519 private key is 48 bytes. Nothing real is smaller, so a PEM
/// block whose body decodes to less than this is a fixture and one that
/// reaches it is key material.
pub const SMALLEST_REAL_KEY: usize = 48;

/// Directories `scan-keys` does not walk. Build output and vendored packages
/// hold other people's fixtures by the thousand, and `.git` holds every
/// version of every file, which would report one leak per commit that touched
/// it rather than one per file.
const UNWALKED: &[&str] = &[".git", "target", "node_modules", ".venv", "dist"];

/// One PEM private key block: where it is, and how many bytes its base64 body
/// decodes to. The size is the whole verdict, which is why it is the only
/// thing recorded besides the position.
pub struct KeyBlock {
    pub path: String,
    pub line: usize,
    pub bytes: usize,
}

/// Every PEM private key block under a root, split by whether it could hold a
/// key. `real` is empty in a repository whose blocks are all sensor controls.
pub struct KeyScan {
    pub root: String,
    pub fixtures: Vec<KeyBlock>,
    pub real: Vec<KeyBlock>,
    pub files_read: usize,
}

/// One primitive's static probe: the paths that count as an artifact, and the
/// substrings that count as a check file naming it.
struct Probe {
    primitive: u8,
    name: &'static str,
    artifacts: &'static [&'static str],
    markers: &'static [&'static str],
}

/// Where a check lives in a repository that has one. Read once per scan and
/// searched for each primitive's markers.
const CHECK_FILES: &[&str] = &[
    ".github/workflows/",
    ".gitlab-ci.yml",
    ".circleci/config.yml",
    "ci/",
    "Makefile",
    "justfile",
    ".pre-commit-config.yaml",
    ".claude/settings.json",
    ".claude/hooks/",
    "package.json",
    "noxfile.py",
    "tox.ini",
];

/// Files that carry the rules a repository declares for its agents, read for
/// `[UNENFORCED]` markers.
const RULE_FILES: &[&str] = &["CLAUDE.md", "AGENTS.md", ".cursorrules"];

/// The twelve probes, in `docs/PRIMITIVES.md` order. Every path here is a
/// convention some repository actually uses, never a gantry-specific one:
/// pointing the scan at this repository and having it recognise this
/// repository's private layout would make the number a compliment rather than
/// a measurement.
const PROBES: &[Probe] = &[
    Probe {
        primitive: 1,
        name: "Instruction",
        artifacts: &[
            "CLAUDE.md",
            "AGENTS.md",
            ".cursorrules",
            ".cursor/rules/",
            ".github/copilot-instructions.md",
            "instructions/",
        ],
        markers: &[
            "claude.md",
            "agents.md",
            "copilot-instructions",
            "instruction",
        ],
    },
    Probe {
        primitive: 2,
        name: "Context delivery",
        artifacts: &[".claude/hooks/", ".mcp.json", ".cursor/mcp.json"],
        markers: &[
            "sessionstart",
            "userpromptsubmit",
            "precompact",
            "additionalcontext",
        ],
    },
    Probe {
        primitive: 3,
        name: "Context management",
        artifacts: &[
            "graphify-out/",
            "retrieval.json",
            "embeddings/",
            "vectorstore/",
            ".rag/",
            ".index/",
        ],
        markers: &["retriev", "rerank", "compact", "embedding"],
    },
    Probe {
        primitive: 4,
        name: "Tool interface",
        artifacts: &[
            ".mcp.json",
            ".cursor/mcp.json",
            "tools/",
            "config/tools/",
            "openapi.json",
            "openapi.yaml",
        ],
        markers: &["mcp", "schema", "tool"],
    },
    Probe {
        primitive: 5,
        name: "Execution environment",
        artifacts: &[
            "Dockerfile",
            "docker-compose.yml",
            ".devcontainer/",
            "sandbox/",
            ".claude/settings.json",
        ],
        markers: &["sandbox", "seatbelt", "seccomp", "container", "egress"],
    },
    Probe {
        primitive: 6,
        name: "Durable state",
        artifacts: &[
            "PLAN.md",
            "docs/PLAN.md",
            "TODO.md",
            "checkpoints/",
            "state/",
            ".claude/state/",
        ],
        markers: &["checkpoint", "resume", "snapshot"],
    },
    Probe {
        primitive: 7,
        name: "Orchestration",
        artifacts: &[
            ".claude/settings.json",
            ".claude/hooks/",
            ".github/workflows/",
        ],
        markers: &["hook", "approval", "gate", "retry"],
    },
    Probe {
        primitive: 8,
        name: "Sub-agents",
        artifacts: &[".claude/agents/", "agents/", ".claude/subagents/"],
        markers: &["subagent", "agent"],
    },
    Probe {
        primitive: 9,
        name: "Skills",
        artifacts: &[
            ".claude/skills/",
            ".claude/commands/",
            "skills/",
            "runbooks/",
            "playbooks/",
        ],
        markers: &["skill", "runbook", "playbook"],
    },
    Probe {
        primitive: 10,
        name: "Verification",
        artifacts: &["tests/", "test/", "spec/", "__tests__/", "src/test/"],
        markers: &["test", "lint", "typecheck"],
    },
    Probe {
        primitive: 11,
        name: "Observability",
        artifacts: &[
            "logs/",
            "telemetry/",
            "otel-collector.yaml",
            "otel.yaml",
            ".ledger/",
        ],
        markers: &["trace", "telemetry", "otel", "ledger"],
    },
    Probe {
        primitive: 12,
        name: "Governance",
        artifacts: &[
            ".claude/settings.json",
            "config/policy.json",
            "policy.json",
            "CODEOWNERS",
            ".github/CODEOWNERS",
        ],
        markers: &["policy", "codeowner", "drift", "permission", "audit"],
    },
];

/// One primitive's number and the paths behind it. The evidence field is never
/// empty by construction: it either names what was found or names every path
/// that was looked in and came back empty.
#[derive(Debug, Clone)]
pub struct Finding {
    pub primitive: u8,
    pub name: &'static str,
    pub score: u8,
    pub evidence: String,
}

/// An `[UNENFORCED]` marker in a rule file. `check` is the id the marker names
/// as the thing that would close it, when the marker names one.
#[derive(Debug, Clone)]
pub struct Marker {
    pub file: String,
    pub line: usize,
    pub check: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ScanReport {
    pub root: String,
    pub findings: Vec<Finding>,
    /// The minimum across the twelve, never the average.
    pub overall: u8,
    pub checks_read: Vec<String>,
    pub markers: Vec<Marker>,
}

/// Lowercased, with whole-line comments dropped. A CI file that mentions the
/// sandbox in a comment is not a file that checks the sandbox, and crediting
/// one would flatter every well-commented repository, this one first.
fn searchable(text: &str) -> String {
    text.lines()
        .map(str::trim_start)
        .filter(|l| !l.starts_with('#') && !l.starts_with("//"))
        .collect::<Vec<&str>>()
        .join("\n")
        .to_lowercase()
}

/// Read every check file the repository has, ready for substring search.
fn read_checks(repo: &RepoRead) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for entry in CHECK_FILES {
        match entry.strip_suffix('/') {
            Some(dir) => {
                for name in repo.files_in(dir) {
                    let rel = format!("{dir}/{name}");
                    if let Some(text) = repo.text(&rel) {
                        out.push((rel, searchable(&text)));
                    }
                }
            }
            None => {
                if let Some(text) = repo.text(entry) {
                    out.push(((*entry).to_string(), searchable(&text)));
                }
            }
        }
    }
    out
}

/// The first check file naming one of the markers, with the marker it named.
/// ponytail: plain substring matching over check files, which can credit a
/// mention in a comment as a check. The evidence line names the file and the
/// string so a reader can overrule it; parsing each CI dialect properly is the
/// upgrade path, and it buys accuracy this command does not need to promise.
fn enforcing<'a>(
    checks: &'a [(String, String)],
    markers: &[&'a str],
) -> Option<(&'a str, &'a str)> {
    for (path, text) in checks {
        for marker in markers {
            if text.contains(marker) {
                return Some((path.as_str(), marker));
            }
        }
    }
    None
}

/// The first backticked token in a fragment that looks like a check id: no
/// whitespace, and a separator in it. `ci/sensor-placement-honoured` qualifies,
/// a prose phrase does not. Odd segments of a backtick split are the quoted
/// ones, and the caller strips the marker's own closing backtick first so that
/// parity is real. Taking only the first pair read the gap between the
/// marker's backtick and the id's as the token, found it empty, and fell
/// through to the next line, which is how a marker naming its check on its own
/// line was reported as naming none.
fn check_id(fragment: &str) -> Option<String> {
    fragment
        .split('`')
        .skip(1)
        .step_by(2)
        .map(str::trim)
        .find(|token| {
            !token.is_empty() && !token.contains(char::is_whitespace) && token.contains(['/', '-'])
        })
        .map(str::to_string)
}

fn markers(repo: &RepoRead) -> Vec<Marker> {
    let mut out = Vec::new();
    for file in RULE_FILES {
        let Some(text) = repo.text(file) else {
            continue;
        };
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let Some((_, after)) = line.split_once("[UNENFORCED]") else {
                continue;
            };
            // The marker is usually backticked in a rule file, which leaves its
            // own closing backtick at the head of the fragment. Strip it, or
            // every quoted token after it reads on the wrong parity.
            let after = after.trim_start_matches('`');
            // The check id is on the marker's own line, or wraps onto the next
            // one. Same line wins: a wrapped fallback that fired while the id
            // sat on the marker's own line reported the next path in the
            // paragraph as the check that would close the rule.
            let check = check_id(after).or_else(|| lines.get(i + 1).and_then(|n| check_id(n)));
            out.push(Marker {
                file: (*file).to_string(),
                line: i + 1,
                check,
            });
        }
    }
    out
}

/// True when the line carries a PEM private key header of any type. Written
/// out rather than matched with a pattern because the only variable part is
/// the key type, which is upper case, digits and spaces between two fixed
/// halves.
fn is_key_header(line: &str) -> bool {
    let Some(after) = line.split_once("-----BEGIN ").map(|(_, a)| a) else {
        return false;
    };
    let Some((kind, _)) = after.split_once("PRIVATE KEY-----") else {
        return false;
    };
    kind.chars().all(|c| c.is_ascii_uppercase() || c == ' ')
}

/// True when the line is nothing but base64, once the quoting a JSON string or
/// a Rust literal wraps it in is stripped. A block's body is the run of these
/// under its header, and the run ends at the first line that is not one, which
/// is the `END` marker in a terminated block and the next line of code in a
/// fixture that has none.
fn body_line(line: &str) -> Option<&str> {
    let stripped = line.trim().trim_matches(['"', ',', '\\']);
    let base64 = !stripped.is_empty()
        && stripped
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=');
    base64.then_some(stripped)
}

/// Bytes a base64 body of this many characters decodes to. Only the length is
/// needed to tell a key from a fixture, so nothing is decoded and no base64
/// dependency is taken.
fn decoded_len(chars: usize) -> usize {
    (chars / 4) * 3
        + match chars % 4 {
            2 => 1,
            3 => 2,
            // A remainder of 1 is not valid base64 at all.
            _ => 0,
        }
}

/// Every PEM private key block in a file's text, as (line in the file, decoded
/// body size).
///
/// A control stored in JSON carries its newlines as the two characters
/// backslash and n, so its block is one line in the file and four in the
/// value; those are split while the file's own line number is kept, so the
/// report names a line an editor can jump to.
///
/// The body is the run of base64 lines under the header rather than the span
/// between `BEGIN` and `END`. Pairing the markers reads too wide: a fixture
/// with no footer matches the `END` of an unrelated block further down and the
/// whole span between them is measured as one key.
fn key_blocks(text: &str) -> Vec<(usize, usize)> {
    let mut lines: Vec<(usize, &str)> = Vec::new();
    for (number, line) in text.lines().enumerate() {
        for part in line.split("\\n") {
            lines.push((number + 1, part));
        }
    }
    let mut out = Vec::new();
    for (i, (number, line)) in lines.iter().enumerate() {
        if !is_key_header(line) {
            continue;
        }
        let chars: usize = lines[i + 1..]
            .iter()
            .map_while(|(_, follower)| body_line(follower))
            .map(|body| body.trim_end_matches('=').len())
            .sum();
        out.push((*number, decoded_len(chars)));
    }
    out
}

/// Walk a tree and measure every PEM private key block in it.
///
/// This exists because a secret scanner cannot be pointed at a harness without
/// alerting on it. The `no-private-key` sensor's negative controls have to be
/// the literal bytes its check greps for, one per branch of the check, or the
/// branch is dead while the sensor still reports live. So the harness ships a
/// scanner exemption for those paths, and an exemption is a switched-off
/// sensor: this is what stands behind it. It reads the whole tree rather than
/// the exempted paths, so widening the exemption cannot widen the hole.
///
/// Measuring the body beats matching the header, which is what the exemption
/// turns off, and beats parsing it: an `openssl pkey` parse cannot load an
/// OpenSSH block at all, so a real OpenSSH key would read as unparseable and
/// pass.
pub fn scan_keys(repo: &RepoRead) -> KeyScan {
    let mut scan = KeyScan {
        root: repo.root().display().to_string(),
        fixtures: Vec::new(),
        real: Vec::new(),
        files_read: 0,
    };
    for rel in repo.walk() {
        // A file with no readable text holds no PEM block; read_to_string
        // returning None covers both a binary and an unreadable file.
        let Some(text) = repo.text(&rel) else {
            continue;
        };
        scan.files_read += 1;
        if !text.contains("PRIVATE KEY") {
            continue;
        }
        for (line, bytes) in key_blocks(&text) {
            let block = KeyBlock {
                path: rel.clone(),
                line,
                bytes,
            };
            if bytes >= SMALLEST_REAL_KEY {
                scan.real.push(block);
            } else {
                scan.fixtures.push(block);
            }
        }
    }
    scan
}

impl KeyScan {
    /// True when every block found is too small to be a key.
    pub fn ok(&self) -> bool {
        self.real.is_empty()
    }

    /// The report as text. A finding names the file, the line and the size,
    /// and the fix names what to do rather than only what is wrong.
    pub fn text(&self) -> String {
        let mut s = String::new();
        for block in &self.real {
            s.push_str(&format!(
                "{}:{}: a PEM private key block decoding to {} bytes. Fix: {SMALLEST_REAL_KEY} bytes is a PKCS8 ed25519 key and nothing real is smaller, so this is key material rather than a sensor control. Rotate it, remove it from history, and if the file has to show the shape of a key, truncate the body the way config/sensors/no-private-key.json does.\n",
                block.path, block.line, block.bytes
            ));
        }
        if self.real.is_empty() {
            for block in &self.fixtures {
                s.push_str(&format!(
                    "  {}:{} decodes to {} bytes\n",
                    block.path, block.line, block.bytes
                ));
            }
            s.push_str(&format!(
                "{} file(s) read under {}, {} PEM private key block(s), every one under {SMALLEST_REAL_KEY} bytes, so every one is a fixture\n",
                self.files_read,
                self.root,
                self.fixtures.len()
            ));
        } else {
            s.push_str(&format!(
                "{} block(s) hold key material. A scanner exemption covering them (.gitleaks.toml, .github/secret_scanning.yml) exempts a path from pattern matching and exempts nothing from this check, which is why this reads every file under {} and not only the exempted ones.\n",
                self.real.len(),
                self.root
            ));
        }
        s
    }
}

pub fn scan(repo: &RepoRead) -> ScanReport {
    let checks = read_checks(repo);
    let mut findings = Vec::new();
    for probe in PROBES {
        let found: Vec<&str> = probe
            .artifacts
            .iter()
            .copied()
            .filter(|p| repo.present(p))
            .collect();
        let (score, evidence) = if found.is_empty() {
            (
                0,
                format!("looked in {}: found nothing", probe.artifacts.join(", ")),
            )
        } else {
            match enforcing(&checks, probe.markers) {
                Some((path, marker)) => (
                    3,
                    format!(
                        "found {}, and {} names it (contains \"{}\"); a file says the check is wired, only telemetry says it ran",
                        found.join(", "),
                        path,
                        marker
                    ),
                ),
                None => (
                    2,
                    format!(
                        "found {}, and no check file names it, so it is an artifact carried by discipline",
                        found.join(", ")
                    ),
                ),
            }
        };
        findings.push(Finding {
            primitive: probe.primitive,
            name: probe.name,
            score: score.min(STATIC_CEILING),
            evidence,
        });
    }
    let overall = findings.iter().map(|f| f.score).min().unwrap_or(0);
    ScanReport {
        root: repo.root().display().to_string(),
        findings,
        overall,
        checks_read: checks.into_iter().map(|(path, _)| path).collect(),
        markers: markers(repo),
    }
}

impl ScanReport {
    /// The report as text. One line per primitive, `primitive NN Name | score
    /// | evidence`, so a check can read it without a JSON parser and fail on a
    /// number with nothing behind it.
    pub fn text(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!(
            "gantry scan {}\nread-only static scan of what is on disk. Twelve primitives, a path behind every number.\n\n",
            self.root
        ));
        for f in &self.findings {
            s.push_str(&format!(
                "primitive {:02} {:<22} | {} | {}\n",
                f.primitive, f.name, f.score, f.evidence
            ));
        }
        s.push_str(&format!(
            "\noverall {} | the minimum across {} primitives, never the average\n",
            self.overall,
            self.findings.len()
        ));
        s.push_str(&format!(
            "\nceiling: a static read resolves three states, absent (0), an artifact nothing enforces (2), and an artifact a check file names ({STATIC_CEILING}). It awards no 1, because habits leave no file; no 4 or 5, because a file says a check is wired and only a run says it fired; and no N/A, because a tree does not show which primitives the workload exercises. For 4 and above, run gantry score over a ledger: it reads events, not files. The two numbers are not averaged, and where they disagree the telemetry one measured something running.\n"
        ));
        if self.checks_read.is_empty() {
            s.push_str(&format!(
                "\ncheck files: looked in {}: found nothing, so no primitive here can score above 2\n",
                CHECK_FILES.join(", ")
            ));
        } else {
            s.push_str(&format!(
                "\ncheck files read: {}\n",
                self.checks_read.join(", ")
            ));
        }
        s.push_str(&format!(
            "\n[UNENFORCED] markers in {}: {} (work items, not failures)\n",
            RULE_FILES.join(", "),
            self.markers.len()
        ));
        for m in &self.markers {
            match &m.check {
                Some(check) => s.push_str(&format!("  {}:{} {}\n", m.file, m.line, check)),
                None => s.push_str(&format!(
                    "  {}:{} names no check id on this line or the next, so nothing says what would close it\n",
                    m.file, m.line
                )),
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_check_id_is_read_off_the_marker_line_or_the_next_one() {
        assert_eq!(
            check_id("` ci/sensor-placement-honoured `.rest"),
            Some("ci/sensor-placement-honoured".to_string())
        );
        // A prose phrase in backticks is not a check id, which is what keeps
        // the paragraph defining the convention from reading as a work item
        // that names a check.
        assert_eq!(check_id("and `gantry scan` reports it"), None);
        assert_eq!(check_id("no backticks here"), None);
        // The shape a rule file actually writes: the marker is itself
        // backticked, so the fragment after it opens with a stray closing
        // backtick that markers() strips. The id has to survive both the
        // stripping and the prose that follows it on the same line, because
        // reading past it lands on the next path in the paragraph.
        assert_eq!(
            check_id(" `ci/anchor-schedule`"),
            Some("ci/anchor-schedule".to_string())
        );
        assert_eq!(
            check_id(" `ci/sensor-placement-honoured`. This marker was carried by"),
            Some("ci/sensor-placement-honoured".to_string())
        );
    }

    #[test]
    fn the_first_check_file_naming_a_marker_is_the_evidence() {
        let checks = vec![
            ("ci/run.sh".to_string(), "cargo test --all".to_string()),
            (
                ".github/workflows/ci.yml".to_string(),
                "run: zsh".to_string(),
            ),
        ];
        assert_eq!(
            enforcing(&checks, &["test", "lint"]),
            Some(("ci/run.sh", "test"))
        );
        assert_eq!(enforcing(&checks, &["seatbelt"]), None);
    }
}
