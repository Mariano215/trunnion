//! The remediation queue and the brief it prints.
//!
//! The ordering here is a port of `remediation_rank` in harness-kit's
//! `report/render.py`, which is the reference implementation of spec
//! 0.2.0-draft section 2.2. Two implementations of one rule drift, so the
//! expected orders below are the Python's actual output for all three risk
//! levels, captured by running it rather than by reading it. A port that
//! disagrees with the thing it was ported from is the port being wrong, and
//! this is the test that says so.

use trunnion::remediate::{self, KEYS};
use trunnion::scan::{Finding, ScanReport};
use trunnion::workspace::Risk;

/// Produced by, in harness-kit:
///
/// ```text
/// sorted(keys, key=lambda k: (render.remediation_rank(k, keys, risk), keys.index(k)))
/// ```
const INTERNAL: [&str; 12] = [
    "execution_environment",
    "tool_interface",
    "context_management",
    "durable_state",
    "orchestration",
    "instruction",
    "context_delivery",
    "sub_agents",
    "skills",
    "verification",
    "observability",
    "governance",
];

/// Client facing and regulated agree: once the work leaves the building, the
/// trust layer rises above everything the ablation order would have put first.
const LEAVES_THE_BUILDING: [&str; 12] = [
    "execution_environment",
    "verification",
    "observability",
    "governance",
    "tool_interface",
    "context_management",
    "durable_state",
    "orchestration",
    "instruction",
    "context_delivery",
    "sub_agents",
    "skills",
];

fn ordered(risk: Risk) -> Vec<&'static str> {
    let all: Vec<&str> = KEYS.to_vec();
    let mut keys: Vec<&'static str> = KEYS.to_vec();
    keys.sort_by_key(|k| {
        (
            remediate::rank(k, risk, &all),
            KEYS.iter().position(|x| x == k).unwrap_or(0),
        )
    });
    keys
}

#[test]
fn the_order_matches_the_python_it_was_ported_from() {
    assert_eq!(ordered(Risk::Internal), INTERNAL, "internal");
    assert_eq!(
        ordered(Risk::ClientFacing),
        LEAVES_THE_BUILDING,
        "client_facing"
    );
    assert_eq!(ordered(Risk::Regulated), LEAVES_THE_BUILDING, "regulated");
}

/// A report where every primitive is on the floor, so the queue is the whole
/// twelve and the ordering is the only thing under test.
fn floored() -> ScanReport {
    let names = [
        "Instruction",
        "Context delivery",
        "Context management",
        "Tool interface",
        "Execution environment",
        "Durable state",
        "Orchestration",
        "Sub-agents",
        "Skills",
        "Verification",
        "Observability",
        "Governance",
    ];
    ScanReport {
        root: "/tmp/example".to_string(),
        findings: names
            .iter()
            .enumerate()
            .map(|(i, name)| Finding {
                primitive: (i + 1) as u8,
                name,
                score: 0,
                evidence: "looked in nothing: found nothing".to_string(),
                gap: "to reach 2, something has to exist".to_string(),
            })
            .collect(),
        overall: 0,
        checks_read: Vec::new(),
        markers: Vec::new(),
    }
}

#[test]
fn the_queue_is_the_ranked_order_and_the_target_is_the_next_prescribed_level() {
    let report = floored();
    let gaps = remediate::gaps(&report, Risk::Internal);
    assert_eq!(gaps.len(), 12, "every primitive on the floor is a gap");
    let keys: Vec<&str> = gaps.iter().map(|g| g.key).collect();
    assert_eq!(keys, INTERNAL);
    // Levels 0 to 2 describe not having done the thing, so nothing is
    // prescribed for them. Aiming a brief at 2 would produce a brief with no
    // requirement, no artifact and no acceptance in it.
    assert!(
        gaps.iter().all(|g| g.target == 3),
        "the floor is asked for the first level anything is written for"
    );
}

#[test]
fn a_primitive_already_above_the_next_level_is_not_in_the_queue() {
    let mut report = floored();
    report.findings[6].score = 3; // orchestration
    let gaps = remediate::gaps(&report, Risk::Internal);
    assert_eq!(gaps.len(), 11);
    assert!(
        !gaps.iter().any(|g| g.key == "orchestration"),
        "a primitive already at 3 is not asked for 3"
    );
}

#[test]
fn the_brief_quotes_the_contract_rather_than_summarising_it() {
    let report = floored();
    let doc = remediate::document(&report, Risk::Regulated, "example").unwrap();

    // The contract's own words for the level asked for, not a paraphrase.
    let contracts = remediate::Contracts::load().unwrap();
    let tools = contracts.get("tool_interface").unwrap();
    // Only 3 and 4 carry requirements, which is why the queue aims at those.
    assert!(
        !tools.targets.contains_key("2"),
        "levels below 3 describe not having done the thing"
    );
    let three = tools.targets.get("3").expect("level 3 is prescribed");
    assert!(
        doc.contains(&three.check),
        "the acceptance line is the contract's own test, not a restatement of it"
    );

    // Every section the brief promises is present for at least one gap, and
    // the acceptance line is the contract's test rather than a restatement.
    assert!(doc.contains("WHAT IS THERE NOW"), "{doc}");
    assert!(doc.contains("RULES FOR THE WORK"), "{doc}");
    assert!(
        doc.contains("contracts 0.2.0 against spec 0.2.0-draft"),
        "the brief says which contracts it quoted: {}",
        doc.lines().take(4).collect::<Vec<_>>().join(" | ")
    );
    // Regulated work puts the trust layer immediately after execution
    // environment, and the document is printed in queue order.
    let exec = doc.find("Execution environment").unwrap();
    let verify = doc.find("Verification").unwrap();
    let tool = doc.find("Tool interface").unwrap();
    assert!(
        exec < verify && verify < tool,
        "regulated work audits before it capabilities"
    );
}

/// A repository at the static ceiling is not finished, it is at the point
/// where the remaining work is enforcement rather than artifacts. The queue
/// asks all twelve for 4, which is what the contracts prescribe there.
#[test]
fn a_repository_at_the_static_ceiling_is_asked_for_enforcement() {
    let mut report = floored();
    for f in report.findings.iter_mut() {
        f.score = 3;
    }
    report.overall = 3;
    let gaps = remediate::gaps(&report, Risk::Internal);
    assert_eq!(
        gaps.len(),
        12,
        "everything at 3 still has a level 4 to reach"
    );
    assert!(gaps.iter().all(|g| g.target == 4));

    let doc = remediate::document(&report, Risk::Internal, "example").unwrap();
    assert!(
        doc.contains("REQUIREMENT FOR LEVEL 4"),
        "the brief quotes the level 4 contract: {}",
        &doc[..400.min(doc.len())]
    );
}

/// Above 4 the contracts prescribe nothing, because level 5 is emergent. A
/// queue that invented work there would be inventing it.
#[test]
fn nothing_is_prescribed_above_the_level_the_contracts_carry() {
    let mut report = floored();
    for f in report.findings.iter_mut() {
        f.score = 4;
    }
    report.overall = 4;
    let doc = remediate::document(&report, Risk::Internal, "example").unwrap();
    assert!(doc.contains("0 gap(s)"), "{doc}");
    assert!(
        doc.contains("run trunnion score over a ledger"),
        "the empty state names what actually moves the number: {doc}"
    );
}
