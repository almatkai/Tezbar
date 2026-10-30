#!/usr/bin/env python3
"""Freeze cumulative original-beta.17 evidence without combining round ratios."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import sqlite3
import statistics
import subprocess
import shutil
import re
from desktop import processes, summarize

ROOT = Path(__file__).resolve().parents[2]
MIB = 1024 ** 2


def sha(p):
    h = hashlib.sha256()
    with p.open('rb') as f:
        for b in iter(lambda: f.read(1024 * 1024), b''):
            h.update(b)
    return h.hexdigest()


def percentile(values, p):
    values = sorted(values)
    return values[math.ceil(len(values) * p / 100) - 1]


def aggregate(values):
    return dict(median=statistics.median(values), range=[min(values), max(values)], values=values)


def pair(b, c):
    ratio = b['median'] / c['median'] if c['median'] else None
    return dict(original=b, final=c, ratio=ratio, target5xPassed=ratio is not None and ratio >= 5)


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--artifacts', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    out = a.artifacts.resolve()
    load = lambda name: json.loads((out / name).read_text())
    reference = load('reference-receipt.json')
    calibration = load('accounting-calibration.json')
    assert calibration['recordedCpuScaleToSeconds'] == 1., 'Review raw CPU units before publishing seconds'
    assert reference['base'] == '9ff3663342b48ab01be2e56ab30f6d7123b0ced8'
    # Recheck authored original inputs after both builds, not only before them.
    original_checked = 0
    for name, digest in {**reference['originalFiles'], **reference['addedFiles']}.items():
        if name.startswith(('src/', 'src-tauri/', 'scripts/')) and '/gen/' not in name:
            expected = reference['changedFiles'].get(name, digest)
            assert sha(out / 'original-source' / name) == expected, name
            original_checked += 1
    native = load('native-comparison.json')
    assert native['exactOrderedIdScoreParity'] and len(native['runs']) == 4
    for case in native['summary']:
        case['target5xP50Passed'] = case['p50_speedup'] >= 5
        case['target5xP95Passed'] = case['baseline_p95_median_ms'] / case['candidate_p95_median_ms'] >= 5
    desktop = {name: load(name + '/result.json') for name in
               ['original-1', 'final-1', 'final-2', 'original-2', 'long-original', 'long-final']}
    desktop_receipts = {}
    for name, run in desktop.items():
        assert run['completed'], name
        assert load(name + '/cleanup.json')['owned_survivors'] == [], name
        with sqlite3.connect('file:' + str(Path(run['root']) / 'data/search/index.sqlite3') + '?mode=ro&immutable=1', uri=True) as db:
            assert db.execute("SELECT count(*) FROM documents WHERE category='files'").fetchone()[0] == 5000
        knowledge = Path(run['root']) / 'data/knowledge/knowledge.sqlite3'
        # All owners have exited; use the checkpointed, immutable DB without
        # asking Python SQLite to recreate WAL sidecars. Preserve the synthetic
        # database after timing so OS-temp cleanup cannot erase this receipt.
        saved = out / 'closed-fixtures' / name / 'knowledge.sqlite3'
        saved.parent.mkdir(parents=True, exist_ok=True)
        if not saved.exists(): shutil.copy2(knowledge, saved)
        assert sha(saved) == sha(knowledge)
        with sqlite3.connect('file:' + str(saved) + '?mode=ro&immutable=1', uri=True) as db:
            assert db.execute('SELECT count(*) FROM knowledge_sources').fetchone()[0] == 96
            assert db.execute('SELECT count(*) FROM knowledge_chunks').fetchone()[0] == 192
            assert db.execute('SELECT count(*) FROM knowledge_gemma_embeddings').fetchone()[0] == 192
            model_identity = db.execute('SELECT model_key,count(*),sum(length(vector)) FROM knowledge_gemma_embeddings GROUP BY model_key').fetchall()
            assert model_identity == [(run['runtime_status']['modelKey'], 192, 192 * 256 * 4)]
        expected = ('5ac291cdacc7493c03dab643d5ab1567d31f9cbc8a694b139d2a91aedf7c9ab2'
                    if name.startswith('long-') else '27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c')
        assert run['metrics']['vectors_sha256'] == expected
        assert not run['metrics']['bridge_totals']['errors']
        docs = hashlib.sha256()
        for f in sorted((Path(run['root']) / 'knowledge-docs').glob('*.txt')):
            docs.update(f.name.encode()); docs.update(f.read_bytes())
        desktop_receipts[name] = dict(root=run['root'],binarySha256=run['binary_sha256'],
            fixtureDocumentsSha256=docs.hexdigest(),runtimeStatus=run['runtime_status'],modelIndexIdentity=model_identity,
            vectorsSha256=expected,launcherFiles=5000,knowledgeSources=96,chunks=192,
            closedSyntheticKnowledgeDb=dict(path=str(saved),sha256=sha(saved)))
        maps = '\n'.join(Path(v['path']).read_text() for v in run['runtime_paths'] if v['code'] == 0)
        expected_map = ('/knowledge/runtimes/75698aa84eb35398cee7b3afb6fee7b9bdb1db7a3aedc3cc43af25780100e475/libllama'
                        if name in ['final-1', 'final-2', 'long-final'] else '/knowledge/models/embeddinggemma-2-q4_k_m/runtime/')
        assert expected_map in maps and 'libllama' in maps, name
        desktop_receipts[name]['expectedRuntimeLibraryMapped'] = True
    assert len({desktop_receipts[n]['fixtureDocumentsSha256'] for n in ['original-1','original-2','final-1','final-2']}) == 1
    assert desktop_receipts['long-original']['fixtureDocumentsSha256'] == desktop_receipts['long-final']['fixtureDocumentsSha256']
    assert len({r['runtimeStatus']['modelKey'] for r in desktop_receipts.values()}) == 1
    groups = [[desktop[n] for n in names] for names in [('original-1', 'original-2'), ('final-1', 'final-2')]]
    ui = {}
    for phase in ['search_open_close', 'simple_bursts', 'deep_lexical_bursts', 'deep_model_bursts',
                  'repeat_deep_model_bursts', 'source_index_with_bursts', 'semantic_index_with_bursts']:
        row = {}
        fields = ['inputToPaintMs'] if phase == 'search_open_close' else ['responseMs', 'inputToTwoFramesMs', 'inputToCorrectDomObservedMs']
        for field in fields:
            for percentile_value in [50, 95]:
                measurements = []
                for runs in groups:
                    vals = []
                    for run in runs:
                        payload = run['metrics'][phase]['value']
                        bursts = payload['bursts'] if isinstance(payload, dict) else payload
                        vals.append(percentile([x[field] for x in bursts], percentile_value))
                    measurements.append(aggregate(vals))
                row[field + '_p' + str(percentile_value)] = pair(*measurements)
        row['familyCpuSeconds'] = pair(*[aggregate([r['metrics'][phase]['cpu_s'] for r in runs]) for runs in groups])
        row['dispatched'] = [[sum(v.get('dispatched', 0) for v in
            (r['metrics'][phase]['value'].get('bursts', []) if isinstance(r['metrics'][phase]['value'], dict)
             else r['metrics'][phase]['value'])) for r in runs] for runs in groups]
        ui[phase] = row
    memory = {}
    for phase in ['hidden_idle', 'post_search_idle', 'active_model_idle', 'post_repeat_idle']:
        metrics = {}
        for field in ['footprint_median_bytes', 'footprint_last_bytes', 'footprint_peak_bytes', 'rss_last_bytes',
                      'cpu_s', 'interrupt_wakeups', 'idle_wakeups', 'wall_s']:
            metrics[field] = pair(*[aggregate([r['metrics'][phase][field] for r in runs]) for runs in groups])
        metrics['footprint_p95_bytes'] = pair(*[aggregate([percentile([
            sum(p['footprint'] for p in s['processes']) for s in r['samples'] if s['phase'] == phase], 95)
            for r in runs]) for runs in groups])
        roles = set(p['role'] for runs in groups for r in runs for s in r['samples']
                    if s['phase'] == phase for p in s['processes'])
        metrics['last_sample_components'] = {role: {field: pair(*[aggregate([
            sum(p[field] for p in [s for s in r['samples'] if s['phase'] == phase][-1]['processes']
                if p['role'] == role) for r in runs]) for runs in groups])
            for field in ['footprint', 'rss']} for role in sorted(roles)}
        metrics['activity_components'] = {role: {field: pair(*[aggregate([
            summarize([dict(s, processes=[p for p in s['processes'] if p['role'] == role])
                       for s in r['samples'] if s['phase'] == phase])[field]
            for r in runs]) for runs in groups]) for field in ['cpu_s', 'interrupt_wakeups']}
            for role in sorted(roles)}
        memory[phase] = metrics
    indexing = {}
    for field in ['actual_complete_s', 'published_complete_s']:
        indexing[field] = pair(*[aggregate([r['metrics']['semantic_index_with_bursts']['value'][field] for r in runs]) for runs in groups])
    indexing['durableCpuSeconds'] = pair(*[aggregate([r['metrics']['semantic_index_with_bursts']['value']['durable_family']['cpu_s'] for r in runs]) for runs in groups])
    long = {}
    for phase in ['active_model_idle', 'post_repeat_idle']:
        long[phase] = {k: pair(*[aggregate([desktop[n]['metrics'][phase][k]]) for n in ['long-original', 'long-final']])
                       for k in ['footprint_median_bytes', 'rss_last_bytes', 'cpu_s', 'interrupt_wakeups']}
    long['indexing'] = {k: pair(*[aggregate([desktop[n]['metrics']['semantic_index_with_bursts']['value'][k]])
                                  for n in ['long-original', 'long-final']]) for k in ['actual_complete_s', 'published_complete_s']}
    parity, retention, ordinary = load('ui-parity-opening.json'), load('retention90.json'), load('ordinary-activation/result.json')
    assert parity['completed'] and parity['exact_final_query_parity'] and parity['candidate_ime_hide_recovery']
    assert all(not r['survivors'] for r in parity['openings'] + parity['parity'])
    assert retention['completed'] and all(r['same_model_process'] and not r['survivors'] for r in retention['runs'])
    assert ordinary['completed'] and ordinary['old_manifests_unchanged'] and not ordinary['survivors']
    openings = pair(*[aggregate([r['first_ui_ms'] for r in parity['openings'] if r['variant'] == v])
                      for v in ['baseline', 'candidate']])
    # Exact unchanged bytes are the explicit basis for reusing the R6 exhaustive
    # raw-vector and boundary runs. Their timings are not cumulative measurements.
    previous_path = ROOT / 'docs/performance/beta17-runtime-package-results.json'
    previous = json.loads(previous_path.read_text())
    catalog_path = ROOT / 'src-tauri/runtime/embedding-macos-arm64.json'
    catalog = json.loads(catalog_path.read_text())
    assert catalog == previous['catalog']
    reused = {}
    for name, record in previous['evidence'].items():
        if name.startswith(('exact-', 'bounds-')):
            raw = Path(previous['artifacts']) / name
            assert sha(raw) == record['sha256'], raw
            reused[name] = record
    for name in ['accepted-ordinary', 'accepted-bench', 'original-ordinary', 'original-bench']:
        raw = (out / name).read_bytes()
        for marker in [b'TEZBAR_BENCH_HOME', b'__desktop_bench_reply']:
            assert (marker in raw) == name.endswith('-bench'), name
    package = out / 'accepted-ordinary.app/Contents/Resources/embedding-runtime/embedding-runtime.zip'
    assert sha(package) == catalog['archiveSha256']
    assert sha(ROOT / 'build/embedding-runtime/resources/embedding-runtime.zip') == catalog['archiveSha256']
    source_hashes = load('accepted-source-sha256.json')
    for name, digest in source_hashes.items():
        assert sha(ROOT / name) == digest, name
    before = load('installed-models-before.json')
    for name, record in before.items():
        weights = Path(record['weights'])
        assert sha(weights) == record['weightsSha256']
        for file, digest in record['files'].items():
            assert sha(weights.parent / file) == digest
    assert re.search(r'test result: ok\. 450 passed; 0 failed; 8 ignored;', (out/'accepted-rust-suite.log').read_text())
    assert '747 passed' in (out/'accepted-typescript.log').read_text()
    assert '9 skipped' in (out/'accepted-typescript.log').read_text()
    assert 'Ran 5 tests' in (out/'accepted-package-check.log').read_text()
    assert 'pass 16' in (out/'accepted-release-policy.log').read_text()
    assert 'test result: ok. 1 passed; 0 failed;' in (out/'accepted-worker.log').read_text()
    assert all(c['exit'] == 0 for c in load('acceptance-commands.json'))
    identities = set()
    def walk(v):
        if isinstance(v, dict):
            if all(k in v for k in ['pid', 'start', 'footprint']):
                identities.add((v['pid'], v['start']))
            for x in v.values(): walk(x)
        elif isinstance(v, list):
            for x in v: walk(x)
    for f in out.rglob('*.json'):
        if 'original-source' not in f.parts:
            try: walk(json.loads(f.read_text()))
            except json.JSONDecodeError: pass
    survivors = [p for pid, start in identities for p in processes(pid) if p['pid'] == pid and p['start'] == start]
    assert not survivors, survivors
    binaries = {n: dict(sha256=sha(out/n), bytes=(out/n).stat().st_size) for n in [
        'original-ordinary', 'original-bench', 'original-native-tests', 'accepted-ordinary', 'accepted-bench', 'accepted-native-tests']}
    app_files = {name: {str(p.relative_to(out/name)): dict(sha256=sha(p),bytes=p.stat().st_size)
                       for p in (out/name).rglob('*') if p.is_file()}
                 for name in ['original-ordinary.app','original-bench.app','accepted-ordinary.app','accepted-bench.app']}
    for run in parity['parity']:
        for value in run['values']:
            value['full_rows_sha256'] = hashlib.sha256(json.dumps(value['rows'], sort_keys=True, separators=(',', ':')).encode()).hexdigest()
            value['rows'] = [{k:v for k,v in row.items() if k not in ['icon', 'iconUrl', 'thumbnail']} for row in value['rows']]
    result = dict(schemaVersion=1, base=reference['base'], candidateParent='ac14c06bfe6435c83e5399f39ec8158227732d6c',
        candidateProductionChangesThisRound=False, sourceSha256=source_hashes, artifacts=str(out), binaries=binaries, appFiles=app_files,
        host=load('host-toolchain.json'), reference={k:v for k,v in reference.items() if k != 'originalFiles'},
        provenanceReceipt=dict(path=str(out/'reference-receipt.json'), sha256=sha(out/'reference-receipt.json'),authoredOriginalInputsRechecked=original_checked),
        protocol=dict(order=['original-1','final-1','final-2','original-2'],
            fixtures=dict(launcherNativeRows=100400,deepNativeRows=10000,desktopFiles=5000,documents=96,vectors=192),
            scope='macOS arm64; synthetic corpora, complete observed responsible-PID/descendant families; same Q4 weights/settings',
            percentiles='nearest-rank within process, median of process p50/p95; n=2 ordinary processes/build; no pooled percentile or confidence interval',
            cache='fresh isolated profiles/processes, warm/uncontrolled OS/shader caches; cold semantic clears only application vector cache',
            accounting='summed physical-footprint ledgers, not deduplicated RAM; summed RSS may count shared pages; 0.5-second ownership sampling can miss short-lived helpers',
            desktop='Complete actual packaged app cloned into isolated benchmark identity wrapper; same opt-in harness, production frontend/search/runtime; ordinary package separately activated',
            timing='No task builds/tests overlap timed phases; physical input interference and locked-display/path guards enforced'),
        accountingCalibration=calibration, fixtureDataHygiene=load('fixture-data-hygiene.json'),
        harnessSha256={str(p.relative_to(ROOT)):sha(p) for p in Path(__file__).parent.glob('*.py')},
        harnessRevisionNote='Durable-completion sampling was present for all six primary runs. Later changes add ownership metadata and post-exit private-store cleanup only; no timed path/cadence/probe changed. Opt-in Rust harness is identical on both sides.',
        native=native, desktopReceipts=desktop_receipts, ui=ui, memory=memory, indexing=indexing, longChunksSinglePair=long,
        openings=openings, uiParity=parity, retention90={k:v for k,v in retention.items() if k!='runs'},
        retentionRuns=[{k:v for k,v in r.items() if k!='samples'} for r in retention['runs']],
        ordinaryActivation={k:v for k,v in ordinary.items() if k!='samples'},
        installedModelsUnchanged=before, runtimeCatalog=catalog,
        reusedExactness=dict(reason='Identical official server/ggml, patched library, source patch, weights and catalog; raw 768 values and 256-F32 outputs reused, not timing ratios',
            report=str(previous_path),reportSha256=sha(previous_path),records=previous['exactness'],evidence=reused),
        acceptance=dict(rustPassed=450,rustIgnored=8,typescriptPassed=747,typescriptSkipped=9,typecheck=True,packageChecks=5,releasePolicyChecks=16,actualPackagedWorker=True,
            unbundledDevelopmentRealSourceStagedZipAndPackagedMissingResource=True,
            unchangedProductionFromParent='Only one cfg(test) resource-seam regression; no production behavior changed'),
        ownershipAudit=dict(identitiesChecked=len(identities),survivors=survivors),
        limitations=['Uniform 5x runtime/search and one-fifth RAM not achieved', 'Synthetic macOS arm64 only; no personal corpus or other-platform execution',
            'Small process cohorts and uncontrolled OS/shader/desktop activity; p95 descriptive only',
            'Opening is coarse first-input readiness, not cold install; two frames are render opportunities, not pixel paint',
            'Historical missing /tmp artifacts not used; c81288e is not original beta.17',
            'No multiplying cross-round ratios or fabricated aggregate percentage; individual metrics retained'],
        releasePreparation='Local ad-hoc only. Separate authorized Developer ID package/signing/catalog review and multi-platform release validation still required; no publication authorized')
    result['evidence'] = {str(f.relative_to(out)): dict(sha256=sha(f),bytes=f.stat().st_size)
        for f in out.rglob('*') if f.is_file() and 'original-source' not in f.parts and not any(s.endswith('.app') for s in f.parts)
        and f.suffix not in ['.tar'] and f.name != 'final-summary.log' and f != a.output.resolve()}
    a.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    for k, v in indexing.items(): print(k, v)
    for k, v in memory.items(): print(k, 'footprint MiB', v['footprint_median_bytes']['original']['median']/MIB, v['footprint_median_bytes']['final']['median']/MIB)
    print('Saved', a.output, 'owned identities', len(identities))


if __name__ == '__main__':
    main()
