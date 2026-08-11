# Contributing

## The one rule this project actually enforces

A rule carried only by a guide caps at maturity 3. That is the whole thesis,
and it applies to this repository before it applies to anyone else's: **a rule
added to `CLAUDE.md` without something that fails when the rule is broken is a
defect in that file, not a standard.**

So a pull request that adds a rule adds the check. A pull request that adds
behaviour adds the check that catches the behaviour going wrong. If you cannot
write a check that fails, say so in the pull request rather than adding the
rule, because a rule nothing enforces reads as coverage and is not.

## Before you open a pull request

```
zsh ci/run.sh
```

That is the same gate CI runs: `cargo fmt --check`, `cargo clippy --all-targets
-D warnings`, the test suite, then live checks that drive the built binary.
macOS is required, because the sandbox tests exercise seatbelt.

## What gets a change rejected

- **A check that has never been observed failing.** Break the thing
  deliberately, watch the check go red, and paste that output into the pull
  request. A green check that cannot fail is worse than no check: it reports
  coverage that does not exist.
- **A number with no path behind it.** Every score this project produces names
  the artifact it found, or names every path it looked in and found nothing. An
  opinion dressed as a measurement is the failure mode the whole tool exists to
  catch.
- **A new dependency without a reason.** Anything added to `[dependencies]`
  needs an entry in `docs/DEPENDENCIES.md` saying why, and `ci/run.sh` fails
  without one. This is a security control plane that reads other people's
  repositories; the dependency list is part of what a reviewer has to trust.
- **`unwrap` or `expect` outside tests and `main`.** Clippy enforces it.
- **An error that names a cause and no fix.** Every `Fault` carries `{cause,
  fix}`, and the fix names the concrete action, because an agent reads that
  message and acts on it. "Invalid configuration" is not a fix.

## Style

Direct and technical. Sentence case. No emoji, no exclamation marks. State what
the thing does rather than how transformative it is.

Comments explain **why**, not what. The what is in the code underneath. A
comment that restates the line above it is noise a reviewer has to read past.

Commit messages: a conventional prefix (`feat:`, `fix:`, `docs:`) and then a
sentence describing what changed and, in the body, why it was wrong before.

## Scope, so you do not waste an afternoon

This repository is the control plane. Two things deliberately live elsewhere
and pull requests moving them here will be declined:

- **What a maturity level means** is the
  [Agent Harness Maturity Specification](https://github.com/Mariano215/agent-harness-maturity).
  Gantry measures against it and does not define it.
- **What to build to reach a level** is
  [harness-kit](https://github.com/Mariano215/harness-kit). Gantry quotes those
  contracts in a remediation brief and never scores against them, which
  `tests/invariants.rs` enforces.

That separation is the point. Gantry refuses to be prescriptive, harness-kit
refuses to infer a level, and the specification refuses to ship code.

## Reporting a vulnerability

Do not open a public issue. Use GitHub's private security advisory on this
repository, or contact the maintainer through
[github.com/Mariano215](https://github.com/Mariano215).
