#!/usr/bin/env python3
"""Vendor harness-kit's contracts into config/contracts.json.

Why a vendored copy at all. The remediation brief quotes the contract's own
words for a requirement and its check, because a paraphrased requirement is how
a check ends up testing something adjacent. Those words live in harness-kit and
this binary ships without it, so the text has to be in the tree.

Why JSON and not the YAML. trunnion has no YAML parser and is not getting one:
the obvious crate was archived by its author, and an unmaintained parser in a
tool that reads other people's repositories is the supply chain this project
refuses. serde_json is already here. The conversion happens once, on a
developer machine, and the result is reviewed in a diff like any other file.

What is deliberately dropped. `evidence`, `signals` and `compounding` are not
carried: the brief does not use them, and a field vendored but unread is a
field that rots without anything noticing. `anti_pattern` is carried, because
the brief has a section that quotes it.

What this must never become. contracts.yaml:33 says trunnion does not read that
file and must not, which is the line keeping scoring and prescription apart:
harness-kit refuses to infer a level, trunnion refuses to prescribe. The vendored
copy is quoted by the remediation brief and is read by nothing that produces a
number. tests/invariants.rs enforces that, because a rule of this kind kept by
intention is worth nothing.

Usage:
    python3 dev/vendor-contracts.py ../harness-kit/contracts.yaml
"""

import json
import pathlib
import sys

import yaml

CARRIED = ("requirement", "artifact", "check")


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    src = pathlib.Path(sys.argv[1])
    doc = yaml.safe_load(src.read_text())

    contracts = []
    for c in doc["contracts"]:
        targets = {}
        for level, body in (c.get("targets") or {}).items():
            targets[str(level)] = {
                k: " ".join(str(body[k]).split()) for k in CARRIED if body.get(k)
            }
        contracts.append(
            {
                "key": c["key"],
                "id": c["id"],
                "targets": targets,
                "anti_pattern": " ".join(str(c.get("anti_pattern", "")).split()),
                "cost": " ".join(str(c.get("cost", "")).split()),
            }
        )

    if len(contracts) != 12:
        print(f"expected twelve contracts, got {len(contracts)}", file=sys.stderr)
        return 1

    # The citation harness-kit's ci/consumers-cite-current-version.sh greps
    # for, verbatim and version-interpolated, so re-vendoring updates it and a
    # copy left behind is reported as stale by the repository it came from
    # rather than by nobody.
    out = {
        "_source": (
            f"harness-kit contracts {doc['contracts_version']}, "
            "vendored from contracts.yaml by trunnion's dev/vendor-contracts.py"
        ),
        "_note": (
            "Quoted by the remediation brief so a requirement reaches the reader "
            "in the words that defined it. Read by nothing that produces a score: "
            "harness-kit refuses to infer a level and trunnion refuses to prescribe, "
            "and tests/invariants.rs is what keeps that true."
        ),
        "contracts_version": doc["contracts_version"],
        "spec_version": doc["spec_version"],
        "contracts": contracts,
    }
    dest = pathlib.Path(__file__).resolve().parent.parent / "config" / "contracts.json"
    dest.write_text(json.dumps(out, indent=2, ensure_ascii=False) + "\n")
    print(
        f"vendored {len(contracts)} contracts "
        f"({doc['contracts_version']}, spec {doc['spec_version']}) into {dest}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
