# Changelog

Notable changes to trunnion. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Every entry names the consequence for a score or for what a reader can trust,
because a changelog entry that only says a file changed makes the reader open
the diff to find out whether it mattered.

## [Unreleased]

### Added

- **`trunnion report`: a deliverable the recipient can refuse.** The proving
  workload named in `docs/PLAN.md` on day one and unbuilt for twenty-four
  slices. A new `audit.finding` event kind makes a claim a ledger leaf, and
  `trunnion report <ledger> <out>` writes a document where every finding
  travels with an inclusion bundle for the claim, for the read it rests on and
  for the model call that produced it, beside the key they check against.
  Holding only that directory and the binary, a third party runs
  `trunnion ledger verify-inclusion proofs/f-1.finding.json ledger.pub` with no
  log, no config and no network. The document states above the findings that a
  proof shows what was read, asked and unchanged, and never that the finding is
  true. No score moves: nothing is credited for making a claim, because a level
  that counted claims would pay for volume. The workload that produces one is
  [assay](https://github.com/Mariano215/assay), which runs the same audit six
  times adding one control per stage. See `docs/proof/25.md`.
- **An operations view: the whole harness on one screen.** Nine views each
  answered one question and none answered "is this healthy right now". The new
  tenth view carries counts for the window, tool-call latency, a live event
  tail, the twelve primitives and a control topology, at `#/operations` on any
  `trunnion console`. It surfaces two fields that have been recorded since
  slices 04 and 10 and read by nothing: `tool.result.duration_ms` and
  `model.call.latency_ms`. No score moves, because a view enforces nothing.
- **`GET /api/operations`**, the aggregate behind it, computed over the whole
  ledger rather than in the browser: `/api/events` caps at 1000 rows, so a
  page-derived count would describe the page while the screen read as a
  statement about the log. A count is null when the ledger has never carried
  that kind and a number when it has, so "never instrumented" and "ran and
  found none" cannot render alike, and a percentile under a twenty-sample floor
  is null with its sample count beside it.

### Changed

- **Renamed from gantry to trunnion.** The old name collided with an
  established Joomla template framework, with a crate published on crates.io in
  2020 that made `cargo install gantry` install someone else's tool, and, as of
  two days before this change, with a release-governance CLI in this project's
  own subject area. The crate, the binary, the library, the container path and
  the `TRUNNION_` environment variables all move. Anyone on 0.1.1 changes the
  image they pull and the command they run; nothing about the event schema, the
  ledger format or a stored ledger changes, and a ledger written by 0.1.1
  verifies unchanged.
- **The proof documents are deliberately not renamed.** `docs/proof/00.md`
  through `23.md` still say gantry, because they record what ran under the name
  it had. `docs/proof/RENAME.md` says so and explains how to read them.

### Fixed

- **The instruction-lifecycle sensor could not pass on any harness anyone had
  ever initialised.** `config/instruction-reviews.jsonl` sat in
  `templates/laptop` and never in the copy list `template validate` returns, so
  `template init` wrote the sensor and not the file its check greps; the row it
  did carry named a pack hash the template had stopped having. Primitive 01
  therefore could not reach 4 on a fresh harness, and the control failed on its
  first run for a reason the operator did not cause. `template validate` now
  refuses a bundle whose sensor reads a review record it does not ship, and one
  whose packs are not covered by a row.
- **`docs/proof/08-run.sh` said primitives 2 and 3 score N/A with the model
  endpoint down.** They score 3 and 2. The gateway's error branch appends a
  `model.call` with its prompt hash and window before returning the Fault, so
  what an unreachable endpoint costs is the reply, the exit status and the
  seal, never the telemetry. The comment had been wrong since slice 08 and is
  the kind of instruction that looks like coverage.
- **The laptop template declared an instruction-pack hash that was not its
  own.** `templates/laptop/config/policy.json` pinned this repository's
  `instructions/pack.md` rather than the pack the template ships, and nothing
  recomputes it at `template init`, so every harness built from the template
  opened with an instruction_pack divergence it had done nothing to earn. Found
  while re-pinning hashes for the rename; it predates it.

## [0.1.1] - 2026-08-11

### Fixed

- **The container image is built for arm64 as well as amd64.** 0.1.0 shipped
  an amd64-only image, so `docker pull` on an Apple Silicon machine failed
  with `no matching manifest for linux/arm64/v8` while the README told that
  reader to run exactly that command. Proved on arm64 before shipping: the
  image builds, `template init` writes a harness, and a run records
  `landlock-v4` with no shortfall, so containment is real on that
  architecture and not merely present.

## [0.1.0] - 2026-08-11

First release. Pre-1.0: the API is not stable, and every slice built so far
carries a proof document produced by running it.

### Added

- **Linux isolation.** Off-macOS runs were contained by nothing and recorded
  the backend as `none`, in a tool whose subject is whether other people's
  agents are contained. The Linux backend is Landlock, negotiated against the
  running kernel, and it applies inside a plain unprivileged `docker run` with
  no flags and no added capabilities. The recorded string names the ABI
  actually in force, so a kernel enforcing the filesystem half and nothing
  about egress cannot be recorded as enforcing both.
- **Ships as a container.** `CLAUDE.md` opened by saying so and no Dockerfile
  existed. `ghcr.io/mariano215/trunnion` carries the binary and the starting
  harness on a slim base, runs as an unprivileged user, and mounts the
  operator's harness and ledger rather than baking them: an image carrying a
  ledger would ship a signing identity every install shared.
- **A release pipeline.** A `v*` tag runs the gate on both platforms, builds
  macOS arm64 and Linux x86_64 archives with a checksum file, publishes the
  image, and writes release notes from this file. Three guards run before
  anything is published: the tag matches `Cargo.toml`, this file has a section
  for it, and the gate passes.
- **A Linux CI job.** The gate ran on macOS only, so the Landlock backend was
  verified once by hand in a container. Format, clippy and the suite now run on
  `ubuntu-24.04` on every change. The kernel is not pinned: a runner that
  enforces nothing fails the suite, which is the intended result.
- **A workspace.** `trunnion project add|list|remove|scan|remediate` registers a
  set of repositories, local paths or git URLs, so one install answers for more
  than one project. A cloned repository is pinned to the commit resolved at add
  time, so a score can always be traced back to an exact revision. No
  credentials are stored: cloning shells out to git, which resolves whatever
  helper the operator already has, with terminal prompting off.
- **Remediation.** `trunnion project remediate <id>` turns each finding into a
  brief that pastes into any agent, quoting harness-kit's requirement, artifact
  and acceptance check verbatim rather than summarising them. The queue is
  ordered by business risk: for client-facing and regulated work the trust layer
  outranks everything, because capability nothing can audit is a liability. The
  contracts are vendored to `config/contracts.json` and compiled in, and
  `tests/invariants.rs` fails the build if anything producing a score reads
  them.
- **A gap on every finding.** A scan said why a number was what it was and
  nothing about what would move it. Each finding now carries the shortfall,
  derived from the probe that produced it, so it can only ask for something the
  scan actually looked for.
- **`ScanReport` serialises.** The report is machine-readable, which is what the
  workspace sweep, the console and remediation read.
- **A workspace console.** `trunnion console` with no argument serves every
  registered project: `/api/projects`, `/api/projects/:id/scan` and
  `/api/projects/:id/remediate`. `trunnion console <ledger-dir>` is unchanged.
- **The console has a face.** Redrawn to the design in
  `docs/design/console-a-loadbearing.html`, and the workspace routes above
  finally have a view: a project index, the twelve primitives as a rail
  resting on the minimum, the evidence behind every number, and the
  remediation queue in the contracts' own words and in the order the API
  returns it. A front end that ranked that queue itself would be trunnion
  prescribing a level.

### Fixed

- **Argument injection in `git clone`.** A target only had to contain `://` to
  be treated as a URL, so `--upload-pack=...; ssh://x` was passed positionally
  and git read it as the option it resembles and ran the command it names. The
  URL now follows `--` and a leading dash is refused. The realistic vector was
  never an operator attacking their own machine, it was a URL pasted out of an
  issue or a message. Proved able to fail by removing the guard.
- **Project ids no longer escape the cache.** An id was taken unchecked into the
  path segment `cache/<id>`, so `--id ../../thing` wrote a clone outside it.
- **Removing a project no longer blocks re-adding it.** `remove` deliberately
  leaves the checkout, and the clone then refused a destination that already
  existed, so remove-then-add failed on a directory trunnion wrote itself.
- **A console with no ledger showed a verification alarm.** The boot path read
  any failure to read `/api/verify` as a verification failure, so a console
  started without a ledger opened on a red takeover about a log that does not
  exist. "There is no log here" and "the log here is damaged" are different
  states and only the second is an alarm.
- **Primitive 05 scored the sandbox's name, not the sandbox.** The rule
  required `tool.request./sandbox` to equal `seatbelt`, so a fully contained
  Linux run scored 3 and the level was a statement about which operating
  system ran the workload. It credits any sandbox that is not `none`, and an
  event with no sandbox field at all is still not evidence of one.
- **The level-2 gap described the wrong test.** It said a check file had to name
  the artifact found; the scan actually resolves a check file whose text
  contains one of the probe's markers. A gap describing a different test from
  the one the number came from is the failure this scanner exists to catch.

### Changed

- **A profile declares a property, not a mechanism.**
  `profile_requirements.isolation.declared` named `seatbelt` and every reader
  compared it by string equality, so a Linux host confining a run with Landlock
  ABI v4 recorded a shortfall it did not have, and a `regulated` profile under
  `on_unavailable: refuse` could not start there at all. The tracked profiles
  declare `per_run_confinement`. This is not a rename: Landlock added TCP
  restrictions in ABI v4, so `landlock-v1` through `-v3` provide the mechanism
  and not the property and are still short. A profile that means one mechanism
  still names it and is not satisfied by a different backend.
- **A gap names no maturity level.** The scan's next reachable state is 2 and
  the remediation queue targets 3, the first level anything is prescribed for,
  so a number quoted in the gap was right in one output and wrong in the other.
