//! Slice 10 integration: the read-only console API of `docs/CONSOLE-API.md`,
//! exercised over a real loopback socket against a fixture ledger. Binding
//! loopback is what `tests/sandbox.rs` already does and it is not a route out:
//! the listener is this process, so the suite stays offline.

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::{fs, thread};
use trunnion::console;
use trunnion::event::NewEvent;
use trunnion::ledger::Ledger;

// -- fixture ----------------------------------------------------------------

fn workdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("trunnion-console-it-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn event(run_id: &str, seq: u64, ts: &str, kind: &str, subject: Value) -> NewEvent {
    NewEvent {
        id: format!("{run_id}-{seq}"),
        run_id: run_id.to_string(),
        parent_id: None,
        seq,
        ts: ts.to_string(),
        kind: kind.to_string(),
        actor: json!({"type": "system", "id": "system:broker", "identity_source": "local", "rung": null}),
        authority: json!({"policy_version": "sha256:fixture", "diverged": []}),
        subject,
        redacted: vec![],
        attestation: None,
    }
}

/// Two runs: one sealed with a denial, a clean capability run and a
/// promotion; one that never sealed. The unsealed run is deliberate, because
/// the API must show the seam rather than hide it.
fn fixture_ledger(name: &str) -> PathBuf {
    let dir = workdir(name).join("ledger");
    let mut ledger = Ledger::init(&dir).unwrap();
    for ev in [
        event(
            "run-1000",
            0,
            "2026-08-05T09:14:01.000Z",
            "run.open",
            json!({"workload": "repo-audit", "restored_checkpoint": null}),
        ),
        event(
            "run-1000",
            1,
            "2026-08-05T09:14:02.000Z",
            "model.call",
            json!({"provider": "fixture", "tokens": 12}),
        ),
        event(
            "run-1000",
            2,
            "2026-08-05T09:14:03.000Z",
            "policy.decision",
            json!({
                "verdict": "deny",
                "capability": "repo.write",
                "rule": "r-destructive-shell",
                "message": "This command is destructive. Run it by hand if you mean it.",
            }),
        ),
        event(
            "run-1000",
            3,
            "2026-08-05T09:14:04.000Z",
            "capability.run",
            json!({"capability": "repo.write", "outcome": "clean"}),
        ),
        event(
            "run-1000",
            4,
            "2026-08-05T09:14:05.000Z",
            "rung.change",
            json!({
                "capability": "repo.write",
                "from": "assisted",
                "to": "autonomous",
                "trigger": "earned",
                "approver": "user:mariano@local",
            }),
        ),
        event(
            "run-1000",
            5,
            "2026-08-05T09:14:06.000Z",
            "run.seal",
            json!({"outcome": "complete", "event_count": 6}),
        ),
        event(
            "run-2000",
            0,
            "2026-08-05T10:00:00.000Z",
            "run.open",
            json!({"workload": "unsealed-audit", "restored_checkpoint": null}),
        ),
        event(
            "run-2000",
            1,
            "2026-08-05T10:00:01.000Z",
            "tool.request",
            json!({"tool": "Read", "target": "README.md"}),
        ),
    ] {
        ledger.append(ev).unwrap();
    }
    dir
}

