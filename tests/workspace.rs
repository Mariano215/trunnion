//! The workspace registry: a list of repositories one install answers for.
//!
//! Two properties matter here. The registry round-trips, because it is read
//! back days after it was written and a field that changes meaning between the
//! write and the read loses a project silently. And a project that cannot be
//! registered is refused with the path or the name in the message, because the
//! operator is looking at a typo and the fault is the only thing that tells
//! them which one.
//!
//! Every test drives the library, which takes the gantry home as an argument,
//! so no test can reach the operator's real `~/.gantry` even if the
//! environment says otherwise. The one test that reads `GANTRY_HOME` is the
//! only one that sets it.

use gantry::workspace::{self, Risk, Source, Workspace};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn workdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gantry-ws-it-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A repository with one commit, to be cloned over `file://`.
fn remote(root: &Path) -> String {
    let repo = root.join("remote");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.email", "ci@example.invalid"]);
    git(&repo, &["config", "user.name", "ci"]);
    fs::write(repo.join("CLAUDE.md"), "# rules\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "first"]);
    format!("file://{}", repo.display())
}

#[test]
fn a_local_project_is_added_listed_removed_and_the_registry_round_trips() {
    let root = workdir("local");
    let home = root.join("home");
    let project_dir = root.join("code/foo");
    fs::create_dir_all(&project_dir).unwrap();

    let mut ws = Workspace::load(&home).unwrap();
    let added = ws
        .add(
            &home,
            &project_dir.display().to_string(),
            None,
            Risk::Internal,
        )
        .unwrap();
    assert_eq!(added.id, "foo", "the id defaults to the directory basename");
    assert_eq!(added.risk, Risk::Internal);
    assert_eq!(added.ledger, None);
    assert_eq!(added.last_scan, None);
    ws.save(&home).unwrap();

    // The file on disk, read as a document rather than through the type that
    // wrote it: a consumer outside this binary reads these names.
    let path = workspace::registry_path(&home);
    let raw: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(raw["version"], 1);
    assert_eq!(raw["projects"][0]["id"], "foo");
    assert_eq!(raw["projects"][0]["source"]["kind"], "local");
    assert_eq!(raw["projects"][0]["risk"], "internal");
    assert!(raw["projects"][0]["ledger"].is_null());
    assert!(raw["projects"][0]["last_scan"].is_null());

    let reloaded = Workspace::load(&home).unwrap();
    assert_eq!(reloaded.ids(), vec!["foo".to_string()]);
    assert_eq!(
        reloaded.find("foo").unwrap().source,
        // Stored absolute: the registry is read from whatever directory the
        // next command runs in, and a relative path would resolve against it.
        Source::Local {
            path: fs::canonicalize(&project_dir)
                .unwrap()
                .display()
                .to_string()
        }
    );

    let mut ws = reloaded;
    let removed = ws.remove("foo").unwrap();
    assert_eq!(removed.id, "foo");
    ws.save(&home).unwrap();
    assert!(Workspace::load(&home).unwrap().projects.is_empty());
}

#[test]
fn a_risk_level_round_trips_under_the_name_remediation_ranks_by() {
    let root = workdir("risk");
    let home = root.join("home");
    let project_dir = root.join("bar");
    fs::create_dir_all(&project_dir).unwrap();

    let mut ws = Workspace::load(&home).unwrap();
    ws.add(
        &home,
        &project_dir.display().to_string(),
        None,
        Risk::ClientFacing,
    )
    .unwrap();
    ws.save(&home).unwrap();

    let raw: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(workspace::registry_path(&home)).unwrap())
            .unwrap();
    assert_eq!(
        raw["projects"][0]["risk"], "client_facing",
        "the three values come from harness-kit and are what remediation ranks by; a renamed one is a broken join"
    );
    assert_eq!(
        Workspace::load(&home).unwrap().find("bar").unwrap().risk,
        Risk::ClientFacing
    );
    assert!(Risk::parse("client_facing").is_ok());
    assert!(Risk::parse("regulated").is_ok());
    // A near miss is refused rather than defaulted: silently registering a
    // regulated repository as internal is the failure this rejects.
    let fault = Risk::parse("client-facing").unwrap_err();
    assert!(fault.fix.contains("client_facing"), "{fault}");
}

