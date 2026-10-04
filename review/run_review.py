#!/usr/bin/env python3
"""Run review probes in a disposable source copy; production sources stay intact."""

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["regressions", "performance"])
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    with tempfile.TemporaryDirectory(prefix="bibtui-review-") as temp:
        scratch = Path(temp)
        for name in ("Cargo.toml", "Cargo.lock", "build.rs"):
            shutil.copy2(root / name, scratch / name)
        shutil.copytree(root / "src", scratch / "src")
        shutil.copytree(root / "tests/fixtures", scratch / "tests/fixtures")
        with (scratch / "src/app/tests.rs").open("a") as tests:
            tests.write("\n" + (root / "review/regression_probes.rs").read_text())
            if args.mode == "performance":
                tests.write("\n" + (root / "review/performance_probe.rs").read_text())
        env = os.environ.copy()
        # Reuse dependency builds. Cargo fingerprints rebuild the copied crate.
        env["CARGO_TARGET_DIR"] = str(root / "target")
        command = ["cargo", "test", "--locked", "--lib"]
        if args.mode == "performance":
            command += ["--release", "app::tests::review_perf_scaling", "--", "--exact", "--nocapture"]
        else:
            command += ["app::tests::review_", "--", "--test-threads=1"]
        print("Running in disposable copy:", scratch, flush=True)
        print(" ".join(command), flush=True)
        return subprocess.run(command, cwd=scratch, env=env, check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