/// Four held calls, one per state the inbox has to keep apart: nobody looked,
/// somebody said no, somebody said yes, and a grant already spent by a retry
/// with the call held again after it. The broker emits tool.request and then
/// exactly one policy.decision, so the order here is the order the pairing
/// depends on.
fn hold_ledger(name: &str) -> PathBuf {
    let dir = workdir(name).join("ledger");
    let mut ledger = Ledger::init(&dir).unwrap();
    let mut seq = 0u64;
    let mut events: Vec<NewEvent> = Vec::new();
    let held = |call: &str, target: &str, req: &str, s: &mut u64| {
        let base = *s;
        *s += 2;
        vec![
            event(
                "run-3000",
                base,
                &format!("2026-08-05T11:00:{base:02}.000Z"),
                "tool.request",
                json!({"request_id": req, "call_hash": call, "tool": "Bash", "args": {"command": target}}),
            ),
            event(
                "run-3000",
                base + 1,
                &format!("2026-08-05T11:00:{:02}.000Z", base + 1),
                "policy.decision",
                json!({
                    "verdict": "hold",
                    "capability": "vcs.publish",
                    "rule": "r-publish",
                    "message": "This call gates pre and needs an approval event before it can proceed.",
                    "request": {"tool": "Bash", "target": target},
                }),
            ),
        ]
    };
    events.extend(held(
        "sha256:aaa",
        "git push origin main",
        "req-a",
        &mut seq,
    ));
    events.extend(held(
        "sha256:bbb",
        "git push origin release",
        "req-b",
        &mut seq,
    ));
    events.extend(held(
        "sha256:ccc",
        "git push origin docs",
        "req-c",
        &mut seq,
    ));
    events.extend(held(
        "sha256:ddd",
        "git push origin spent",
        "req-d",
        &mut seq,
    ));
    for (grant, verdict, call) in [
        ("g-b", "deny", "sha256:bbb"),
        ("g-c", "approve", "sha256:ccc"),
        ("g-d", "approve", "sha256:ddd"),
    ] {
        events.push(event(
            "run-3000",
            seq,
            &format!("2026-08-05T11:00:{seq:02}.000Z"),
            "approval",
            json!({
                "grant_id": grant,
                "verdict": verdict,
                "call_hash": call,
                "rule": "r-publish",
                "approver": "user:mariano@local",
                "approver_source": "local",
                "request_id": "req-x",
            }),
        ));
        seq += 1;
    }
    // The spent grant, and the same call held again after it.
    events.push(event(
        "run-3000",
        seq,
        &format!("2026-08-05T11:00:{seq:02}.000Z"),
        "approval.use",
        json!({"grant_id": "g-d", "call_hash": "sha256:ddd", "rule": "r-publish", "approver": "user:mariano@local", "self_approved": true}),
    ));
    seq += 1;
    events.extend(held(
        "sha256:ddd",
        "git push origin spent",
        "req-d2",
        &mut seq,
    ));
    for ev in events {
        ledger.append(ev).unwrap();
    }
    dir
}

// -- a client, because the suite may not reach for an HTTP crate ------------

struct Reply {
    status: u16,
    content_type: String,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("body is not JSON ({e}): {}", self.body))
    }
}

fn raw(addr: SocketAddr, request: &str) -> Reply {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    stream.flush().unwrap();
    let mut text = String::new();
    stream.read_to_string(&mut text).unwrap();
    let (head, body) = text.split_once("\r\n\r\n").expect("no header terminator");
    let status: u16 = head
        .lines()
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|s| s.parse().ok())
        .expect("no status code");
    let content_type = head
        .lines()
        .find_map(|l| l.strip_prefix("content-type: "))
        .unwrap_or_default()
        .trim()
        .to_string();
    Reply {
        status,
        content_type,
        body: body.to_string(),
    }
}

fn get(addr: SocketAddr, target: &str) -> Reply {
    raw(
        addr,
        &format!("GET {target} HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n"),
    )
}

/// Starts the server on an ephemeral loopback port and returns its address.
/// The thread is left running: the test binary exits and takes it with it.
fn serve(ledger: &Path) -> SocketAddr {
    let listener = console::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let dir = ledger.display().to_string();
    thread::spawn(move || console::serve_on(&listener, Some(&dir)));
    addr
}

// -- the routes -------------------------------------------------------------

