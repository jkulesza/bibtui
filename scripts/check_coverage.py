#!/usr/bin/env python3
"""Check LLVM JSON line coverage against deliberately reviewed floors."""
import json
from pathlib import Path
import sys


def line_percent(summary):
    lines = summary["lines"]
    return 100.0 * lines["covered"] / lines["count"] if lines["count"] else 100.0


def check(report, baseline):
    data = report["data"][0]
    actual = {"TOTAL": line_percent(data["totals"])}
    for item in data["files"]:
        path = item["filename"].replace("\\", "/")
        for name in baseline:
            if path.endswith("/" + name):
                actual[name] = line_percent(item["summary"])
    failures = []
    for name, minimum in baseline.items():
        measured = actual.get(name)
        if measured is None:
            failures.append(f"{name}: missing from coverage report")
        elif measured + 1e-9 < minimum:
            failures.append(f"{name}: {measured:.2f}% below {minimum:.2f}%")
        else:
            print(f"{name}: {measured:.2f}% (minimum {minimum:.2f}%)")
    return failures


def main():
    report = json.loads(Path(sys.argv[1]).read_text())
    baseline = json.loads(Path(__file__).resolve().parent.parent.joinpath("coverage-baseline.json").read_text())
    failures = check(report, baseline)
    for failure in failures:
        print(failure, file=sys.stderr)
    return bool(failures)


if __name__ == "__main__":
    raise SystemExit(main())
