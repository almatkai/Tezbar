#!/usr/bin/env python3
"""Interleaved fresh-process stock/control/patch comparison; no global cache edits."""
import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import sys


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--model', required=True)
    p.add_argument('--supervisor', required=True)
    p.add_argument('--libraries', type=Path, required=True)
    a = p.parse_args()
    a.output.mkdir(exist_ok=False)
    runs = []
    # Each treatment occupies every ordinal position twice. Fresh process/profile,
    # warm/uncontrolled OS + Metal caches, not cold-install measurements.
    orders = [('stock', 'control', 'patched'), ('patched', 'stock', 'control'),
              ('control', 'patched', 'stock'), ('control', 'stock', 'patched'),
              ('stock', 'patched', 'control'), ('patched', 'control', 'stock')]
    exact = None
    try:
        for round_, order in enumerate(orders):
            for kind in order:
                out = a.output / f'{round_+1}-{kind}'
                cmd = [sys.executable, str(Path(__file__).with_name('model-memory.py')),
                       '--output', str(out), '--model', a.model, '--supervisor', a.supervisor,
                       '--config', 'poll0', '--normal-only', '--idle', '0.2']
                if kind != 'stock':
                    cmd += ['--library', str(a.libraries / ('libllama-control.dylib' if kind == 'control' else 'libllama-no-logits.dylib'))]
                with (a.output / (out.name+'.log')).open('x') as log:
                    code = subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT).returncode
                row = dict(round=round_+1, kind=kind, code=code, path=str(out))
                if (out/'result.json').exists():
                    r = json.loads((out/'result.json').read_text())
                    row.update(ready_ms=r.get('ready_ms'), completed=r['completed'],
                               workloads=r['workloads'], pressure_before=r['pressure_before'],
                               pressure_after=r['pressure_after'], survivors=r.get('owned_survivors'))
                    payload = json.loads((out/'vectors.json').read_text())
                    if exact is None:
                        exact = payload
                    assert payload == exact, 'raw 768-value response mismatch'
                    assert not row['survivors']
                runs.append(row)
                assert code == 0, row
        summary = {}
        for kind in ['stock', 'control', 'patched']:
            rows = [r for r in runs if r['kind'] == kind]
            summary[kind] = dict(ready_ms=[r['ready_ms'] for r in rows],
                ready_median_ms=statistics.median(r['ready_ms'] for r in rows),
                first_query_ms=[r['workloads'][0]['timings_ms'][0] for r in rows],
                chunks16_first_ms=[r['workloads'][3]['timings_ms'][0] for r in rows])
        print(json.dumps(summary, indent=2))
    finally:
        (a.output/'result.json').write_text(json.dumps(dict(runs=runs, summary=locals().get('summary'),
            label='fresh processes; warm/uncontrolled OS and shader caches; no builds concurrent'), indent=2)+'\n')


if __name__ == '__main__':
    main()