#[test]
fn every_route_answers_from_the_ledger_in_the_contracted_shape() {
    let ledger = fixture_ledger("routes");
    let addr = serve(&ledger);

    // GET /api/score
    let score = get(addr, "/api/score");
    assert_eq!(score.status, 200);
    assert_eq!(score.content_type, "application/json; charset=utf-8");
    let v = score.json();
    assert!(v["scores"].is_array(), "score has no scores array: {v}");
    assert!(v.get("overall").is_some(), "score has no overall: {v}");
    assert!(v["rules_version"].is_string());
    assert_eq!(v["events_scored"], json!(8));

    // GET /api/head
    let head = get(addr, "/api/head").json();
    assert_eq!(head["size"], json!(8));
    for field in ["root_hash", "ts", "key_id", "sig"] {
        assert!(head[field].is_string(), "head lacks {field}: {head}");
    }

    // GET /api/events
    let events = get(addr, "/api/events").json();
    assert_eq!(events["total"], json!(8));
    assert_eq!(events["returned"], json!(8));
    assert_eq!(events["offset"], json!(0));
    let first = &events["events"][0];
    for field in [
        "v",
        "id",
        "run_id",
        "seq",
        "ts",
        "kind",
        "actor",
        "authority",
        "subject_hash",
        "redacted",
        "_subject",
        "_attestation_state",
    ] {
        assert!(
            first[field] != Value::Null || field == "parent_id",
            "event lacks {field}: {first}"
        );
    }
    assert_eq!(first["kind"], json!("run.open"));
    // Newest last, exactly as the ledger is appended.
    assert_eq!(events["events"][7]["kind"], json!("tool.request"));
    // No producer emits attestations yet, so every row is absent. The point
    // is that it says so per event rather than saying nothing.
    for ev in events["events"].as_array().unwrap() {
        assert!(
            matches!(
                ev["_attestation_state"].as_str(),
                Some("verified" | "unverified" | "forged" | "absent")
            ),
            "unexpected attestation state: {ev}"
        );
        assert_eq!(ev["_attestation_state"], json!("absent"));
    }

    // GET /api/events/:id
    let id = first["id"].as_str().unwrap().to_string();
    let one = get(addr, &format!("/api/events/{id}")).json();
    assert_eq!(one["event"]["id"], json!(id));
    assert_eq!(one["index"], json!(0));
    assert_eq!(one["tree_size"], json!(8));
    assert_eq!(one["event"]["_attestation_state"], json!("absent"));

    // GET /api/runs
    let runs = get(addr, "/api/runs").json();
    let runs = runs["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2);
    // Newest first: the unsealed run opened later.
    assert_eq!(runs[0]["run_id"], json!("run-2000"));
    assert_eq!(runs[0]["sealed"], json!(false));
    assert_eq!(runs[0]["sealed_at"], Value::Null);
    assert_eq!(runs[0]["workload"], json!("unsealed-audit"));
    let sealed = &runs[1];
    assert_eq!(sealed["run_id"], json!("run-1000"));
    assert_eq!(sealed["sealed"], json!(true));
    assert_eq!(sealed["sealed_at"], json!("2026-08-05T09:14:06.000Z"));
    assert_eq!(sealed["opened_at"], json!("2026-08-05T09:14:01.000Z"));
    assert_eq!(sealed["workload"], json!("repo-audit"));
    assert_eq!(sealed["events"], json!(6));
    assert_eq!(sealed["denials"], json!(1));
    assert_eq!(sealed["unattested"], json!(6));
    assert_eq!(sealed["kinds"]["policy.decision"], json!(1));
    assert_eq!(sealed["kinds"]["run.seal"], json!(1));

    // GET /api/policy
    let policy = get(addr, "/api/policy").json();
    assert_eq!(policy["profile"], json!("laptop"));
    assert!(policy["version"]
        .as_str()
        .unwrap_or_default()
        .starts_with("sha256:"));
    let caps = policy["capabilities"].as_array().unwrap();
    let repo_write = caps
        .iter()
        .find(|c| c["id"] == json!("repo.write"))
        .expect("repo.write is in the tracked policy");
    assert_eq!(repo_write["rung"], json!("assisted"));
    assert_eq!(repo_write["effect"], json!("write.local"));
    let rules = policy["rules"].as_array().unwrap();
    let fired = rules
        .iter()
        .find(|r| r["id"] == json!("r-destructive-shell"))
        .expect("r-destructive-shell is in the tracked policy");
    assert_eq!(fired["decision"], json!("deny"));
    assert_eq!(fired["fired"], json!(1));
    // A rule that never fired is listed, not hidden.
    assert!(
        rules.iter().any(|r| r["fired"] == json!(0)),
        "an unfired rule must still be shown: {rules:?}"
    );

    // GET /api/trust
    let trust = get(addr, "/api/trust").json();
    let cap = trust["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["capability"] == json!("repo.write"))
        .cloned()
        .expect("repo.write is in the tracked policy");
    assert_eq!(cap["declared_rung"], json!("assisted"));
    // Replayed from the ledger: the rung.change moved it, the config did not.
    assert_eq!(cap["earned_rung"], json!("autonomous"));
    assert_eq!(cap["clean_since_rung"], json!(0));
    let history = cap["history"].as_array().unwrap();
    assert_eq!(history.len(), 2);
    let change = history
        .iter()
        .find(|h| h["kind"] == json!("rung.change"))
        .unwrap();
    assert_eq!(change["from"], json!("assisted"));
    assert_eq!(change["to"], json!("autonomous"));
    assert_eq!(change["approver"], json!("user:mariano@local"));
    assert!(change["event_id"].is_string());

    // GET /api/verify
    let verify = get(addr, "/api/verify").json();
    assert_eq!(verify["ok"], json!(true));
    assert_eq!(verify["entries"], json!(8));
    assert_eq!(verify["attestations_verified"], json!(0));
    assert_eq!(verify["attestations_unverified"], json!(0));
    assert_eq!(verify["faults"], json!([]));
    assert_eq!(verify["head"]["size"], json!(8));
    let reproduce = verify["reproduce"].as_str().unwrap();
    assert!(
        reproduce.starts_with("trunnion ledger verify /"),
        "reproduce must be the runnable offline command: {reproduce}"
    );

    // A non-API path serves the console shell, so the front end routes itself.
    let shell = get(addr, "/ledger/run-1000");
    assert_eq!(shell.status, 200);
    assert_eq!(shell.content_type, "text/html; charset=utf-8");
    assert!(shell.body.contains("<!doctype html>"));
}

#[test]
fn the_events_filters_narrow_the_set_and_page_it() {
    let ledger = fixture_ledger("filters");
    let addr = serve(&ledger);

    let one_kind = get(addr, "/api/events?kind=policy.decision").json();
    assert_eq!(one_kind["total"], json!(1));
    assert_eq!(one_kind["events"][0]["kind"], json!("policy.decision"));

    // kind repeats into a set.
    let two_kinds = get(addr, "/api/events?kind=run.open&kind=run.seal").json();
    assert_eq!(two_kinds["total"], json!(3));

    let by_run = get(addr, "/api/events?run=run-2000").json();
    assert_eq!(by_run["total"], json!(2));

    let by_actor = get(addr, "/api/events?actor=system%3Abroker").json();
    assert_eq!(by_actor["total"], json!(8));
    assert_eq!(
        get(addr, "/api/events?actor=system%3Ascorer").json()["total"],
        json!(0)
    );

    // since is inclusive at the bound, and a whole-second bound must not
    // exclude an event that carries a fraction inside that second.
    let since = get(addr, "/api/events?since=2026-08-05T09:14:04Z").json();
    assert_eq!(since["total"], json!(5));
    assert_eq!(
        get(addr, "/api/events?since=2026-08-06").json()["total"],
        json!(0)
    );

    let paged = get(addr, "/api/events?limit=2&offset=3").json();
    assert_eq!(paged["total"], json!(8), "total counts the filtered set");
    assert_eq!(paged["returned"], json!(2));
    assert_eq!(paged["offset"], json!(3));
    assert_eq!(paged["events"][0]["kind"], json!("capability.run"));

    // Combined filters intersect.
    let combined = get(addr, "/api/events?run=run-1000&kind=run.seal").json();
    assert_eq!(combined["total"], json!(1));
}

#[test]
fn a_long_query_is_answered_whole_or_refused_never_truncated() {
    let ledger = fixture_ledger("longquery");
    let addr = serve(&ledger);

    // Well past the 1024-byte buffer the first server read. The last kind is
    // the one that matches, so a truncated read answers the wrong question.
    let mut target = String::from("/api/events?");
    for i in 0..300 {
        target.push_str(&format!("kind=filler.kind.{i:04}&"));
    }
    target.push_str("kind=policy.decision");
    assert!(target.len() > 4000);
    let long = get(addr, &target);
    assert_eq!(long.status, 200, "body: {}", long.body);
    let long = long.json();
    assert_eq!(long["total"], json!(1));
    assert_eq!(long["events"][0]["kind"], json!("policy.decision"));

    // Past the cap, the request is refused with a Fault rather than cut down
    // to a query that means something else.
    let mut huge = String::from("/api/events?");
    for i in 0..8000 {
        huge.push_str(&format!("kind=filler.kind.{i:06}&"));
    }
    huge.push_str("kind=run.open");
    let refused = raw(
        addr,
        &format!("GET {huge} HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n"),
    );
    assert_eq!(refused.status, 400, "body: {}", refused.body);
    let fault = refused.json();
    assert!(fault["cause"].is_string() && fault["fix"].is_string());
}

#[test]
fn the_inbox_names_every_held_call_and_what_the_record_says_about_it() {
    let ledger = hold_ledger("approvals");
    let addr = serve(&ledger);
    let a = get(addr, "/api/approvals").json();

    let holds = a["holds"].as_array().unwrap();
    // Four distinct calls, and the call held twice is one row and not two:
    // a grant binds to the call hash, so that is the unit an approver acts on.
    assert_eq!(holds.len(), 4, "holds: {holds:#?}");
    let by_hash = |h: &str| {
        holds
            .iter()
            .find(|x| x["call_hash"] == json!(h))
            .cloned()
            .unwrap_or_else(|| panic!("no hold for {h}"))
    };

    // Nobody looked.
    let a_hold = by_hash("sha256:aaa");
    assert_eq!(a_hold["state"], json!("waiting"));
    assert_eq!(a_hold["releases_next_call"], json!(false));
    assert_eq!(a_hold["grants"], json!([]));
    assert_eq!(a_hold["rule"], json!("r-publish"));
    assert_eq!(a_hold["capability"], json!("vcs.publish"));
    assert_eq!(a_hold["tool"], json!("Bash"));
    assert_eq!(a_hold["target"], json!("git push origin main"));
    assert_eq!(a_hold["held"], json!(1));
    assert_eq!(a_hold["request_id"], json!("req-a"));
    assert!(a_hold["message"]
        .as_str()
        .unwrap_or_default()
        .contains("approval event"));
    // The command is the whole point of the view: an operator who has to grep
    // a ledger to find out a run is blocked on them has no inbox at all.
    let cmd = a_hold["approve_command"].as_str().unwrap();
    assert!(
        cmd.starts_with("trunnion approve /") && cmd.contains(" req-a "),
        "the command must be runnable as printed: {cmd}"
    );

    // Somebody said no. A refusal is a state, not an absence, and it releases
    // nothing.
    let b = by_hash("sha256:bbb");
    assert_eq!(b["state"], json!("refused"));
    assert_eq!(b["releases_next_call"], json!(false));
    assert_eq!(b["grants"][0]["verdict"], json!("deny"));
    assert_eq!(b["grants"][0]["approver"], json!("user:mariano@local"));
    assert_eq!(b["grants"][0]["spent"], json!(false));
    assert_eq!(b["grants"][0]["permitted"], json!(true));

    // Somebody said yes, and the retry has not happened.
    let c = by_hash("sha256:ccc");
    assert_eq!(c["state"], json!("released"));
    assert_eq!(c["releases_next_call"], json!(true));

    // A grant already spent by a retry, with the call held again after it. A
    // single-use grant that looks usable would be the worst row on the page.
    let d = by_hash("sha256:ddd");
    assert_eq!(d["state"], json!("spent"));
    assert_eq!(d["releases_next_call"], json!(false));
    assert_eq!(d["grants"][0]["spent"], json!(true));
    assert!(d["grants"][0]["spent_at"].is_string());
    assert_eq!(d["held"], json!(2), "the retry is the same hold, counted");
    assert_eq!(
        d["request_id"],
        json!("req-d2"),
        "the command names the most recent request, which is the one still held"
    );

    assert_eq!(a["blocked"], json!(3));
    assert_eq!(a["released"], json!(1));
    // The tracked laptop policy permits any approver, and the view has to say
    // so: self approval is permitted here and recorded rather than refused.
    assert_eq!(a["approvers"], json!("any"));
    assert!(a["ledger"].as_str().unwrap().starts_with('/'));

    // Still read-only. The route that names the command refuses to be one.
    let post = raw(
        addr,
        "POST /api/approvals HTTP/1.1\r\nhost: localhost\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
    );
    assert_eq!(post.status, 405);
}

#[test]
fn a_ledger_with_no_hold_has_an_empty_inbox_rather_than_no_route() {
    let ledger = fixture_ledger("no-holds");
    let addr = serve(&ledger);
    let a = get(addr, "/api/approvals").json();
    assert_eq!(a["holds"], json!([]));
    assert_eq!(a["blocked"], json!(0));
    assert_eq!(a["released"], json!(0));
}

// -- refusals ---------------------------------------------------------------

#[test]
fn an_unknown_api_path_is_a_404_fault() {
    let ledger = fixture_ledger("unknown");
    let addr = serve(&ledger);

    let miss = get(addr, "/api/nonesuch");
    assert_eq!(miss.status, 404);
    assert_eq!(miss.content_type, "application/json; charset=utf-8");
    let fault = miss.json();
    assert!(
        fault["cause"].as_str().unwrap().contains("/api/nonesuch"),
        "the cause must name the path: {fault}"
    );
    assert!(
        fault["fix"].as_str().unwrap().contains("/api/score"),
        "the fix must name the routes that do exist: {fault}"
    );

    // An id that was never appended is a 404, not an empty 200.
    let no_event = get(addr, "/api/events/ev-never-appended");
    assert_eq!(no_event.status, 404);
    assert!(no_event.json()["fix"].is_string());
}

#[test]
fn a_write_method_is_refused_because_the_api_is_read_only() {
    let ledger = fixture_ledger("readonly");
    let addr = serve(&ledger);

    for method in ["POST", "PUT", "PATCH", "DELETE"] {
        let reply = raw(
            addr,
            &format!(
                "{method} /api/score HTTP/1.1\r\nhost: localhost\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
            ),
        );
        assert_eq!(reply.status, 405, "{method} was not refused");
        let fault = reply.json();
        assert!(
            fault["cause"].as_str().unwrap().contains(method),
            "the cause must name the method: {fault}"
        );
        assert!(fault["fix"].as_str().unwrap().contains("GET"));
    }
}

#[test]
fn a_query_that_cannot_be_parsed_is_refused_rather_than_guessed() {
    let ledger = fixture_ledger("badquery");
    let addr = serve(&ledger);

    for target in [
        "/api/events?limit=lots",
        "/api/events?offset=-1",
        "/api/events?since=last%20tuesday",
        "/api/events?since=2026-08-05T09%3A14%3A02%2B02%3A00",
        "/api/events?kinds=run.open",
        "/api/events?kind",
        "/api/events?kind=%zz",
    ] {
        let reply = get(addr, target);
        assert_eq!(
            reply.status, 400,
            "{target} was not refused: {}",
            reply.body
        );
        let fault = reply.json();
        assert!(
            !fault["cause"].as_str().unwrap_or_default().is_empty()
                && !fault["fix"].as_str().unwrap_or_default().is_empty(),
            "{target} produced a Fault with no fix: {fault}"
        );
    }

    // A limit above the maximum returns the maximum rather than erroring.
    let clamped = get(addr, "/api/events?limit=99999").json();
    assert_eq!(clamped["returned"], json!(8));
}

// -- the adversarial case ---------------------------------------------------

#[test]
fn a_mutated_event_makes_verify_report_not_ok_and_name_the_entry() {
    let ledger = fixture_ledger("tampered");
    let addr = serve(&ledger);
    assert_eq!(get(addr, "/api/verify").json()["ok"], json!(true));

    // Rewrite one stored envelope in place, same length and still canonical
    // JSON, so only the hashes give it away. The ledger is append-only, so
    // this is exactly the edit no code path performs.
    let path = ledger.join("events.jsonl");
    let text = fs::read_to_string(&path).unwrap();
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let before = lines[2].clone();
    lines[2] = lines[2].replace("2026-08-05T09:14:03.000Z", "2026-08-05T09:14:09.000Z");
    assert_ne!(before, lines[2], "the fixture entry was not altered");
    fs::write(&path, format!("{}\n", lines.join("\n"))).unwrap();

    // Derived on the request, never cached: the same server now reports the
    // fault without a restart.
    let verify = get(addr, "/api/verify").json();
    assert_eq!(verify["ok"], json!(false), "verify: {verify}");
    let faults = verify["faults"].as_array().unwrap();
    assert!(!faults.is_empty());
    assert!(
        faults.iter().any(|f| f["index"] == json!(2)),
        "no fault names the altered entry: {faults:?}"
    );
    for fault in faults {
        let text = fault["fault"].as_str().unwrap();
        assert!(
            text.contains("Fix:"),
            "a fault must carry the action to take: {text}"
        );
    }
    assert!(
        verify["reproduce"]
            .as_str()
            .unwrap()
            .starts_with("trunnion ledger verify "),
        "the reader gets the command that checks the server"
    );
}

/// Two calls interleave: request A, request B, then the decision for A. An
/// adjacency walk pairs A's decision with B's request and reports the hold
/// against the wrong call, with the wrong approve command under it. The
/// correlation the decision itself records gets it right.
#[test]
fn a_hold_is_correlated_by_the_recorded_call_and_not_by_position() {
    let dir = workdir("inbox-correlation").join("ledger");
    let mut ledger = Ledger::init(&dir).unwrap();
    for ev in [
        event(
            "run-3000",
            0,
            "2026-08-07T10:00:00.000Z",
            "run.open",
            json!({"workload": "interleaved", "restored_checkpoint": null}),
        ),
        event(
            "run-3000",
            1,
            "2026-08-07T10:00:01.000Z",
            "tool.request",
            json!({"request_id": "run-3000-req-1", "call_hash": "sha256:aaa", "tool": "Bash", "args": {"command": "git push origin main"}}),
        ),
        event(
            "run-3000",
            2,
            "2026-08-07T10:00:02.000Z",
            "tool.request",
            json!({"request_id": "run-3000-req-2", "call_hash": "sha256:bbb", "tool": "Bash", "args": {"command": "ls"}}),
        ),
        event(
            "run-3000",
            3,
            "2026-08-07T10:00:03.000Z",
            "policy.decision",
            json!({
                "verdict": "hold",
                "rule": "r-publish",
                "capability": "vcs.publish",
                "message": "this call needs an approval before it proceeds",
                "request_id": "run-3000-req-1",
                "call_hash": "sha256:aaa"
            }),
        ),
        event(
            "run-3000",
            4,
            "2026-08-07T10:00:04.000Z",
            "run.seal",
            json!({"outcome": "complete"}),
        ),
    ] {
        ledger.append(ev).unwrap();
    }

    let addr = serve(&dir);
    let body = get(addr, "/api/approvals").json();
    let holds = body["holds"].as_array().expect("holds is an array");
    assert_eq!(holds.len(), 1, "one call was held");
    assert_eq!(
        holds[0]["call_hash"], "sha256:aaa",
        "the hold names the call its own decision named, not the request that happened to precede it"
    );
    assert_eq!(holds[0]["request_id"], "run-3000-req-1");
    assert_eq!(holds[0]["state"], "waiting");
}

/// A run that skips a seq number. The hole is reported and is not a fault:
/// the record cannot tell a harness killed mid-run from an event a producer
/// numbered and never appended, so ok stays true and the gap is a finding.
#[test]
fn verify_reports_a_seq_gap_and_the_ledger_still_reads_ok() {
    let dir = workdir("verify-seq-gap").join("ledger");
    let mut ledger = Ledger::init(&dir).unwrap();
    for ev in [
        event(
            "run-4000",
            0,
            "2026-08-07T11:00:00.000Z",
            "run.open",
            json!({"workload": "gapped", "restored_checkpoint": null}),
        ),
        // seq 1 and 2 are never appended.
        event(
            "run-4000",
            3,
            "2026-08-07T11:00:03.000Z",
            "run.seal",
            json!({"outcome": "complete"}),
        ),
    ] {
        ledger.append(ev).unwrap();
    }

    let addr = serve(&dir);
    let body = get(addr, "/api/verify").json();
    assert_eq!(
        body["ok"], true,
        "a hole in seq is a finding and never a fault"
    );
    let gaps = body["seq_gaps"].as_array().expect("seq_gaps is an array");
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0]["run_id"], "run-4000");
    assert_eq!(gaps[0]["after"], 0);
    assert_eq!(gaps[0]["before"], 3);
    assert_eq!(gaps[0]["missing"], 2);
}

