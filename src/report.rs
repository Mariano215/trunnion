//! The deliverable. A repository audit ends in a document a third party can
//! check without trusting the party who handed it over, which is the only
//! thing that makes an agent's output worth reading: every finding travels
//! with an inclusion bundle proving the claim was on a signed log, and beside
//! it the bundle for the read it rests on.
//!
//! What the proof is worth is stated in the document rather than left for the
//! reader to assume. It shows which bytes the agent read, which model was
//! asked, under which policy and pack, and that none of that record has
//! changed since it was written. It does not show that the finding is true. A
//! document that blurred those two would be worse than no document, because
//! it would put a signature under a guess.

use crate::ledger::{InclusionBundle, Ledger};
use crate::Fault;
use serde_json::Value;
use std::fs;
use std::path::Path;

/// One finding, with the bundles that cover it. Each bundle travels with the
/// subject it commits to, because an envelope carries `subject_hash` and not
/// the content: a recipient holding the bundle alone can prove an entry was in
/// the log and cannot tell whether the sentence printed beside it is that
/// entry. Shipping the subject closes that, and `verify-inclusion` recomputes
/// the hash rather than taking the pairing on trust.
struct Cited {
    /// Its number in the document, assigned in log order.
    id: String,
    /// The ledger event the claim is, which is the identity that survives
    /// outside this document.
    event_id: String,
    subject: Value,
    /// (filename stem, bundle, subject) triples: the finding itself first,
    /// then the events it cited, in the order it cited them.
    bundles: Vec<(String, InclusionBundle, Value)>,
}

#[derive(Debug)]
pub struct Report {
    pub findings: usize,
    pub bundles: usize,
    pub refusals: usize,
}

/// Read a sealed ledger and write the deliverable into `out_dir`.
pub fn write(ledger_dir: &Path, out_dir: &Path) -> Result<Report, Fault> {
    let ledger = Ledger::open(ledger_dir)?;
    let events = ledger.events_with_subjects()?;

    // Position is index: `events_with_subjects` returns the envelopes in
    // append order, which is the order the tree was built in.
    let index_of =
        |id: &str| -> Option<usize> { events.iter().position(|e| e["id"].as_str() == Some(id)) };

    let mut cited: Vec<Cited> = Vec::new();
    for (at, event) in events.iter().enumerate() {
        if event["kind"].as_str() != Some("audit.finding") {
            continue;
        }
        let subject = event["_subject"].clone();
        // Findings are numbered in log order for the document. The identity a
        // finding actually has is its event id, which the ledger already
        // guarantees is unique; a second id minted per run collided the moment
        // an audit ran over more than one file, and two things called f-1 in
        // one report is worse than no name at all.
        let fid = format!("f-{}", cited.len() + 1);
        let event_id = event["id"].as_str().unwrap_or("(unrecorded)").to_string();
        let mut bundles = vec![(format!("{fid}.finding"), ledger.prove(at)?, subject.clone())];
        for (n, ev_id) in subject["evidence"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            let ev_id = ev_id.as_str().unwrap_or_default();
            let Some(evidence_at) = index_of(ev_id) else {
                // A cited event that is not in this log is a hole in the
                // claim, and printing the finding without saying so would
                // hand the reader a proof set that quietly covers less than
                // the document says it does.
                return Err(Fault::new(
                    format!("finding {fid} cites event {ev_id}, which is not in {}", ledger_dir.display()),
                    "report the ledger the audit wrote; a finding's evidence and the finding itself are in one log by construction, so this means the wrong directory or an edited record",
                ));
            };
            let kind = events[evidence_at]["kind"]
                .as_str()
                .unwrap_or("event")
                .replace('.', "-");
            bundles.push((
                format!("{fid}.{n}-{kind}"),
                ledger.prove(evidence_at)?,
                events[evidence_at]["_subject"].clone(),
            ));
        }
        cited.push(Cited {
            id: fid,
            event_id,
            subject,
            bundles,
        });
    }

    let proofs_dir = out_dir.join("proofs");
    fs::create_dir_all(&proofs_dir).map_err(|e| {
        Fault::new(
            format!("cannot create {}: {e}", proofs_dir.display()),
            "choose an output directory this process can write to; the audit itself writes nothing to the repository it read",
        )
    })?;

    let mut written = 0usize;
    for finding in &cited {
        for (stem, bundle, subject) in &finding.bundles {
            let text = serde_json::to_string_pretty(bundle).map_err(|e| {
                Fault::new(
                    format!("an inclusion bundle did not serialise: {e}"),
                    "report this as a bug; InclusionBundle is serialisable by construction",
                )
            })?;
            write_file(&proofs_dir.join(format!("{stem}.json")), &text)?;
            let subject_text = serde_json::to_string_pretty(subject).map_err(|e| {
                Fault::new(
                    format!("a subject did not serialise: {e}"),
                    "report this as a bug; a subject read off the ledger is JSON already",
                )
            })?;
            write_file(
                &proofs_dir.join(format!("{stem}.subject.json")),
                &subject_text,
            )?;
            written += 1;
        }
    }

    let pub_key = fs::read_to_string(ledger_dir.join("keys/ledger.pub")).map_err(|e| {
        Fault::new(
            format!("cannot read the ledger public key: {e}"),
            "the report travels with the key its proofs check against; without it the bundles are unverifiable by the recipient",
        )
    })?;
    write_file(&out_dir.join("ledger.pub"), &pub_key)?;

    let refusals = refusals(&events);
    let markdown = document(&cited, &events, &refusals)?;
    write_file(&out_dir.join("report.md"), &markdown)?;

    Ok(Report {
        findings: cited.len(),
        bundles: written,
        refusals: refusals.len(),
    })
}

