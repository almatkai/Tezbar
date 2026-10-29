#!/usr/bin/env python3
"""Interleave frozen beta.17/candidate release tests; fail on any result drift.

Standard library only. Runs synchronously, in this worktree, without rebuilding
or modifying either executable. Complete ordered signatures are retained once;
every process must match the checked-in beta.17 signatures, not just its peers.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[2]


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-executable", type=Path, required=True)
    parser.add_argument("--candidate-executable", type=Path, required=True)
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--samples", type=int, default=80)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.rounds < 2 or not 10 <= args.samples <= 10000:
        parser.error("use at least two rounds and 10..10000 samples")
    executables = {
        "baseline": args.baseline_executable.resolve(strict=True),
        "candidate": args.candidate_executable.resolve(strict=True),
    }
    baseline = json.loads((ROOT / "docs/performance/beta17-baseline.json").read_text())
    expected = {m["name"]: m["results"] for m in baseline["measurements"] if "results" in m}
    log_dir = Path(tempfile.mkdtemp(prefix="tezbar-search-ab-"))
    runs = []
    signatures = None
    for round_index in range(args.rounds):
        # Alternate AB/BA to expose drift, rather than batching all candidates.
        order = ["baseline", "candidate"] if round_index % 2 == 0 else ["candidate", "baseline"]
        for variant in order:
            command = [str(executables[variant]), "search_matrix", "--ignored",
                       "--test-threads=1", "--nocapture"]
            env = dict(os.environ, TEZBAR_BENCH_SAMPLES=str(args.samples))
            process = subprocess.run(command, cwd=ROOT, env=env, text=True,
                                     stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            log = log_dir / f"{round_index + 1}-{variant}.log"
            log.write_text(process.stdout)
            if process.returncode:
                raise RuntimeError(f"{variant} failed ({process.returncode}); see {log}")
            measurements = [json.loads(line.split("TEZBAR_PERF ", 1)[1])
                            for line in process.stdout.splitlines() if "TEZBAR_PERF " in line]
            actual = {m["name"]: m["results"] for m in measurements if "results" in m}
            if actual != expected:
                changed = sorted(name for name in set(actual) | set(expected)
                                 if actual.get(name) != expected.get(name))
                raise RuntimeError(f"Ordered IDs/scores changed: {changed}; see {log}")
            if variant == "candidate":
                signatures = actual
            runs.append({"round": round_index + 1, "variant": variant,
                         "log": str(log), "exact_baseline_parity": True,
                         "measurements": [{k: v for k, v in m.items() if k != "results"}
                                          for m in measurements]})
            print(f"round {round_index + 1} {variant}: all {len(actual)} signatures identical", flush=True)

    summary = []
    for name in expected:
        by_variant = {variant: [next(m for m in run["measurements"] if m["name"] == name)
                                for run in runs if run["variant"] == variant]
                      for variant in executables}
        b50 = [m["p50_ms"] for m in by_variant["baseline"]]
        c50 = [m["p50_ms"] for m in by_variant["candidate"]]
        ratios = [b / c for b, c in zip(b50, c50)]
        summary.append({
            "name": name,
            "baseline_p50_median_ms": statistics.median(b50),
            "candidate_p50_median_ms": statistics.median(c50),
            "p50_speedup": statistics.median(b50) / statistics.median(c50),
            "paired_p50_speedup_range": [min(ratios), max(ratios)],
            "baseline_p50_range_ms": [min(b50), max(b50)],
            "candidate_p50_range_ms": [min(c50), max(c50)],
            "baseline_p95_median_ms": statistics.median(m["p95_ms"] for m in by_variant["baseline"]),
            "candidate_p95_median_ms": statistics.median(m["p95_ms"] for m in by_variant["candidate"]),
        })
    report = {
        "schemaVersion": 1,
        "createdAt": datetime.now(timezone.utc).isoformat(),
        "productionBase": baseline["base"],
        "instrumentedBase": "d874f1f",
        "scope": "Native search synthetic fixtures; not UI latency, model inference, idle activity or total app RAM",
        "host": baseline["host"],
        "samplesPerCasePerProcess": args.samples,
        "rounds": args.rounds,
        "order": [f"{r['round']}:{r['variant']}" for r in runs],
        "executables": {variant: {"path": str(path), "sha256": sha256(path)}
                        for variant, path in executables.items()},
        "exactOrderedIdScoreParity": True,
        "signatures": signatures,
        "summary": summary,
        "runs": runs,
    }
    output = args.output if args.output.is_absolute() else ROOT / args.output
    output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    for case in summary:
        print(f"{case['name']}: {case['baseline_p50_median_ms']:.4f} -> "
              f"{case['candidate_p50_median_ms']:.4f} ms ({case['p50_speedup']:.2f}x)")
    print(f"Saved {output}; raw logs {log_dir}")


if __name__ == "__main__":
    main()
