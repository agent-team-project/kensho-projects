#!/usr/bin/env python3
"""Fail honestly when a later-stage Workplane product gate is not implemented."""

from __future__ import annotations

import argparse
import sys


OMISSIONS = {
    "acceptance": [
        "M1 real HTTP transaction and idempotency",
        "M1 human/agent equivalent authorization flow",
        "M1 production-browser transaction",
        "M2+ replay, outbox, realtime, and revoke behavior",
        "M3+ structured brief and complete public-plane parity",
        "M4 browser accessibility, search, recovery, and offline application startup",
    ],
    "release": [
        "scale/performance fixtures",
        "backup/restore and fault recovery",
        "full browser/accessibility matrix",
        "dynamic and independent security review",
        "dogfood research evidence and independent product/research verdicts",
        "SBOM, provenance, license, and release-readiness report",
    ],
}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("stage", choices=sorted(OMISSIONS))
    args = parser.parse_args()
    print(f"{args.stage} is intentionally not green at M0; explicitly unimplemented:", file=sys.stderr)
    for omission in OMISSIONS[args.stage]:
        print(f"- {omission}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
