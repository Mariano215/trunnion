//! Slice 16 integration: `gantry scan` reads a repository and writes nothing.
//! The two properties that matter are that every number carries a path behind
//! it, including the numbers that are zero, and that the tree the scan was
//! pointed at is byte-identical afterwards. Both are asserted here, and the
//! second is asserted twice: once against the running scan, once against the
//! shape of the module, because a read-only property that depends on nobody
//! adding a write later is a promise rather than a control.

use gantry::scan::{scan, scan_keys, RepoRead, SMALLEST_REAL_KEY, STATIC_CEILING};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn workdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gantry-scan-it-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn repo_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

/// Every file under a tree with its bytes and its modification time, which is
/// what a scan that touched anything would change.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>, SystemTime)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let meta = fs::metadata(&path).unwrap();
                out.push((
                    path.clone(),
                    fs::read(&path).unwrap(),
                    meta.modified().unwrap(),
                ));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn an_empty_repository_scores_zero_and_says_where_it_looked() {
    let dir = workdir("empty");
    let report = scan(&RepoRead::open(&dir).unwrap());

    assert_eq!(report.findings.len(), 12, "twelve primitives, always");
    assert_eq!(report.overall, 0);
    for f in &report.findings {
        assert_eq!(f.score, 0, "primitive {} found nothing", f.primitive);
        assert!(
            f.evidence.contains("looked in") && f.evidence.contains("found nothing"),
            "primitive {} scored 0 without naming the paths it looked in: {}",
            f.primitive,
            f.evidence
        );
        // The paths have to be real strings, not an empty list rendered as a
        // sentence: a zero with no path behind it is the thing this command
        // exists to refuse.
        assert!(f.evidence.len() > "looked in : found nothing".len());
    }
    assert!(report.checks_read.is_empty());
    assert!(report.markers.is_empty());
    let text = report.text();
    assert!(text.contains("no primitive here can score above 2"));
}

#[test]
fn a_scan_never_writes_to_the_target() {
    let dir = workdir("readonly");
    fs::create_dir_all(dir.join(".claude/hooks")).unwrap();
    fs::create_dir_all(dir.join("tests")).unwrap();
    fs::write(
        dir.join("CLAUDE.md"),
        "# rules\n`[UNENFORCED]` `ci/nothing`\n",
    )
    .unwrap();
    fs::write(dir.join(".claude/hooks/pre.sh"), "echo hi\n").unwrap();
    fs::write(dir.join("tests/t.rs"), "fn main() {}\n").unwrap();

    let before = snapshot(&dir);
    let report = scan(&RepoRead::open(&dir).unwrap());
    let after = snapshot(&dir);

    assert_eq!(
        before, after,
        "the scan changed the tree it was pointed at; it reads and nothing else"
    );
    // And it did do the work, so the comparison is not passing on an empty run.
    assert!(report.findings.iter().any(|f| f.score > 0));
}

#[test]
fn the_scanner_holds_no_write_capable_filesystem_call() {
    let source = fs::read_to_string(repo_path("src/scan.rs")).unwrap();
    // The names of every std::fs entry point that can change a tree. The scan
    // is read-only because none of them is reachable from this module, not
    // because the author meant well.
    for forbidden in [
        "fs::write",
        "File::create",
        "OpenOptions",
        "create_dir",
        "remove_file",
        "remove_dir",
        "fs::rename",
        "fs::copy",
        "set_permissions",
        "fs::hard_link",
        "fs::soft_link",
    ] {
        assert!(
            !source.contains(forbidden),
            "src/scan.rs references {forbidden}; gantry scan is read-only, so every filesystem call in it goes through RepoRead, which has no write"
        );
    }
}

