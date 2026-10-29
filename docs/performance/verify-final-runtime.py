#!/usr/bin/env python3
"""Keep post-review verification separate from the frozen four-process A/B run."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess

spec = importlib.util.spec_from_file_location('summary', Path(__file__).with_name('summarize-runtime.py'))
summary = importlib.util.module_from_spec(spec)
spec.loader.exec_module(summary)

p = argparse.ArgumentParser()
p.add_argument('--artifacts', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
r = a.artifacts

def artifact(path):
    return dict(path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest())

raw_path = r/'post-review-desktop-candidate.json'
raw = json.loads(raw_path.read_text())
assert raw['completed'] and raw['protocol'] == 'visibility-and-durable-completion-v2'
measured_path = Path(__file__).with_name('beta17-runtime-candidate.json')
measured = json.loads(measured_path.read_text())
assert raw['launcher_counts'] == measured['runs'][0]['launcher_counts']
assert raw['knowledge_counts'] == measured['runs'][0]['knowledge_counts']
assert raw['metrics']['vectors_sha256'] == measured['runs'][0]['phases']['vectors_sha256']
assert raw['metrics']['model_after_images'] == 'ready'
assert raw['metrics']['image_retention']['mounted']['loaded'] == 96
assert raw['metrics']['image_retention']['released']['unrevoked'] == 0
for phase, visible in [('hidden_idle',False),('visible_idle',True),('post_features',False),('active_model_idle',False)]:
    samples = [s for s in raw['samples'] if s['phase'] == phase]
    assert len(samples) >= 40 and all(s['main_on_screen'] == visible for s in samples)

test_path = r/'runtime-final-release-tests.log'
test_log = test_path.read_text()
counts = re.search(r'test result: ok\. (\d+) passed; 0 failed; (\d+) ignored;', test_log)
assert counts
assert 'unselected_download_keeps_progress_observation_active_then_returns_to_sleep ... ok' in test_log
parity_path = r/'runtime-final-search-parity.json'
parity = json.loads(parity_path.read_text())
assert parity['exactOrderedIdScoreParity'] and len(parity['runs']) == 4 and len(parity['signatures']) == 10
test_binary = r/'runtime-final-tests'
assert artifact(test_binary)['sha256'] == parity['executables']['candidate']['sha256']
production = r/'production-final-app'
desktop = r/'desktop-final-app'
assert raw['binary_sha256'] == artifact(desktop)['sha256']
for marker in [b'TEZBAR_BENCH_HOME', b'__desktop_bench_reply']:
    assert marker not in production.read_bytes() and marker in desktop.read_bytes()
for name in ['runtime-final-desktop-build','runtime-final-production-build']:
    assert 'Finished `release` profile' in (r/(name+'.log')).read_text()

root = Path(__file__).resolve().parents[2]
paths = subprocess.check_output(['git','diff','354689e','--name-only'], cwd=root, text=True).splitlines()
source = {name:hashlib.sha256((root/name).read_bytes()).hexdigest() for name in paths if name.startswith(('src/','src-tauri/'))}
result = dict(schemaVersion=1,
    scope='One additional full release desktop validation after download-progress cadence correction; not a replacement paired comparison or a new speedup claim.',
    paired_measurement_source_commit=subprocess.check_output(['git','rev-parse','9ad23a9'], cwd=root, text=True).strip(),
    paired_measurements=artifact(measured_path),
    post_measurement_change='Keep 250ms progress observation during downloads, independently of indexing selection/pause; new regression covers active and quiescent transitions. No download occurred in the paired workload.',
    source_sha256=source,
    checks=dict(rust_passed=int(counts[1]),rust_ignored=int(counts[2]),rust_log=artifact(test_path),
                exact_40_case_search_parity=artifact(parity_path),desktop_build=artifact(r/'runtime-final-desktop-build.log'),
                production_build=artifact(r/'runtime-final-production-build.log')),
    binaries=dict(desktop=artifact(desktop),production=artifact(production)),
    frozen_test_executable=artifact(test_binary),
    final_text_probe=artifact(Path(__file__).with_name('beta17-runtime-text.json')),
    desktop_validation=dict(raw=artifact(raw_path),log=artifact(r/'post-review-desktop-candidate.log'),
                            ready_ms=raw['ready_ms'],first_ui_ms=raw['first_ui_ms'],
                            launcher_counts=raw['launcher_counts'],knowledge_counts=raw['knowledge_counts'],
                            phases={k:summary.compact_phase(v) if isinstance(v,dict) and 'wall_s' in v else v for k,v in raw['metrics'].items()}))
a.output.write_text(json.dumps(result,indent=2)+'\n')
print('Verified complete final desktop, builds, release suite and exact search parity:', a.output)
