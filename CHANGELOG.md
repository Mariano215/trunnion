# Changelog

Notable changes to gantry. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Every entry names the consequence for a score or for what a reader can trust,
because a changelog entry that only says a file changed makes the reader open
the diff to find out whether it mattered.

## [Unreleased]

### Added

- **A workspace.** `gantry project add|list|remove|scan|remediate` registers a
  set of repositories, local paths or git URLs, so one install answers for more
  than one project. A cloned repository is pinned to the commit resolved at add
  time, so a score can always be traced back to an exact revision. No
  credentials are stored: cloning shells out to git, which resolves whatever
  helper the operator already has, with terminal prompting off.
- **Remediation.** `gantry project remediate <id>` turns each finding into a
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
- **A workspace console.** `gantry console` with no argument serves every
  registered project: `/api/projects`, `/api/projects/:id/scan` and
  `/api/projects/:id/remediate`. `gantry console <ledger-dir>` is unchanged.

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
  existed, so remove-then-add failed on a directory gantry wrote itself.
- **The level-2 gap described the wrong test.** It said a check file had to name
  the artifact found; the scan actually resolves a check file whose text
  contains one of the probe's markers. A gap describing a different test from
  the one the number came from is the failure this scanner exists to catch.

### Changed

- **A gap names no maturity level.** The scan's next reachable state is 2 and
  the remediation queue targets 3, the first level anything is prescribed for,
  so a number quoted in the gap was right in one output and wrong in the other.