#[test]
fn a_git_project_is_cloned_pinned_to_a_commit_and_scanned_from_the_cache() {
    let root = workdir("git");
    let home = root.join("home");
    let url = remote(&root);

    let mut ws = Workspace::load(&home).unwrap();
    let added = ws.add(&home, &url, None, Risk::Internal).unwrap();
    ws.save(&home).unwrap();

    assert_eq!(added.id, "remote", "the id defaults to the repository name");
    let Source::Git { url: stored, rev } = &added.source else {
        panic!("a git URL registered as {:?}", added.source);
    };
    assert_eq!(stored, &url);
    assert_eq!(
        rev.len(),
        40,
        "rev is the commit the clone landed on, so the scan is pinned: {rev}"
    );
    assert!(rev.chars().all(|c| c.is_ascii_hexdigit()), "rev: {rev}");

    // The clone lands under the gantry home, never beside the source, and it
    // is what a scan of this project reads.
    let checkout = ws.checkout(&home, &added);
    assert_eq!(checkout, workspace::cache_dir(&home, "remote"));
    assert!(checkout.join("CLAUDE.md").is_file());
    let report = gantry::scan::scan(&gantry::scan::RepoRead::open(&checkout).unwrap());
    assert!(
        report.findings.iter().any(|f| f.score > 0),
        "the pinned checkout scans through the same scan() the single-repository command uses"
    );
}

#[test]
fn adding_a_path_that_does_not_exist_names_the_path() {
    let root = workdir("missing");
    let home = root.join("home");
    let missing = root.join("code/nope");

    let mut ws = Workspace::load(&home).unwrap();
    let fault = ws
        .add(&home, &missing.display().to_string(), None, Risk::Internal)
        .unwrap_err();
    assert!(
        fault.cause.contains(&missing.display().to_string()),
        "the fault has to name the path the operator typed: {fault}"
    );
    assert!(
        fault.fix.contains(&missing.display().to_string()),
        "{fault}"
    );
    assert!(ws.projects.is_empty(), "a refused add registered nothing");

    // A file is not a repository root, and the fault says which one it is.
    let file = root.join("a-file");
    fs::write(&file, "x").unwrap();
    let fault = ws
        .add(&home, &file.display().to_string(), None, Risk::Internal)
        .unwrap_err();
    assert!(fault.cause.contains("is not a directory"), "{fault}");
    assert!(fault.fix.contains(&file.display().to_string()), "{fault}");
}

#[test]
fn a_duplicate_id_names_the_project_already_registered() {
    let root = workdir("dupe");
    let home = root.join("home");
    let first = root.join("one/foo");
    let second = root.join("two/foo");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();

    let mut ws = Workspace::load(&home).unwrap();
    ws.add(&home, &first.display().to_string(), None, Risk::Internal)
        .unwrap();
    let fault = ws
        .add(&home, &second.display().to_string(), None, Risk::Internal)
        .unwrap_err();
    assert!(
        fault.cause.contains("foo")
            && fault
                .cause
                .contains(&fs::canonicalize(&first).unwrap().display().to_string()),
        "the fault has to name the project already holding the id and where it came from: {fault}"
    );
    assert!(
        fault.fix.contains("--id"),
        "the fix has to name the flag that resolves it: {fault}"
    );
    assert_eq!(ws.projects.len(), 1, "the refused add registered nothing");

    // Which the flag then does.
    ws.add(
        &home,
        &second.display().to_string(),
        Some("foo-two"),
        Risk::Internal,
    )
    .unwrap();
    assert_eq!(ws.ids(), vec!["foo".to_string(), "foo-two".to_string()]);
}

#[test]
fn the_registry_lives_where_gantry_home_says_and_a_missing_one_is_empty() {
    let root = workdir("home");
    let home = root.join("elsewhere");
    // The only test that touches the environment, because it is the only
    // thing under test here; every other test passes the home in, which is
    // what makes reaching the real ~/.gantry impossible rather than unlikely.
    std::env::set_var("GANTRY_HOME", &home);
    assert_eq!(workspace::home().unwrap(), home);
    assert_eq!(
        workspace::registry_path(&workspace::home().unwrap()),
        home.join("workspace.json")
    );
    std::env::remove_var("GANTRY_HOME");

    // An install that has registered nothing is a state, not a failure: the
    // file is absent until the first add writes it.
    assert!(!workspace::registry_path(&home).exists());
    let ws = Workspace::load(&home).unwrap();
    assert!(ws.projects.is_empty());
    assert_eq!(ws.version, 1);
}

