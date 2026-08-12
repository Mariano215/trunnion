//! Chokepoint invariant: every provider call goes through the gateway. A
//! direct ureq call anywhere else in src/ or tests/ is a hole in primitive
//! 11, so this test greps for the literal and fails the build if it turns
//! up outside src/gateway.rs.
use std::fs;
use std::path::{Path, PathBuf};

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn ureq_only_in_gateway() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let allowed = root.join("src").join("gateway.rs");
    // This file names the invariant in its own doc comment and assertion
    // message, so it is excluded from its own scan rather than matched.
    let self_file = root.join("tests").join("invariants.rs");

    let mut files = Vec::new();
    files_under(&root.join("src"), &mut files);
    files_under(&root.join("tests"), &mut files);

    for f in files {
        if f == allowed || f == self_file {
            continue;
        }
        let text = fs::read_to_string(&f).unwrap_or_default();
        assert!(
            !text.contains("ureq"),
            "every provider call goes through the gateway, but {} references ureq directly",
            f.display()
        );
    }
}

/// An arrow asserts a handoff, and an event records one end of it. A table
/// mapping an event kind to a source and a destination lane would make every
/// event an arrow, and the diagram would be complete and partly untrue, which
/// is the defect this project exists to prevent one layer down from a scorer
/// that reads configuration. Every peer in trace.js is read out of a subject
/// field, so this asserts the shape that keeps it that way.
#[test]
fn the_trace_view_derives_no_edge_from_an_event_kind_alone() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = fs::read_to_string(root.join("assets/trace.js")).unwrap();
    let peers = src
        .split_once("const PEER_FIELD = {")
        .expect("trace.js declares PEER_FIELD, the one place a peer is read")
        .1
        .split_once("};")
        .expect("PEER_FIELD is a closed object literal")
        .0;
    let mut entries = 0;
    for line in peers.lines().filter(|l| l.contains("=>")) {
        entries += 1;
        assert!(
            line.contains("s."),
            "every PEER_FIELD entry reads a subject field, and this one does not: {line}"
        );
    }
    assert!(
        entries > 0,
        "PEER_FIELD is empty, so this test asserted nothing. Fix: it is the one place a peer is read and it should list every kind whose producer records one"
    );
    // The rendered edge count states both halves, so a reader knows what the
    // picture refused to draw and not only what it drew.
    assert!(
        src.contains("inferred: 0"),
        "the legend prints the inferred count, which is zero by construction and printed anyway"
    );
}

/// Prescription and scoring stay apart, and not by intention.
///
/// `contracts.yaml` says trunnion does not read it and must not. The reason is
/// the split the two projects are built on: harness-kit refuses to infer a
/// level, trunnion refuses to prescribe one. trunnion vendors the contracts anyway,
/// because a remediation brief has to quote the requirement in the words that
/// defined it, and the rule survives only while nothing that produces a number
/// reads them. A scorer that started ranking against contract text would be a
/// scorer measuring a document instead of a system, which is the failure both
/// projects exist to avoid, and it would be one import away and invisible in
/// review.
#[test]
fn nothing_that_produces_a_score_reads_the_vendored_contracts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let self_file = root.join("tests").join("invariants.rs");
    // The two scorers, static and telemetry. Neither may reach the contracts.
    for module in ["scan.rs", "scorer.rs"] {
        let path = root.join("src").join(module);
        let text = fs::read_to_string(&path).unwrap();
        for forbidden in ["remediate", "contracts.json", "CONTRACTS_JSON"] {
            assert!(
                !text.contains(forbidden),
                "src/{module} produces a score and references {forbidden}; the contracts say what to build, never what a level is worth",
            );
        }
    }
    // And the contracts are read in exactly one place, so the rule above has
    // one file to hold rather than a habit to maintain.
    let mut files = Vec::new();
    files_under(&root.join("src"), &mut files);
    let readers: Vec<String> = files
        .iter()
        .filter(|f| **f != self_file)
        .filter(|f| {
            fs::read_to_string(f)
                .unwrap_or_default()
                .contains("contracts.json")
        })
        .map(|f| f.display().to_string())
        .collect();
    assert_eq!(
        readers.len(),
        1,
        "the vendored contracts are read in one place, src/remediate.rs, and these read them: {readers:?}"
    );
}

/// The operations topology draws declared architecture, and must not quietly
/// become a second claim about observed flow.
///
/// The trace view is the one picture allowed to assert a handoff, and it may
/// only do so where a producer recorded a peer. The operations topology exists
/// for a different job: it shows the shape of the system with live counts on
/// the nodes. That is honest exactly as long as its edges stay a fixed table
/// and never key off an event kind, because an arrow derived from a kind would
/// be an inference wearing the same ink as the trace view's observations.
///
/// So: the edge list is a declared constant of node-name pairs, every endpoint
/// resolves to a declared node, and the panel says on screen that the edges are
/// declared. The counts are the only part read from the log, and they arrive
/// from `/api/operations` already computed.
#[test]
fn the_operations_topology_declares_its_edges_and_derives_none_from_an_event_kind() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = fs::read_to_string(root.join("assets/views.js")).unwrap();

    let nodes_block = src
        .split_once("const TOPO_NODES = {")
        .expect("views.js declares TOPO_NODES, the topology's declared layout")
        .1
        .split_once("};")
        .expect("TOPO_NODES is a closed object literal")
        .0;
    let node_names: Vec<String> = nodes_block
        .lines()
        .filter_map(|l| l.split_once('\'').and_then(|(_, r)| r.split_once('\'')))
        .map(|(name, _)| name.to_string())
        .collect();
    assert!(
        node_names.len() >= 2,
        "TOPO_NODES is empty or unparsed, so this test asserted nothing"
    );

    let edges_block = src
        .split_once("const TOPO_EDGES = [")
        .expect("views.js declares TOPO_EDGES, the one place an edge comes from")
        .1
        .split_once("];")
        .expect("TOPO_EDGES is a closed array literal")
        .0;
    let mut edges = 0;
    for line in edges_block.lines().filter(|l| l.contains('[')) {
        edges += 1;
        // Both endpoints are declared node names. An entry naming an event
        // kind, a subject field or anything read from the log would mean the
        // picture had started deriving its arrows from the record.
        let named: Vec<&str> = line
            .split('\'')
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, s)| s)
            .collect();
        assert_eq!(
            named.len(),
            2,
            "every edge is a pair of declared node names: {line}"
        );
        for end in named {
            assert!(
                node_names.iter().any(|n| n == end),
                "edge endpoint {end:?} is not a declared node, so the layout and the edge table disagree: {line}"
            );
            assert!(
                !end.contains('.'),
                "edge endpoint {end:?} looks like an event kind, and an edge derived from a kind is an inference the trace view exists to refuse"
            );
        }
    }
    assert!(
        edges > 0,
        "TOPO_EDGES is empty, so this test asserted nothing"
    );

    // Said on screen, not only in the source, because the reader is who the
    // distinction protects.
    assert!(
        src.contains("derived from the record"),
        "the topology panel states how many of its edges came from the log, which is none and is printed anyway"
    );
}
