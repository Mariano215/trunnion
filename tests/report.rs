//! Slice 25: the deliverable. A report is worth what its proofs are worth, so
//! the two things under test are that a bundle written beside a finding checks
//! out against the key that travels with it, and that a claim edited after the
//! fact fails rather than reading as verified. The second is the whole product:
//! a report nobody can refute is a report nobody needs to trust.

use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use trunnion::broker::BrokerRun;
use trunnion::gateway::Pinning;
use trunnion::ledger::{self, InclusionBundle, Ledger};
use trunnion::policy::Policy;

fn workdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("trunnion-rp-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn repo_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

/// One audited file, one finding citing the read that produced it.
fn audited_ledger(dir: &Path) -> PathBuf {
    let target = dir.join("orders.py");
    fs::write(
        &target,
        "def get_order(order_id):\n    return Order.query.get(order_id)\n",
    )
    .unwrap();
    let pack = dir.join("pack.md");
    fs::write(&pack, "you are an audit agent").unwrap();

    let led = dir.join("ledger");
    let pin = Pinning {
        policy: repo_path("config/policy.json"),
        instructions: pack,
        settings: None,
        diverged: vec![],
        permission_mode: None,
    };
    let mut run = BrokerRun::open(
        Ledger::init(&led).unwrap(),
        Policy::load(&pin.policy).unwrap(),
        "repo-audit",
        &pin,
    )
    .unwrap();
    run.register_builtins().unwrap();
    let doc = run.call("Read", target.to_str().unwrap()).unwrap();
    run.finding(json!({
        "class": "authz.boundary",
        "path": target.to_str().unwrap(),
        "line": 2,
        "claim": "get_order returns any order by id with no ownership check, while its sibling refund path checks one",
        "asserted_by": "local/qwen3:0.6b",
        "evidence": [doc.event_id],
        "status": "asserted",
    }))
    .unwrap();
    run.seal("complete").unwrap();
    led
}

#[test]
fn a_finding_travels_with_a_bundle_that_verifies_against_the_key_beside_it() {
    let dir = workdir("verifies");
    let led = audited_ledger(&dir);
    let out = dir.join("out");

    let report = trunnion::report::write(&led, &out).unwrap();
    assert_eq!(report.findings, 1);
    assert_eq!(
        report.bundles, 2,
        "a finding is covered by its own bundle and by one for every event it cited; fewer means the claim or its evidence travels unproven"
    );

    let key = fs::read_to_string(out.join("ledger.pub")).unwrap();
    for stem in ["f-1.finding", "f-1.0-tool-result"] {
        let path = out.join(format!("proofs/{stem}.json"));
        let bundle: InclusionBundle =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        ledger::verify_bundle(&bundle, &key).unwrap_or_else(|f| {
            panic!("{stem} does not verify against the key shipped beside it: {f}")
        });
    }

    // An envelope carries subject_hash and not the subject, so a bundle alone
    // proves an entry existed and says nothing about whether the sentence
    // printed beside it is that entry. The subject travels with it and the
    // hash is recomputed, or "check it" points at a proof of the wrong thing.
    for stem in ["f-1.finding", "f-1.0-tool-result"] {
        let subject_path = out.join(format!("proofs/{stem}.subject.json"));
        let subject: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&subject_path).unwrap()).unwrap();
        let bundle: InclusionBundle = serde_json::from_str(
            &fs::read_to_string(out.join(format!("proofs/{stem}.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(
            trunnion::event::subject_hash(&subject).unwrap(),
            bundle.envelope.subject_hash,
            "{stem} ships a subject the proven entry did not commit to"
        );
    }

    let text = fs::read_to_string(out.join("report.md")).unwrap();
    assert!(
        text.contains("does not show that the finding is true"),
        "the document has to say what the proof is not, or a signature under a guess reads as a signature under a fact"
    );
    assert!(
        text.contains("trunnion ledger verify-inclusion proofs/f-1.finding.json ledger.pub proofs/f-1.finding.subject.json"),
        "the recipient is told the exact command, or the proof is decoration: {text}"
    );
    let scope = text
        .split("## Findings")
        .next()
        .expect("the document has a scope section before its findings");
    assert!(
        scope.contains("orders.py") && !scope.contains("Files read: none"),
        "the scope section names the files actually read; saying none while a read happened understates the audit exactly where it is claiming to be careful: {scope}"
    );
}

/// A file the broker refused to read is not a file that was read. The two
/// sections must not contradict each other, because the scope list is what a
/// reader uses to decide what the audit did not cover.
#[test]
fn a_refused_read_is_a_refusal_and_never_a_file_this_audit_read() {
    let dir = workdir("refused");
    let secret = dir.join("id_rsa");
    fs::write(&secret, "-----BEGIN OPENSSH PRIVATE KEY-----\nAAAA\n").unwrap();
    let pack = dir.join("pack.md");
    fs::write(&pack, "you are an audit agent").unwrap();

    let led = dir.join("ledger");
    let mut run = BrokerRun::open(
        Ledger::init(&led).unwrap(),
        Policy::load(&repo_path("config/policy.json")).unwrap(),
        "repo-audit",
        &Pinning {
            policy: repo_path("config/policy.json"),
            instructions: pack,
            settings: None,
            diverged: vec![],
            permission_mode: None,
        },
    )
    .unwrap();
    run.register_builtins().unwrap();
    run.call("Read", secret.to_str().unwrap()).expect_err(
        "the credential rule denies this read; without the denial this test proves nothing",
    );
    run.seal("complete").unwrap();

    let out = dir.join("out");
    trunnion::report::write(&led, &out).unwrap();
    let text = fs::read_to_string(out.join("report.md")).unwrap();
    let scope = text.split("## Findings").next().unwrap();
    assert!(
        scope.contains("Files read: none"),
        "a read the policy denied delivered no bytes, so it cannot appear as a file this audit read: {scope}"
    );
    assert!(
        text.contains("**deny**") && text.contains("r-credential-file"),
        "the refusal section names the verdict and the rule, which is the one part of this document that is a fact rather than an assertion: {text}"
    );
}

#[test]
fn a_claim_edited_after_the_fact_fails_rather_than_reading_as_verified() {
    let dir = workdir("tampered");
    let led = audited_ledger(&dir);
    let out = dir.join("out");
    trunnion::report::write(&led, &out).unwrap();

    let key = fs::read_to_string(out.join("ledger.pub")).unwrap();
    let path = out.join("proofs/f-1.finding.json");
    let mut bundle: InclusionBundle =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert!(ledger::verify_bundle(&bundle, &key).is_ok());

    // The subject is stored by hash, so a recipient who wants to soften a
    // finding has to move the hash the envelope commits to. That is the edit
    // the tree catches.
    // The replacement character is chosen against the one that is there. A
    // fixed digit edits nothing one time in sixteen, and a test that passes
    // because it made no change is the dead sensor this project exists to
    // catch; this one was caught by the gate on its first run.
    let hash = bundle.envelope.subject_hash.clone();
    let last = hash.chars().last().expect("a hash is not empty");
    bundle.envelope.subject_hash = format!(
        "{}{}",
        &hash[..hash.len() - 1],
        match last {
            '0' => '1',
            _ => '0',
        }
    );
    let fault = ledger::verify_bundle(&bundle, &key).expect_err(
        "an altered envelope must not verify; a proof that survives an edit proves nothing",
    );
    let text = fault.to_string();
    assert!(
        text.contains("entry") || text.contains("leaf") || text.contains("proof"),
        "the refusal names what failed so the recipient knows the record was altered rather than mislaid: {text}"
    );
}

#[test]
fn a_finding_citing_an_event_from_another_log_is_refused_rather_than_half_proved() {
    let dir = workdir("foreign");
    let target = dir.join("requirements.txt");
    fs::write(&target, "requests\n").unwrap();
    let pack = dir.join("pack.md");
    fs::write(&pack, "you are an audit agent").unwrap();

    let led = dir.join("ledger");
    let mut run = BrokerRun::open(
        Ledger::init(&led).unwrap(),
        Policy::load(&repo_path("config/policy.json")).unwrap(),
        "repo-audit",
        &Pinning {
            policy: repo_path("config/policy.json"),
            instructions: pack,
            settings: None,
            diverged: vec![],
            permission_mode: None,
        },
    )
    .unwrap();
    run.register_builtins().unwrap();
    run.finding(json!({
        "class": "dependency.provenance",
        "path": target.to_str().unwrap(),
        "line": 1,
        "claim": "requests is unpinned and no lockfile fixes it",
        "asserted_by": "local/qwen3:0.6b",
        "evidence": ["some-other-run-4"],
        "status": "asserted",
    }))
    .unwrap();
    run.seal("complete").unwrap();

    let fault = trunnion::report::write(&led, &dir.join("out"))
        .expect_err("a report must not quietly ship a finding whose evidence is not in the log it was built from");
    assert!(
        fault.to_string().contains("some-other-run-4"),
        "the refusal names the citation it could not cover: {fault}"
    );
}