#[test]
fn a_registry_this_build_cannot_read_is_refused_rather_than_started_over() {
    let root = workdir("version");
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();
    fs::write(
        workspace::registry_path(&home),
        "{\"version\":99,\"projects\":[]}\n",
    )
    .unwrap();
    let fault = Workspace::load(&home).unwrap_err();
    assert!(fault.cause.contains("99"), "{fault}");

    fs::write(workspace::registry_path(&home), "not json\n").unwrap();
    let fault = Workspace::load(&home).unwrap_err();
    assert!(
        fault
            .cause
            .contains(&workspace::registry_path(&home).display().to_string()),
        "the fault names the file to fix: {fault}"
    );
}

/// A clone left under the cache does not block the id it was written for.
///
/// `remove` deliberately leaves the checkout: an operator who removed a
/// project by mistake still has the tree. That is only safe if re-adding the
/// same id works, and it did not — the clone refused a destination that
/// already existed, so remove-then-add failed on a directory gantry wrote
/// itself. A clone killed part way leaves the same tree and the same dead end.
#[test]
fn a_leftover_checkout_does_not_block_re_adding_the_id() {
    let root = workdir("orphan");
    let home = root.join("home");
    let url = remote(&root);

    let mut ws = Workspace::load(&home).unwrap();
    ws.add(&home, &url, Some("bar"), Risk::Internal).unwrap();
    ws.save(&home).unwrap();
    let cache = workspace::cache_dir(&home, "bar");
    assert!(cache.is_dir(), "the clone landed in the cache");

    ws.remove("bar").unwrap();
    ws.save(&home).unwrap();
    assert!(
        cache.is_dir(),
        "remove leaves the checkout, which is what makes the re-add case matter"
    );

    // The registry no longer holds the id, so the tree under it is an orphan
    // and re-adding must not fail on it.
    let project = ws
        .add(&home, &url, Some("bar"), Risk::Internal)
        .expect("re-adding an id whose old checkout is still on disk");
    match &project.source {
        Source::Git { rev, .. } => assert_eq!(rev.len(), 40, "the fresh clone is pinned: {rev}"),
        other => panic!("expected a git source, got {other:?}"),
    }
}

/// A half-written clone is discarded the same way, and the tree that replaces
/// it is a real checkout rather than the wreckage.
#[test]
fn a_partial_clone_is_discarded_rather_than_left_to_be_scanned() {
    let root = workdir("partial");
    let home = root.join("home");
    let url = remote(&root);

    // What a clone killed between mkdir and checkout leaves behind: the
    // destination exists, and there is no repository in it.
    let cache = workspace::cache_dir(&home, "bar");
    fs::create_dir_all(&cache).unwrap();
    fs::write(cache.join("half-written"), "wreckage\n").unwrap();

    let mut ws = Workspace::load(&home).unwrap();
    ws.add(&home, &url, Some("bar"), Risk::Internal)
        .expect("a partial clone is not a permanent block on the id");
    assert!(
        !cache.join("half-written").exists(),
        "the wreckage is gone rather than scanned as if it were the repository"
    );
    assert!(cache.join("CLAUDE.md").is_file(), "the real tree is there");
}

/// A URL is a repository address, not argv.
///
/// `is_url` asks only for a transport, and `--upload-pack=ssh://x` carries
/// one. Passed positionally, git reads it as the option it resembles and runs
/// the command it names. The case that matters is not an operator attacking
/// their own machine, it is the ordinary way a repository address is obtained:
/// pasted out of an issue, a README or a message.
#[test]
fn a_url_that_is_really_a_git_option_is_refused_rather_than_executed() {
    let root = workdir("argv");
    let home = root.join("home");
    let mut ws = Workspace::load(&home).unwrap();

    let canary = root.join("executed");
    let hostile = format!("--upload-pack=touch {}; ssh://x/y", canary.display());
    let fault = ws
        .add(&home, &hostile, Some("hostile"), Risk::Internal)
        .unwrap_err();
    assert!(
        fault.cause.contains("dash"),
        "the fault says why it is not a URL: {fault}"
    );
    assert!(
        !canary.exists(),
        "an option smuggled in as a URL reached git and ran"
    );
    assert!(
        ws.find("hostile").is_none(),
        "a refused clone registers nothing"
    );
}