#[test]
fn an_artifact_scores_two_and_a_check_naming_it_scores_three() {
    let dir = workdir("tiers");
    fs::create_dir_all(dir.join("tests")).unwrap();
    fs::write(dir.join("tests/t.rs"), "fn main() {}\n").unwrap();

    let verification = |root: &Path| {
        scan(&RepoRead::open(root).unwrap())
            .findings
            .into_iter()
            .find(|f| f.primitive == 10)
            .unwrap()
    };

    let before = verification(&dir);
    assert_eq!(before.score, 2, "tests exist and nothing runs them");
    assert!(before.evidence.contains("tests/"));

    fs::create_dir_all(dir.join("ci")).unwrap();
    fs::write(dir.join("ci/run.sh"), "cargo test --all\n").unwrap();
    let after = verification(&dir);
    assert_eq!(after.score, 3, "a check file names the tests");
    assert!(
        after.evidence.contains("ci/run.sh"),
        "the score has to name the check: {}",
        after.evidence
    );

    // A comment is not a check. This is the line between measuring a
    // repository and flattering a well-commented one.
    fs::write(
        dir.join("ci/run.sh"),
        "# we should run cargo test one day\n",
    )
    .unwrap();
    let commented = verification(&dir);
    assert_eq!(
        commented.score, 2,
        "a comment mentioning tests is not a gate"
    );
}

#[test]
fn scanning_this_repository_stays_under_its_own_ceiling_and_reports_its_markers() {
    let report = scan(&RepoRead::open(&repo_path(".")).unwrap());

    for f in &report.findings {
        assert!(
            f.score <= STATIC_CEILING,
            "primitive {} scored {}, above the static ceiling; a file cannot show a check running",
            f.primitive,
            f.score
        );
        assert!(
            !f.evidence.trim().is_empty(),
            "primitive {} scored with no evidence behind it",
            f.primitive
        );
    }
    assert!(
        report.overall <= STATIC_CEILING,
        "the static overall must not exceed the ceiling, and must not exceed what gantry score reads off a real ledger"
    );
    assert!(
        report
            .markers
            .iter()
            .any(|m| m.check.as_deref() == Some("ci/sensor-placement-honoured")),
        "the scan did not report the [UNENFORCED] marker CLAUDE.md carries, which is the one thing CLAUDE.md says it will do"
    );
}

