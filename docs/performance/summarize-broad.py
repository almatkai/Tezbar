#!/usr/bin/env python3
"""Freeze R14's complete cohorts, parity, source/package hashes and cleanup audit."""
import argparse
import ctypes
import hashlib
import json
import math
from pathlib import Path
import shutil
import sqlite3
import statistics
import subprocess
from desktop import LIB, Usage


def sha(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as f:
        for block in iter(lambda: f.read(1024*1024), b''):
            h.update(block)
    return h.hexdigest()


def aggregate(values):
    return dict(median=statistics.median(values), values=values, range=[min(values), max(values)])


def percentile(values, p):
    return sorted(values)[math.ceil(len(values)*p/100)-1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--artifacts', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    root = args.artifacts.resolve()
    repo = Path(__file__).resolve().parents[2]
    load = lambda p: json.loads((root/p).read_text())
    native = load('native-final/runs.json') + load('native-reverse/runs.json')
    assert len(native) == 16
    prior = json.loads((repo/'build/performance-runtime-20261030/cumulative-20261030/native-comparison.json').read_text())
    identities = set()
    original_comparisons = 0
    signatures = {}
    for run in native:
        for row in run['rows']:
            signatures.setdefault(row['name'], row['full_results'])
            assert signatures[row['name']] == row['full_results']
            if row['name'] in prior['signatures']:
                assert row['results'] == prior['signatures'][row['name']]
                original_comparisons += 1
        identities.update((run['pid'], u['start']) for u in run['memory_samples'])
    fields = ['first_ms', 'first_cpu_ms', 'p50_ms', 'p95_ms', 'cpu_p50_ms', 'cpu_p95_ms']
    mutation = load('mutation-seeded.json')
    assert mutation['exit'] == 0 and len(mutation['measurements']) == 60
    assert mutation['coupled_seeds']['complete_ordered_comparisons'] == 192
    actual_launcher = (repo/'src-tauri/src/native_core/launcher/db.rs').read_text()
    assert (root/'mutation-db.rs').read_text() == actual_launcher.replace('let mut docs = HashMap::new();', 'let mut docs = HashMap::with_hasher(crate::hash_seed());', 1)
    identities.update((mutation['pid'], u['start']) for u in mutation['memory_samples'])
    mutation_summary = {}
    for phase in range(5):
        mutation_summary[str(phase)] = {}
        for query in ['document', 'archive project', 'project document']:
            rows = [r for r in mutation['measurements'] if r['phase'] == phase and r['query'] == query]
            assert len(rows) == 4 and all(r['full_results'] == rows[0]['full_results'] for r in rows)
            mutation_summary[str(phase)][query] = {label: {key: aggregate([r[key] for r in rows if r['label'] == label]) for key in fields} for label in ['baseline', 'candidate']}
    native_summary = {name: {label: {key: aggregate([row[key] for run in native if run['label'] == label
                      for row in run['rows'] if row['name'] == name]) for key in fields}
                      for label in ['baseline', 'candidate']} for name in signatures}
    names = ['desktop-baseline-1b', 'desktop-candidate-1b', 'desktop-candidate-2c', 'desktop-baseline-2']
    desktop = {name: load(name+'/result.json') for name in names}
    burst_names = ['simple_bursts', 'simple_broad_bursts', 'deep_lexical_bursts', 'deep_lexical_broad_bursts',
                   'deep_model_bursts', 'deep_model_broad_bursts', 'repeat_deep_model_bursts']
    desktop_summary = {}
    for case in burst_names:
        desktop_summary[case] = {}
        for label in ['baseline', 'candidate']:
            runs = [d for name, d in desktop.items() if label in name]
            values = dict(cpu_s=[], response_p50_ms=[], response_p95_ms=[], frame_p50_ms=[], frame_p95_ms=[], dispatched=[])
            for d in runs:
                metric = d['metrics'][case]
                values['cpu_s'].append(metric['cpu_s'])
                values['dispatched'].append(sum(v['dispatched'] for v in metric['value']))
                for field, column in [('response', 'responseMs'), ('frame', 'inputToTwoFramesMs')]:
                    for p in [50, 95]:
                        values[f'{field}_p{p}_ms'].append(percentile([v[column] for v in metric['value']], p))
            desktop_summary[case][label] = {key: aggregate(v) for key, v in values.items()}
    family = {}
    roles = {}
    for phase in ['hidden_idle', 'post_search_idle', 'active_model_idle', 'post_repeat_idle']:
        family[phase] = {}
        roles[phase] = {}
        for label in ['baseline', 'candidate']:
            runs = [d for name, d in desktop.items() if label in name]
            keys = ['footprint_median_bytes', 'rss_last_bytes', 'cpu_s', 'interrupt_wakeups']
            family[phase][label] = {key: aggregate([d['metrics'][phase][key] for d in runs]) for key in keys}
            family[phase][label]['footprint_p95_bytes'] = aggregate([
                percentile([sum(p['footprint'] for p in s['processes']) for s in d['samples'] if s['phase'] == phase], 95)
                for d in runs])
            role_names = sorted({p['role'] for d in runs for s in d['samples'] if s['phase'] == phase for p in s['processes']})
            roles[phase][label] = {}
            for role in role_names:
                roles[phase][label][role] = {field: aggregate([
                    statistics.median([sum(p[field] for p in s['processes'] if p['role'] == role) for s in d['samples'] if s['phase'] == phase])
                    for d in runs]) for field in ['footprint', 'rss']}
    index = {}
    for label in ['baseline', 'candidate']:
        runs = [d for name, d in desktop.items() if label in name]
        index[label] = dict(durable_seconds=aggregate([d['metrics']['semantic_index_with_bursts']['value']['actual_complete_s'] for d in runs]),
                            durable_family_cpu_s=aggregate([d['metrics']['semantic_index_with_bursts']['value']['durable_family']['cpu_s'] for d in runs]),
                            source_burst_cpu_s=aggregate([d['metrics']['source_index_with_bursts']['cpu_s'] for d in runs]))
    closed = {}
    fixture_digests = set()
    for name, d in desktop.items():
        assert d['completed'] and not d['survivors']
        assert not d['metrics']['bridge_totals']['errors']
        assert d['metrics']['vectors_sha256'] == '27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c'
        launcher = Path(d['root'])/'data/search/index.sqlite3'
        with sqlite3.connect('file:'+str(launcher)+'?mode=ro&immutable=1', uri=True) as db:
            assert db.execute("SELECT count(*) FROM documents WHERE category='files'").fetchone()[0] == 5000
        fixture_hash = hashlib.sha256()
        documents = sorted((Path(d['root'])/'knowledge-docs').glob('*.txt'))
        assert len(documents) == 96
        for document in documents:
            fixture_hash.update(document.name.encode())
            fixture_hash.update(document.read_bytes())
        fixture_digests.add(fixture_hash.hexdigest())
        source = Path(d['root'])/'data/knowledge/knowledge.sqlite3'
        dest = root/'closed-fixtures'/name/'knowledge.sqlite3'
        dest.parent.mkdir(parents=True, exist_ok=True)
        if not dest.exists():
            shutil.copy2(source, dest)
        assert sha(source) == sha(dest)
        # Read-only post-shutdown data inspection; never host-SQLite search timing.
        with sqlite3.connect('file:'+str(dest)+'?mode=ro&immutable=1', uri=True) as db:
            counts = [db.execute('SELECT count(*) FROM '+table).fetchone()[0]
                      for table in ['knowledge_sources', 'knowledge_chunks', 'knowledge_gemma_embeddings']]
            assert counts == [96, 192, 192]
            model = db.execute('SELECT model_key,count(*),sum(length(vector)) FROM knowledge_gemma_embeddings GROUP BY model_key').fetchall()
            assert len(model) == 1 and model[0][1:] == (192, 192*256*4)
        closed[name] = dict(path=str(dest.relative_to(root)), sha256=sha(dest), counts=counts, model=model,
                            launcherFiles=5000, fixtureDocumentsSha256=fixture_hash.hexdigest())
    assert len(fixture_digests) == 1
    parity = load('parity-opening.json')
    retention = load('retention90.json')
    ordinary = load('ordinary-activation-retry/result.json')
    assert parity['completed'] and parity['candidate_ime_hide_recovery'] and parity['exact_final_query_parity']
    assert retention['completed'] and retention['exact_ordered_parity'] and all(r['same_model_process'] for r in retention['runs'])
    assert ordinary['completed'] and ordinary['old_manifests_unchanged'] and not ordinary['survivors']
    attempts = {}
    for path in sorted(root.glob('desktop-*/result.json')):
        d = json.loads(path.read_text())
        assert not d['survivors']
        attempts[path.parent.name] = dict(completed=d['completed'], error=d.get('error'), root=d['root'], observed_steps=list(d['metrics']))
        for sample in d['samples']:
            identities.update((p['pid'], p['start']) for p in sample['processes'])
    for run in retention['runs']:
        assert not run['survivors']
        for sample in run['samples']:
            identities.update((p['pid'], p['start']) for p in sample['processes'])
    for run in parity['openings'] + parity['parity']:
        assert not run['survivors']
        identities.update((p['pid'], p['start']) for p in run['owned_family'])
    for sample in ordinary['samples']:
        identities.update((p['pid'], p['start']) for p in sample['processes'])
    survivors = []
    for pid, start in sorted(identities):
        u = Usage()
        if LIB.proc_pid_rusage(pid, 4, ctypes.byref(u)) == 0 and u.proc_start_abstime == start:
            survivors.append(dict(pid=pid, start=start))
    assert not survivors
    cleanup = dict(observed_identities=len(identities), survivors=survivors, identities=[dict(pid=p, start=s) for p, s in sorted(identities)])
    (root/'cleanup.json').write_text(json.dumps(cleanup, indent=2))
    for path, digest in load('installed-models-before.json').items():
        assert sha(path) == digest, path
    assert sha(repo/'docs/performance/beta17-results-report.html') == '1ee1b0742261dbfb1ad8071817778b5881cb82c084bda482684117279f45816d'
    assert sha(repo/'build/embedding-runtime/resources/embedding-runtime.zip') == '75698aa84eb35398cee7b3afb6fee7b9bdb1db7a3aedc3cc43af25780100e475'
    native_fixture = Path((root/'fixture-path.txt').read_text())
    dest = root/'closed-fixtures/native-launcher.sqlite3'
    if not dest.exists():
        shutil.copy2(native_fixture, dest)
    assert sha(dest) == sha(native_fixture)
    paths = subprocess.check_output(['git', 'ls-files'], cwd=repo, text=True).splitlines()
    paths += ['src-tauri/src/native_core/fts_rank.rs', 'src-tauri/src/native_core/fts_rank_math.c']
    source_pins = {p: sha(repo/p) for p in sorted(set(paths)) if (p.startswith(('src/', 'src-tauri/', 'scripts/')) or p in ['package.json', 'pnpm-lock.yaml', 'vite.config.ts', 'tsconfig.json', 'index.html'])
                   and Path(p).name != 'CONTEXT.md' and '/gen/' not in p and (repo/p).is_file()}
    (root/'source-pins.json').write_text(json.dumps(source_pins, indent=2))
    harness_pins = {str(p.relative_to(repo)): sha(p) for p in sorted((repo/'docs/performance').glob('*.py'))}
    (root/'harness-pins.json').write_text(json.dumps(harness_pins, indent=2))
    exe_pins = {name: sha(root/name) for name in ['baseline-native-tests', 'candidate-native-tests', 'candidate-ordinary', 'candidate-bench']}
    for kind, inventory in load('apps-sha256.json').items():
        for relative, digest in inventory.items():
            assert sha(root/('candidate-'+kind+'.app')/relative) == digest
        assert sha(root/('candidate-'+kind+'.app/Contents/MacOS/tezbar')) == exe_pins['candidate-'+kind]
    assert sha(repo/'build/performance-runtime-20261030/cumulative-20261030/accepted-bench.app/Contents/MacOS/tezbar') == '1cbb16091ef42dc0fe72d009d0d84046663ead874e27b3cbb30ec6fc5923ffac'
    report = dict(schemaVersion=1, taskBase='b01f7deedad3fd43da12e330d83b937a1bfb8649',
                  scope='Exact native rank improvement; not a uniform 5x/one-fifth RAM claim; local macOS arm64 only',
                  exactNativeCompleteReplies=sum(len(r['rows']) for r in native), originalBeta17OrderedIdScoreComparisons=original_comparisons,
                  native=native_summary, mutationTrace=dict(scope=mutation['scope'], summary=mutation_summary, writes=mutation['mutations'], timedQueries=2400,
                      completeOrderedComparisons=60, coupledSeeds=64, additionalOrderedSeedComparisons=192, exactOrderedFtsPages=15,
                      sharedProcessPeakFootprint=max(u['footprint'] for u in mutation['memory_samples']), sharedProcessPeakRss=max(u['rss'] for u in mutation['memory_samples']),
                      inheritedLimitation='Equal final rounded-score/timestamp ties depend on baseline RandomState. Production tie behavior unchanged; supplemental full-order oracle couples map seeds, never sorts away differences.'), nativeMemory=[{k: r[k] for k in ['label', 'test', 'max_observed_footprint', 'max_observed_rss', 'lifetime_max_footprint']} for r in native],
                  desktop=desktop_summary, family=family, roles=roles, indexing=index, attempts=attempts,
                  opening={label: aggregate([r['first_ui_ms'] for r in parity['openings'] if r['variant'] == label]) for label in ['baseline', 'candidate']},
                  retention=[{k: run[k] for k in ['variant', 'same_model_process', 'first', 'recovery']} for run in retention['runs']],
                  artifactRoot=str(root.relative_to(repo)), executables=exe_pins, sourcePinCount=len(source_pins), harnessPins=harness_pins,
                  closedSyntheticFixtures=closed, cpuCalibration=load('cpu-calibration.json'), observedPidStartIdentities=len(identities), survivors=[],
                  ordinaryActivationPassed=True, exactDesktopParityPassed=True, syntheticImeHideRecoveryPassed=True,
                  interpretation=dict(nativeFirst='first per-case call; uncontrolled OS cache, not disk-cold; cold semantic clears only app vector cache',
                                      percentiles='nearest rank within each process; median of process summaries, not pooled percentiles or confidence intervals',
                                      memory='physical-footprint ledger sums are not deduplicated RAM; summed RSS can double-count; .5s desktop/20ms native sampling can miss brief peaks',
                                      cohorts='four complete P/C/C/P desktop runs; every invalidated attempt retained separately, never mixed into accepted cohorts; native P/C/C/P plus reversed C/P/P/C',
                                      arithmetic='same C formula/toolchain defaults; strict bitwise tests required across compiler/platform changes; shadow format guarded to SQLite 3.46.0'))
    # Raw manifest excludes itself; no live profiles or clipboard stores.
    manifest = {str(p.relative_to(root)): sha(p) for p in sorted(root.rglob('*')) if p.is_file() and p.name != 'artifact-manifest.json'}
    (root/'artifact-manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    report['artifactManifest'] = dict(path=str((root/'artifact-manifest.json').relative_to(repo)), sha256=sha(root/'artifact-manifest.json'), files=len(manifest))
    report['reportDocumentSha256'] = sha(args.output.with_suffix('.md'))
    args.output.write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(dict(completeNativeReplies=report['exactNativeCompleteReplies'], originalSignatures=original_comparisons,
                         sourcePins=len(source_pins), artifacts=len(manifest), observedIdentities=len(identities), survivors=survivors)))


if __name__ == '__main__':
    main()