/// The workspace routes, and the state a console started without a ledger is in.
///
/// One test rather than several: `TRUNNION_HOME` is process-global and the tests
/// in this binary run in parallel, so a second test setting it would be racing
/// this one for the same variable.
#[test]
fn the_workspace_routes_answer_without_a_ledger_and_the_log_routes_say_why_they_cannot() {
    let root = std::env::temp_dir().join(format!("trunnion-console-ws-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let home = root.join("home");
    let project = root.join("code/demo");
    fs::create_dir_all(project.join(".claude")).unwrap();
    fs::write(project.join("CLAUDE.md"), "# rules\n").unwrap();
    std::env::set_var("TRUNNION_HOME", &home);

    let mut ws = trunnion::workspace::Workspace::load(&home).unwrap();
    ws.add(
        &home,
        &project.display().to_string(),
        Some("demo"),
        trunnion::workspace::Risk::Regulated,
    )
    .unwrap();
    ws.save(&home).unwrap();

    // A console with no ledger still answers the workspace.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || console::serve_on(&listener, None));

    let list = get(addr, "/api/projects");
    assert_eq!(list.status, 200);
    let v = list.json();
    assert_eq!(
        v["ceiling"],
        json!(3),
        "the page states what a static read caps at"
    );
    let row = &v["projects"][0];
    assert_eq!(row["id"], json!("demo"));
    assert_eq!(
        row["risk"],
        json!("regulated"),
        "risk reaches the page, because it is what orders the queue"
    );
    assert_eq!(row["readable"], json!(true));
    assert_eq!(row["scores"].as_array().unwrap().len(), 12);
    // at_floor is what the front page leads with, since the composite is the
    // minimum and reads 0 for almost every real repository.
    assert!(row["at_floor"].as_u64().unwrap() >= 1, "{row}");

    let scan = get(addr, "/api/projects/demo/scan").json();
    assert_eq!(scan["findings"].as_array().unwrap().len(), 12);
    assert!(
        scan["findings"][0]["gap"].is_string(),
        "the scan carries the gap: {scan}"
    );

    let rem = get(addr, "/api/projects/demo/remediate").json();
    assert!(
        rem["document"]
            .as_str()
            .unwrap()
            .contains("REQUIREMENT FOR LEVEL"),
        "{rem}"
    );
    // Regulated work audits before it capabilities, so the trust layer leads.
    assert_eq!(rem["gaps"][0]["key"], json!("execution_environment"));

    // The log routes report that there is no log, which is a different state
    // from a log that is damaged. The takeover reads the second as an alarm.
    let no_ledger = get(addr, "/api/verify");
    assert_eq!(no_ledger.status, 404);
    assert!(
        no_ledger.body.contains("started without a ledger"),
        "the fault says which state this is: {}",
        no_ledger.body
    );

    let missing = get(addr, "/api/projects/nope/scan");
    assert_eq!(missing.status, 404);
    assert!(missing.body.contains("nope"), "{}", missing.body);

    std::env::remove_var("TRUNNION_HOME");
}

// -- slice 24: the operations aggregate -------------------------------------

/// A ledger whose `tool.result` events carry a duration, so the percentile
/// branch has something to measure. `n` of them, timestamped now, because the
/// default window is a claim about the last 24 hours and a fixture dated last
/// week would test the window rather than the percentile.
fn latency_ledger(name: &str, n: u64) -> PathBuf {
    let dir = workdir(name).join("ledger");
    let mut ledger = Ledger::init(&dir).unwrap();
    let now = trunnion::gateway::rfc3339_now();
    ledger
        .append(event(
            "run-now",
            0,
            &now,
            "run.open",
            json!({"workload": "latency", "restored_checkpoint": null}),
        ))
        .unwrap();
    for i in 0..n {
        ledger
            .append(event(
                "run-now",
                i + 1,
                &now,
                "tool.result",
                // A spread rather than a constant, so a percentile that
                // silently returned the mean or the first sample would differ
                // from the one this asserts.
                json!({"outcome": "ok", "duration_ms": i * 10}),
            ))
            .unwrap();
    }
    dir
}

#[test]
fn a_kind_the_ledger_never_carried_is_null_and_a_kind_with_none_in_the_window_is_zero() {
    let ledger = fixture_ledger("ops-absent");
    let addr = serve(&ledger);
    let all = get(addr, "/api/operations?window=all").json();

    // The fixture carries policy.decision but no approval at all. Those are
    // different states and the difference is the whole point: zero means the
    // walk ran and found none, null means the producer never wrote one, and
    // rendering the second as the first is a dashboard claiming a control was
    // exercised and clean when it was never exercised.
    assert_eq!(all["counts"]["holds"]["count"], json!(0));
    assert_eq!(all["counts"]["approvals"]["count"], Value::Null);
    assert_eq!(all["counts"]["denials"]["count"], json!(1));

    // Every number names where it came from, so a tile can print the path.
    assert_eq!(all["counts"]["runs_opened"]["source"], json!("run.open"));

    // The same absent rule reaches the topology, so a node nobody has ever
    // produced an event for is not drawn as a live node reading zero.
    let nodes = all["topology"].as_array().unwrap();
    let sensors = nodes.iter().find(|n| n["node"] == "sensor bus").unwrap();
    assert_eq!(
        sensors["events"],
        Value::Null,
        "the fixture carries no sensor.verdict"
    );
    let broker = nodes.iter().find(|n| n["node"] == "tool broker").unwrap();
    assert!(broker["events"].as_u64().unwrap() > 0);
}

#[test]
fn the_window_is_a_claim_about_now_so_an_old_ledger_reports_zero_and_not_its_totals() {
    // The fixture is dated 2026-08-05. Under the default window the honest
    // answer is that nothing happened in the last 24 hours, and a dashboard
    // that showed the lifetime totals under a "24h" label would be describing
    // a different question than the one on screen.
    let ledger = fixture_ledger("ops-window");
    let addr = serve(&ledger);
    let day = get(addr, "/api/operations").json();
    assert_eq!(day["window"], json!("24h"));
    assert_eq!(day["scanned"], json!(0));
    assert_eq!(day["counts"]["runs_opened"]["count"], json!(0));
    // Still not null: the kind exists on the log, so this is a real zero.
    assert_eq!(day["counts"]["denials"]["count"], json!(0));

    let all = get(addr, "/api/operations?window=all").json();
    assert!(all["scanned"].as_u64().unwrap() > 0);
    assert_eq!(all["total_events"], all["scanned"]);
}

#[test]
fn a_percentile_under_the_sample_floor_is_null_and_the_sample_count_travels_with_it() {
    let thin = serve(&latency_ledger("ops-thin", 4));
    let g = get(thin, "/api/operations").json()["gate_latency"].clone();
    assert_eq!(g["samples"], json!(4));
    assert_eq!(g["p95"], Value::Null, "four samples do not make a p95");
    assert_eq!(g["p50"], Value::Null);
    // The slowest call is a fact about one call and needs no distribution, so
    // it is reported even when the percentiles are not.
    assert_eq!(g["max"], json!(30));
    assert_eq!(g["source"], json!("tool.result.duration_ms"));

    let thick = serve(&latency_ledger("ops-thick", 20));
    let g = get(thick, "/api/operations").json()["gate_latency"].clone();
    assert_eq!(g["samples"], json!(20));
    // Nearest rank over 0,10,..,190: p95 is the ceil(0.95*20)=19th sample.
    assert_eq!(g["p95"], json!(180));
    assert_eq!(g["p50"], json!(90));
}

#[test]
fn a_window_this_route_does_not_read_is_refused_rather_than_quietly_defaulted() {
    let ledger = fixture_ledger("ops-badwindow");
    let addr = serve(&ledger);
    let r = get(addr, "/api/operations?window=lifetime");
    // Falling through to 24h would answer a question nobody asked and label it
    // with the window they did ask for.
    assert_eq!(r.status, 400, "body: {}", r.body);
    assert!(
        r.body.contains("24h"),
        "the fix names the windows: {}",
        r.body
    );
}