#[test]
fn a_report_round_trips_through_json_and_the_overall_is_still_the_minimum() {
    let dir = workdir("json");
    fs::create_dir_all(dir.join("tests")).unwrap();
    fs::create_dir_all(dir.join("ci")).unwrap();
    fs::write(dir.join("tests/t.rs"), "fn main() {}\n").unwrap();
    fs::write(dir.join("ci/run.sh"), "cargo test --all\n").unwrap();
    fs::write(dir.join("CLAUDE.md"), "# rules\n").unwrap();

    let report = scan(&RepoRead::open(&dir).unwrap());
    let text = serde_json::to_string(&report).unwrap();
    let back: serde_json::Value = serde_json::from_str(&text).unwrap();

    // The consumer of this JSON is another program, so every field the text
    // report prints has to survive the trip under a name it can read.
    assert_eq!(back["root"], report.root);
    assert_eq!(back["overall"], report.overall);
    let findings = back["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 12);
    for (json, f) in findings.iter().zip(&report.findings) {
        assert_eq!(json["primitive"], f.primitive);
        assert_eq!(json["name"], f.name);
        assert_eq!(json["score"], f.score);
        assert_eq!(json["evidence"], f.evidence);
        assert_eq!(json["gap"], f.gap);
    }

    // The rule the whole number rests on: the overall is the worst primitive,
    // never an average that a good primitive can lift.
    let minimum = report.findings.iter().map(|f| f.score).min().unwrap();
    assert_eq!(report.overall, minimum);
    assert_eq!(
        back["overall"].as_u64().unwrap(),
        u64::from(minimum),
        "the serialised overall disagreed with the minimum across the findings"
    );
}

#[test]
fn every_score_under_the_ceiling_names_what_would_raise_it() {
    // Two repositories, so both branches under the ceiling are exercised: one
    // with nothing in it (every primitive at 0) and this one, which has
    // artifacts nothing enforces alongside artifacts a check names.
    for root in [workdir("gap-empty"), repo_path(".")] {
        let report = scan(&RepoRead::open(&root).unwrap());
        for f in &report.findings {
            if f.score >= STATIC_CEILING {
                assert!(
                    f.gap.is_empty(),
                    "primitive {} is at the ceiling and still names a gap; above 3 is a control running, which no file added to the tree can show: {}",
                    f.primitive,
                    f.gap
                );
                continue;
            }
            assert!(
                !f.gap.trim().is_empty(),
                "primitive {} scored {} and says nothing about what is missing",
                f.primitive,
                f.score
            );
            // Grounded in the probe, not invented: the gap names a path or a
            // marker the scan actually looked for, and the level it would
            // reach. A recommendation the scan did not check for is the thing
            // this assertion exists to refuse.
            assert!(
                f.gap.contains(if f.score == 0 {
                    "to reach 2"
                } else {
                    "to reach 3"
                }),
                "primitive {} scored {} and its gap does not name the next level: {}",
                f.primitive,
                f.score,
                f.gap
            );
        }
    }
}

/// A PEM block with a body this long, built at runtime rather than written
/// out, because a 64-character base64 literal in this file would be a real key
/// by the very measure under test and `gantry scan-keys` would fail on the
/// test that proves it works.
fn pem(kind: &str, body_chars: usize) -> String {
    format!(
        "-----BEGIN {kind}PRIVATE KEY-----\n{}\n-----END {kind}PRIVATE KEY-----\n",
        "A".repeat(body_chars)
    )
}

#[test]
fn a_truncated_control_is_a_fixture_and_a_full_body_is_key_material() {
    let dir = workdir("keys");
    // What a sensor's negative control looks like: enough header to make the
    // check fire, no key under it.
    fs::write(dir.join("control.md"), pem("", 6)).unwrap();
    let clean = scan_keys(&RepoRead::open(&dir).unwrap());
    assert!(clean.ok(), "a truncated control is not key material");
    assert_eq!(clean.fixtures.len(), 1);

    // 64 base64 characters is 48 bytes, which is a PKCS8 ed25519 key exactly.
    fs::write(dir.join("real.pem"), pem("", 64)).unwrap();
    let dirty = scan_keys(&RepoRead::open(&dir).unwrap());
    assert!(
        !dirty.ok(),
        "a body at the size of a real key must be caught"
    );
    assert_eq!(dirty.real.len(), 1);
    assert_eq!(dirty.real[0].bytes, SMALLEST_REAL_KEY);
    assert!(
        dirty.text().contains("real.pem:1"),
        "the finding names the file and the line: {}",
        dirty.text()
    );
}

#[test]
fn a_key_pasted_into_a_sensor_control_is_caught_through_the_json_escaping() {
    let dir = workdir("keys-json");
    // The shape a real key arrives in if somebody pastes one where a control
    // belongs: one JSON line, newlines as backslash-n. The scan has to see
    // through that, and the line it names is the line in the file.
    let sensor = format!(
        "{{\n  \"id\": \"no-private-key\",\n  \"negative_control\": [\n    {}\n  ]\n}}\n",
        serde_json::to_string(&pem("OPENSSH ", 400)).unwrap()
    );
    fs::write(dir.join("sensor.json"), sensor).unwrap();
    let scanned = scan_keys(&RepoRead::open(&dir).unwrap());
    assert!(
        !scanned.ok(),
        "a real OpenSSH key wrapped as a JSON control passed; an openssl parse cannot load one at all, which is why the size is what is measured"
    );
    assert_eq!(scanned.real[0].line, 4, "the line named is the file's own");
}

#[test]
fn a_nested_repository_is_not_walked() {
    let dir = workdir("keys-nested");
    fs::create_dir_all(dir.join("worktree/.git")).unwrap();
    fs::write(dir.join("worktree/control.md"), pem("RSA ", 6)).unwrap();
    fs::write(dir.join("own.md"), pem("RSA ", 6)).unwrap();
    let scanned = scan_keys(&RepoRead::open(&dir).unwrap());
    assert_eq!(
        scanned.fixtures.len(),
        1,
        "a checkout with its own .git is a separate repository; walking it reports the same fixture once per copy and buries the tree being scanned"
    );
}
