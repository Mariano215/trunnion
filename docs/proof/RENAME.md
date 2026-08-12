# The rename, and why the proofs still say gantry

This project was called gantry from slice 00 through slice 23 and was renamed
to trunnion on 2026-08-12, between the v0.1.1 release and the next slice.

## Why

The name collided three ways, and the third was decisive.

- An established Joomla template framework has been called Gantry for years
  and owns the search results for the word.
- `gantry` on crates.io was published in May 2020 by an unrelated project, the
  waSCC module registry client. The name cannot be claimed, so `cargo install
  gantry` would install that instead of this.
- `gantry-cli` was published on 2026-08-10, two days before this rename, by a
  release-governance tool. The collision was inside this project's own subject
  area and was still spreading.

`trunnion` was checked free on crates.io, npm and PyPI before the rename.

## What the freeze means

`docs/proof/00.md` through `docs/proof/23.md` are unchanged. They say gantry
throughout, because that is what the commands were called when they were run
and what the output said when it was captured. A proof document records what
happened. Editing twenty-four of them to describe commands nobody typed would
make every one of them a worse record, and this project's whole argument is
that a record is not rewritten to match a later preference.

Read a proof document's `gantry` as `trunnion`. Nothing else about them moved.

The proof run-scripts, `docs/proof/NN-run.sh`, are a different thing: `ci/run.sh`
executes them, so they had to keep working. Exactly one string changed in each,
the path `target/debug/gantry` to `target/debug/trunnion`. Every recorded data
string in them is untouched, including the synthetic `workload: gantry/slice-01`,
the `system:gantry-ledger` actor id, the temporary directory names, and the
`policy.decision` in `01-run.sh` that records a denied fetch of
`https://crates.io/api/v1/crates/gantry`. That last one is the lookup that would
have caught this two years earlier, denied by rule `deny[6]` at the time.

## What moved

Crate, binary, library, image path, the `TRUNNION_` environment variables, the
actor identity `agent:trunnion-laptop`, the console masthead, the site, and every
document outside `docs/proof/*.md`.

Two hashes had to be re-pinned, because the rename changed files that other
files pin by content:

- `config/policy.json` `host_permissions.declared`, because the word Gantry
  appeared in a `$comment` in `.claude/settings.json`.
- `templates/laptop/config/policy.json` `instruction_pack.declared`, because
  the template's instruction pack names the harness it runs under.

The second was already wrong before the rename. It declared
`sha256:e087ac11…`, which is the hash of this repository's `instructions/pack.md`
rather than the template's own, and nothing recomputes it at `template init`. A
harness built from the template would have opened with an instruction_pack
divergence from its first run. It now declares the hash of the file it ships.

The repository's own `instructions/pack.md` never mentioned the name, so its
hash did not move and `config/instruction-reviews.jsonl` needed no new entry.