fn write_file(path: &Path, text: &str) -> Result<(), Fault> {
    fs::write(path, text).map_err(|e| {
        Fault::new(
            format!("cannot write {}: {e}", path.display()),
            "choose an output directory this process can write to",
        )
    })
}

/// Every call the policy stopped. Read off `policy.decision`, so this section
/// is the one part of the document that is a fact about what happened rather
/// than an assertion about what the code contains.
fn refusals(events: &[Value]) -> Vec<(String, String, String)> {
    events
        .iter()
        .filter(|e| e["kind"].as_str() == Some("policy.decision"))
        .filter_map(|e| {
            let s = &e["_subject"];
            let verdict = s["verdict"].as_str()?;
            if verdict != "deny" && verdict != "hold" {
                return None;
            }
            Some((
                verdict.to_string(),
                s["rule"].as_str().unwrap_or("(unnamed rule)").to_string(),
                s["message"]
                    .as_str()
                    .or_else(|| s["capability"].as_str())
                    .unwrap_or("")
                    .to_string(),
            ))
        })
        .collect()
}

fn document(
    cited: &[Cited],
    events: &[Value],
    refusals: &[(String, String, String)],
) -> Result<String, Fault> {
    let mut out = String::new();
    out.push_str("# Repository audit\n\n");
    out.push_str(
        "Every finding below is a model's assertion about code it was shown. The proof beside it\n\
         shows which bytes the agent read, which model was asked, under which policy and\n\
         instruction pack, and that no part of that record has changed since it was written. It\n\
         does not show that the finding is true. Nothing here says the repository is secure: a\n\
         class not listed in scope was not looked for, and a file not listed as read was not\n\
         read.\n\n",
    );

    out.push_str("## Scope\n\n");
    out.push_str(
        "Classes in scope: secrets, dependency provenance, authorisation boundaries. Nothing\n\
         else was looked for, and no vulnerability database was consulted, so a version number\n\
         here is evidence of a version and not of a vulnerability.\n\n",
    );
    // A file the broker was asked to read and refused is not a file that was
    // read, so the list is built from requests whose result came back ok. The
    // refusals have their own section, and a path appearing in both would say
    // the audit saw something it was stopped from seeing.
    let delivered: Vec<&str> = events
        .iter()
        .filter(|e| e["kind"].as_str() == Some("tool.result"))
        .filter(|e| e["_subject"]["outcome"].as_str() == Some("ok"))
        .filter_map(|e| e["_subject"]["request_id"].as_str())
        .collect();
    let read: Vec<String> = events
        .iter()
        .filter(|e| e["kind"].as_str() == Some("tool.request"))
        .filter(|e| e["_subject"]["tool"].as_str() == Some("Read"))
        .filter(|e| {
            e["_subject"]["request_id"]
                .as_str()
                .is_some_and(|id| delivered.contains(&id))
        })
        .filter_map(|e| e["_subject"]["args"]["path"].as_str().map(str::to_string))
        .collect();
    out.push_str(&format!(
        "Files read: {}. A file not on this list was not read, and this report says nothing\n\
         about it.\n\n",
        match read.is_empty() {
            true => "none".to_string(),
            false => read.join(", "),
        }
    ));

    out.push_str("## Findings\n\n");
    if cited.is_empty() {
        out.push_str(
            "None. The audit ran and asserted nothing, which is a different statement from the\n\
             audit not having run: the calls it made are on the log this report was built from.\n\n",
        );
    }
    for finding in cited {
        let s = &finding.subject;
        let fid = &finding.id;
        let line = match s["line"].as_u64() {
            Some(n) => format!(":{n}"),
            None => String::new(),
        };
        out.push_str(&format!(
            "### {fid}  {}  {}{line}\n\n{}\n\n",
            s["class"].as_str().unwrap_or("(no class)"),
            s["path"].as_str().unwrap_or("(no path)"),
            s["claim"].as_str().unwrap_or("(no claim)"),
        ));
        out.push_str(&format!(
            "Asserted by {}, status {}, recorded as event {}.\n\n",
            s["asserted_by"].as_str().unwrap_or("(unrecorded)"),
            s["status"].as_str().unwrap_or("asserted"),
            finding.event_id,
        ));
        out.push_str("Check it:\n\n```\n");
        for (stem, _, _) in &finding.bundles {
            out.push_str(&format!(
                "trunnion ledger verify-inclusion proofs/{stem}.json ledger.pub proofs/{stem}.subject.json\n"
            ));
        }
        out.push_str(
            "```\n\nEach line proves two things: that the entry was in the signed log, and that\n\
             the subject file beside it is the content that entry committed to. Without the\n\
             third argument the command answers only the first, which is a weaker statement\n\
             than this document needs.\n\n",
        );
    }

    out.push_str("## What the harness refused during this audit\n\n");
    if refusals.is_empty() {
        out.push_str("Nothing. No call in this run was denied or held.\n\n");
    } else {
        for (verdict, rule, message) in refusals {
            out.push_str(&format!("- **{verdict}** under `{rule}`. {message}\n"));
        }
        out.push('\n');
    }

    out.push_str("## What this record is worth\n\n");
    let open = events
        .iter()
        .find(|e| e["kind"].as_str() == Some("run.open"));
    let profile = open
        .and_then(|e| e["_subject"]["profile"].as_str())
        .unwrap_or("(unrecorded)");
    let anchored = events
        .iter()
        .any(|e| e["kind"].as_str() == Some("ledger.anchor"));
    out.push_str(&format!(
        "Profile {profile}. The signing key is held by the machine that ran the audit, so a\n\
         verified signature says which run wrote an event and never who operated it. Anchoring:\n\
         {}. Without an anchor held by someone else, an inclusion proof is integrity relative to\n\
         a head this machine signed, which catches an edit and does not catch a writer who\n\
         rewrote its own log and re-signed the result.\n",
        match anchored {
            true => "a head from this log was copied outside it, and the copy is what a rewrite would fail against",
            false => "none",
        }
    ));

    Ok(out)
}
