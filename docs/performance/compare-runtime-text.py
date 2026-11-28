#!/usr/bin/env python3
"""Repeat/interleave the test-only production text allocation/time probe."""
import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess

p = argparse.ArgumentParser()
p.add_argument('--baseline-executable', type=Path, required=True)
p.add_argument('--candidate-executable', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
executables = {'baseline': a.baseline_executable.resolve(), 'candidate': a.candidate_executable.resolve()}
runs = []
for round_number in range(3):
    order = ['baseline', 'candidate'] if round_number % 2 == 0 else ['candidate', 'baseline']
    for variant in order:
        raw = subprocess.check_output([str(executables[variant]), 'runtime_text_allocations', '--ignored', '--test-threads=1', '--nocapture'], stderr=subprocess.STDOUT, text=True)
        values = [json.loads(line.split('TEZBAR_RUNTIME_TEXT ', 1)[1]) for line in raw.splitlines() if 'TEZBAR_RUNTIME_TEXT ' in line]
        assert len(values) == 2
        runs.append(dict(round=round_number+1, variant=variant, measurements=values))
summary = []
for name in ['embedding', 'pages']:
    by = {v: [next(m for m in r['measurements'] if m['name'] == name) for r in runs if r['variant'] == v] for v in executables}
    medians = {v: statistics.median(m['p50_ms'] for m in values) for v, values in by.items()}
    summary.append(dict(name=name, baseline_p50_ms=medians['baseline'],candidate_p50_ms=medians['candidate'],speedup=medians['baseline']/medians['candidate'],
                        baseline_allocations=by['baseline'][0]['allocations'], candidate_allocations=by['candidate'][0]['allocations'],
                        baseline_requested_bytes=by['baseline'][0]['requested_bytes'],candidate_requested_bytes=by['candidate'][0]['requested_bytes']))
a.output.write_text(json.dumps(dict(scope='Test-only allocation request counts/volume, not live RAM or app RSS',executables={v:dict(path=str(path),sha256=hashlib.sha256(path.read_bytes()).hexdigest()) for v,path in executables.items()},runs=runs,summary=summary),indent=2)+'\n')
for row in summary:print(json.dumps(row))
