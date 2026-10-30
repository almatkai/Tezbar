#!/usr/bin/env python3
"""Serial release P/C/C/P, exact complete replies, CPU and native-process RAM.

Uses test executables from this worktree, never host Python SQLite. Fixture must
be isolated and initialized by broad_reader_matrix, not a personal database.
"""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import threading
import time
from desktop import LIB, Usage, TICK_SECONDS


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run(exe, test, fixture, samples, output):
    env = dict(os.environ, TEZBAR_BENCH_SAMPLES=str(samples),
               TEZBAR_BROAD_FIXTURE=str(fixture))
    command = [str(exe), test, '--ignored', '--test-threads=1', '--nocapture']
    p = subprocess.Popen(command, env=env, stdout=subprocess.PIPE,
                         stderr=subprocess.STDOUT, text=True)
    usage = []
    stop = threading.Event()

    def sample():
        while not stop.is_set():
            u = Usage()
            if LIB.proc_pid_rusage(p.pid, 4, ctypes.byref(u)) == 0:
                usage.append(dict(t=time.monotonic(), start=u.proc_start_abstime,
                                  footprint=u.phys_footprint, rss=u.resident_size,
                                  max_footprint=u.lifetime_max_phys_footprint,
                                  cpu_s=(u.user_time+u.system_time)*TICK_SECONDS))
            stop.wait(.02)

    thread = threading.Thread(target=sample)
    thread.start()
    try:
        text, _ = p.communicate(timeout=180)
    finally:
        stop.set()
        thread.join()
        if p.poll() is None:
            p.terminate()  # Only the still-owned Popen child.
            p.wait(timeout=10)
    output.write_text(text)
    assert p.returncode == 0, str(output)
    rows = [json.loads(line.split('TEZBAR_PERF ', 1)[1])
            for line in text.splitlines() if 'TEZBAR_PERF ' in line]
    rows = [row for row in rows if 'p50_ms' in row]
    assert len(rows) == (10 if test == 'search_matrix' else 4)
    assert usage and len({u['start'] for u in usage}) == 1
    return dict(command=command, executable_sha256=sha(exe), pid=p.pid,
                returncode=p.returncode, rows=rows, memory_samples=usage,
                max_observed_footprint=max(u['footprint'] for u in usage),
                max_observed_rss=max(u['rss'] for u in usage),
                lifetime_max_footprint=max(u['max_footprint'] for u in usage))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--candidate', type=Path, required=True)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--samples', type=int, default=80)
    parser.add_argument('--reverse', action='store_true', help='C/P/P/C rather than P/C/C/P')
    args = parser.parse_args()
    args.output.mkdir(exist_ok=False)
    identities = dict(baseline=sha(args.baseline), candidate=sha(args.candidate))
    runs = []
    expected = {}
    order = ['candidate', 'baseline', 'baseline', 'candidate'] if args.reverse else ['baseline', 'candidate', 'candidate', 'baseline']
    for index, label in enumerate(order):
        for test in ['search_matrix', 'broad_reader_matrix']:
            exe = getattr(args, label).resolve()
            result = run(exe, test, args.fixture.resolve(), args.samples,
                         args.output / f'{index}-{label}-{test}.log')
            result.update(label=label, test=test)
            for row in result['rows']:
                name = row['name']
                complete = row['full_results']
                if name not in expected:
                    expected[name] = complete
                assert complete == expected[name], f'complete ordered reply mismatch: {label} {name}'
            runs.append(result)
            (args.output / 'runs.json').write_text(json.dumps(runs, indent=2))
    summary = {}
    for name in expected:
        summary[name] = {}
        for label in identities:
            rows = [row for run in runs if run['label'] == label
                    for row in run['rows'] if row['name'] == name]
            summary[name][label] = {key: statistics.median(row[key] for row in rows)
                                   for key in ['first_ms', 'first_cpu_ms', 'p50_ms', 'p95_ms',
                                               'cpu_p50_ms', 'cpu_p95_ms']}
    result = dict(identities=identities, fixture_sha256=sha(args.fixture),
                  exact_complete_replies=len(expected)*len(runs)//2,
                  scope='native subsystem; 20ms RAM sampling; first call is OS-cache uncontrolled, not disk-cold',
                  cpu_units='getrusage timeval seconds + microseconds/1e6; proc_pid_rusage ticks * mach timebase',
                  summary=summary)
    (args.output / 'summary.json').write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
