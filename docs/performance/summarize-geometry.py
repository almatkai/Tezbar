#!/usr/bin/env python3
"""Freeze R12 rejected-boundary evidence; verify rollback, reuse and ownership."""
import argparse
import ctypes
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import sqlite3
import statistics
import subprocess
from desktop import LIB, Usage, TIMEBASE

spec = importlib.util.spec_from_file_location('ui_summary', Path(__file__).with_name('summarize-ui-allocation.py'))
u = importlib.util.module_from_spec(spec)
spec.loader.exec_module(u)


def sha(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def write_new(path, value):
    with path.open('x') as stream:
        stream.write(json.dumps(value, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--artifacts', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[2]
    root = args.artifacts.resolve(strict=True)
    old_root = repo / 'build/performance-row-layout-20261030'
    old_path = old_root / 'beta17-row-layout-results-final-manifest.json'
    old = json.loads(old_path.read_text())
    assert subprocess.check_output(['git', 'branch', '--show-current'], text=True).strip() == 'fix/beta17-runtime-search-memory'
    assert subprocess.check_output(['git', 'rev-parse', '--show-toplevel'], text=True).strip() == str(repo)
    source = {name: sha(repo / name) for name in old['sourceSha256']}
    changed = {name: value for name, value in source.items() if value != old['sourceSha256'][name]}
    assert set(changed) == {'src/renderer/CommandBar.lifecycle.test.ts'}, changed
    # Reuse is explicit and byte-verified, not a fresh native/model test claim.
    for name, value in old['harnessSha256'].items():
        assert sha(repo / name) == value, name
    for name, value in old['rawFiles'].items():
        assert sha(old_root / name) == value, name
    for name, value in old['reused'].items():
        if isinstance(value, str):
            assert sha(repo / name) == value, name
    for name, value in old['reusedExactness'].items():
        assert sha(name) == value, name
    for model in old['installedModels'].values():
        weights = Path(model['weights'])
        assert sha(weights) == model['weightsSha256']
        for name, value in model['files'].items():
            assert sha(weights.parent / name) == value, name
    runtime = repo / 'build/embedding-runtime/resources/embedding-runtime.zip'
    assert sha(runtime) == old['zipSha256']
    r7 = json.loads((repo / 'docs/performance/beta17-final-results.json').read_text())
    for app, inventory in r7['appFiles'].items():
        path = repo / 'build/performance-runtime-20261030/cumulative-20261030' / app
        assert {str(p.relative_to(path)): sha(p) for p in path.rglob('*') if p.is_file()} == {n: v['sha256'] for n, v in inventory.items()}, app
    for app, inventory in old['rejectedAppFiles'].items():
        path = old_root / app
        assert {str(p.relative_to(path)): sha(p) for p in path.rglob('*') if p.is_file()} == inventory, app
    html = repo / 'docs/performance/beta17-results-report.html'
    assert sha(html) == '1ee1b0742261dbfb1ad8071817778b5881cb82c084bda482684117279f45816d'
    raw = {p.parent.name: json.loads(p.read_text()) for p in root.glob('*/result.json')}
    assert len(raw) == 15 and all(r['completed'] and not r['survivors'] for r in raw.values())
    phases = ['simple_bursts', 'deep_lexical_bursts', 'deep_model_bursts', 'repeat_deep_model_bursts', 'source_index_with_bursts', 'semantic_index_with_bursts']
    quiet = ['hidden_idle', 'post_search_idle', 'active_model_idle', 'post_repeat_idle']
    full, short, trajectory, databases = {}, {}, {}, {}
    for name, run in raw.items():
        if name.startswith('trajectory-'):
            blocks = []
            for i, block in enumerate(run['metrics']['simple_bursts']['blocks']):
                values = block['work']['value']
                assert all(not v['trustedInputEvents'] and not v['errors'] and v['completed'] == v['dispatched'] for v in values)
                ownership = block['quiet']['geometryOwnership']
                assert ownership['queue']['pending'] == 0 and not ownership['bridgeActive'] and not ownership['ownsFinalReply']
                blocks.append(dict(familyCpu=block['work']['cpu_s'], perRoleCpu=u.phase_cpu(run, 'trajectory_work_' + str(i)),
                                   familyMiB=block['quiet']['footprint_median_bytes'] / 2**20,
                                   webMiB=u.role_sum(u.last_roles(run, 'trajectory_quiet_' + str(i)), role='com.apple.WebKit.WebContent') / 2**20,
                                   ownership=ownership))
            trajectory[name] = dict(diagnosticOnly=True, blocks=blocks, final30s=dict(
                familyMiB=run['metrics']['after']['footprint_median_bytes'] / 2**20,
                webMiB=u.role_sum(u.last_roles(run), role='com.apple.WebKit.WebContent') / 2**20,
                ownership=run['metrics']['after']['geometryOwnership']))
            continue
        if not name.startswith(('short-', 'full-')):
            continue
        timings = {}
        for phase in phases:
            if phase not in run['metrics']:
                continue
            metric = run['metrics'][phase]
            values = metric['value']
            values = values['bursts'] if isinstance(values, dict) else values
            assert all(not v['trustedInputEvents'] and not v['errors'] and v['completed'] == v['dispatched'] for v in values)
            cpu = u.phase_cpu(run, phase)
            assert abs(sum(cpu.values()) - metric['cpu_s']) < 1e-8
            timings[phase] = dict(familyCpu=metric['cpu_s'], perRoleCpu=cpu,
                                  latency=u.latency(values), dispatches=[v['dispatched'] for v in values])
        if name.startswith('short-'):
            short[name] = dict(**timings['simple_bursts'], familyQuietMedianMiB=run['metrics']['after']['footprint_median_bytes'] / 2**20,
                               webFinalMiB=u.role_sum(u.last_roles(run), role='com.apple.WebKit.WebContent') / 2**20)
            continue
        full[name] = dict(firstUiMs=run['first_ui_ms'], viewport=run['viewport'], timings=timings,
                          quiet={q: dict(familyMedianMiB=run['metrics'][q]['footprint_median_bytes'] / 2**20,
                                         webFinalMiB=u.role_sum(u.last_roles(run, q), role='com.apple.WebKit.WebContent') / 2**20,
                                         cpu=run['metrics'][q]['cpu_s'], summedFinalRssMiB=run['metrics'][q]['rss_last_bytes'] / 2**20) for q in quiet},
                          completion={k: v for k, v in run['metrics']['semantic_index_with_bursts']['value'].items() if k != 'bursts'})
        source_db = Path(run['root']) / 'data/knowledge/knowledge.sqlite3'
        wal = Path(str(source_db) + '-wal')
        assert not wal.exists() or wal.stat().st_size == 0
        dest = root / 'closed-fixtures' / name / 'knowledge.sqlite3'
        dest.parent.mkdir(parents=True, exist_ok=True)
        if not dest.exists():
            shutil.copyfile(source_db, dest)
        assert sha(source_db) == sha(dest)
        with sqlite3.connect('file:' + str(dest) + '?mode=ro&immutable=1', uri=True) as db:
            counts = {table: db.execute('SELECT count(*) FROM ' + table).fetchone()[0] for table in ['knowledge_sources', 'knowledge_chunks', 'knowledge_gemma_embeddings']}
            identity = db.execute('SELECT model_key,count(*),sum(length(vector)) FROM knowledge_gemma_embeddings GROUP BY model_key').fetchall()
            digest = hashlib.sha256()
            for path, offset, vector in db.execute('SELECT s.path,c.start_offset,g.vector FROM knowledge_gemma_embeddings g JOIN knowledge_chunks c ON c.id=g.chunk_id JOIN knowledge_sources s ON s.id=c.source_id ORDER BY s.path,c.start_offset'):
                assert len(vector) == 256 * 4
                digest.update(Path(path).name.encode())
                digest.update(str(offset).encode())
                digest.update(vector)
        assert counts == dict(knowledge_sources=96, knowledge_chunks=192, knowledge_gemma_embeddings=192)
        assert digest.hexdigest() == run['metrics']['vectors_sha256'] == '27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c'
        databases[name] = dict(path=str(dest.relative_to(repo)), sha256=sha(dest), counts=counts, identity=identity, vectorsSha256=digest.hexdigest())
    assert len({json.dumps(v['identity']) for v in databases.values()}) == 1
    identities = {}
    def collect(value):
        if isinstance(value, dict):
            if all(k in value for k in ['pid', 'start', 'role']):
                identities[(value['pid'], value['start'])] = {k: value[k] for k in ['pid', 'start', 'role']}
            else:
                for child in value.values():
                    collect(child)
        elif isinstance(value, list):
            for child in value:
                collect(child)
    collect(raw)
    for (pid, start), value in identities.items():
        usage = Usage()
        assert not (LIB.proc_pid_rusage(pid, 4, ctypes.byref(usage)) == 0 and usage.proc_start_abstime == start), value
    app = root / 'hybrid-bench.app'
    inventory = {str(p.relative_to(app)): sha(p) for p in app.rglob('*') if p.is_file()}
    assert inventory == json.loads((root / 'hybrid-bench-receipt.json').read_text())
    for name, value in json.loads((root / 'restored-source.json').read_text()).items():
        assert sha(repo / name) == value
    assert all(g['exit'] == 0 for g in json.loads((root / 'restored-gates.json').read_text()))
    assert all(s in (root / 'restored-typescript.log').read_text() for s in ['752 passed', '9 skipped'])
    med = statistics.median
    def ratios(group):
        pre = [v for n, v in group.items() if n.endswith('prefix')]
        candidate = [v for n, v in group.items() if n.endswith('hybrid')]
        return dict(familySimpleCpu=med(v['familyCpu'] for v in candidate) / med(v['familyCpu'] for v in pre),
                    webSimpleCpu=med(v['perRoleCpu']['com.apple.WebKit.WebContent'] for v in candidate) / med(v['perRoleCpu']['com.apple.WebKit.WebContent'] for v in pre))
    comparison = dict(short=ratios(short), full=ratios({n: v['timings']['simple_bursts'] for n, v in full.items()}))
    detail_path = root / 'geometry-details.json'
    write_new(detail_path, dict(short=short, full=full, trajectory=trajectory,
                               frames=raw['hybrid-frames']['steps'], transitions=raw['hybrid-transitions-qualified']['steps']))
    manifest_path = root / 'geometry-manifest.json'
    manifest = dict(sourceSha256=source, changedInputs=changed, reusedR11Manifest=dict(path=str(old_path.relative_to(repo)), sha256=sha(old_path)),
                    reused=old['reused'], reusedExactness=old['reusedExactness'], installedModels=old['installedModels'], zipSha256=sha(runtime),
                    rejectedAppFiles={'hybrid-bench.app': inventory}, ownedIdentities=list(identities.values()), survivors=[], databases=databases,
                    harnessSha256={str(p.relative_to(repo)): sha(p) for p in (repo / 'docs/performance').glob('*.py')},
                    rawFiles={str(p.relative_to(root)): sha(p) for p in root.rglob('*') if p.is_file() and not any(part.endswith('.app') for part in p.relative_to(root).parts)})
    write_new(manifest_path, manifest)
    out = dict(schemaVersion=1, startHead='3fea98fe4dd272844313b9a1a8cac24d9044cddc', productionChanges=False, acceptedCandidate=None, goalAchieved=False,
               decision=dict(boundary='Immediate selection/identity/count/reorder/visibility; stable-target microtask plus post-ancestor-layout recheck',
                             sameFrameQualifiedInTestedCases=True, accepted=False,
                             reason='Correct narrowed boundary does not preserve a robust total CPU benefit; full CPU ranges overlap, source-index input CPU rises, and joint retained RAM remains unqualified',
                             reachableLeakProven=False), comparison=comparison, short=short, full=full, trajectory=trajectory,
               accountingTimebase=dict(numer=TIMEBASE.numer, denom=TIMEBASE.denom),
               receipts=dict(artifacts=str(root.relative_to(repo)), manifestPath=str(manifest_path.relative_to(repo)), manifestSha256=sha(manifest_path),
                             detailsPath=str(detail_path.relative_to(repo)), detailsSha256=sha(detail_path), productionInputCount=len(source), changedInputs=changed,
                             reusedRustPassed=450, reusedRustIgnored=8, reusedNativeSignatures=40, reusedExactArtifacts=len(old['reusedExactness']),
                             ownedIdentities=len(identities), survivors=[], durableIndexes=len(databases)))
    write_new(args.output, out)
    print(json.dumps(dict(comparison=comparison, receipts=out['receipts'])))


if __name__ == '__main__':
    main()
