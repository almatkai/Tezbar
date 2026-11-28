#!/usr/bin/env python3
"""Freeze measured package/integration evidence; fail on vector or ownership drift."""
import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
import statistics
import sys

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(Path(__file__).parent))
from desktop import processes


def sha(path):
    h=hashlib.sha256()
    with path.open('rb') as f:
        for chunk in iter(lambda:f.read(1024*1024),b''):h.update(chunk)
    return h.hexdigest()


def main():
    p=argparse.ArgumentParser()
    p.add_argument('--artifacts',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True)
    a=p.parse_args();base=a.artifacts.resolve()
    load=lambda name:json.loads((base/name).read_text())
    history=json.loads((ROOT/'docs/performance/beta17-memory-results.json').read_text())
    result=dict(base='c81288eb2e3c59e8284402dfef13449b5603481a',artifacts=str(base),
        scope='local macOS arm64, same models; no signing/publication; whole-app 5x / one-fifth not met',
        accounting='summed physical-footprint ledgers, not deduplicated RAM; summed RSS can double-count shared pages',
        catalog=json.loads((ROOT/'src-tauri/runtime/embedding-macos-arm64.json').read_text()))
    result['first_use']=load('first-use-1/result.json')
    # Preserve timings/pressure and treatment sequence in Git, with full payloads
    # retained separately in the immutable artifact files.
    for r in result['first_use']['runs']:
        r['phase_capture']=str(Path(r['path'])/'result.json')
    result['exactness']=[]
    for variant in ['q4','q8']:
        for threads in [2,4,8]:
            b=load(f'exact-{variant}-{threads}-stock/vectors.json')
            c=load(f'exact-{variant}-{threads}-patched/vectors.json')
            assert b==c
            result['exactness'].append(dict(variant=variant,threads=threads,raw_768_values_equal=True,
                workloads=load(f'exact-{variant}-{threads}-patched/result.json')['workloads']))
        new=load(f'bounds-{variant}-patched/result.json')
        old=history['prototype_full'][0 if variant=='q4' else 1]
        assert old['model_sha256']==new['model_sha256']
        old={w['name']:w for w in old['workloads']}
        for w in new['workloads']:
            if 'raw_sha256' in w:
                assert w['raw_sha256']==old[w['name']]['raw_sha256']
                assert w['normalized_f32_sha256']==old[w['name']]['normalized_f32_sha256']
            else:assert w['name']=='sequence8193' and w['error']['status']==old[w['name']]['error']['status']
        result['exactness'].append(dict(variant=variant,boundaries_match_preserved_stock_hashes=True,workloads=new['workloads']))
    result['desktop']=[]
    groups={'stock':[],'patched':[]}
    for name,variant in [('thin-visible-stock-1','stock'),('thin-visible-patched-1','patched'),
        ('thin-visible-patched-2','patched'),('thin-visible-stock-2','stock'),
        ('thin-visible-long-stock','long-stock'),('thin-visible-long-patched','long-patched')]:
        r=load(name+'/result.json');assert r['completed'] and not load(name+'/cleanup.json')['owned_survivors']
        db=sqlite3.connect('file:'+str(Path(r['root'])/'data/search/index.sqlite3')+'?mode=ro',uri=True)
        files=db.execute("SELECT count(*) FROM documents WHERE category='files'").fetchone()[0];db.close();assert files==5000
        expected='5ac291cdacc7493c03dab643d5ab1567d31f9cbc8a694b139d2a91aedf7c9ab2' if r['long_chunks'] else '27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c'
        assert r['metrics']['vectors_sha256']==expected
        row=dict(name=name,variant=variant,binary_sha256=r['binary_sha256'],launcher_files=files,
            first_ui_ms=r['first_ui_ms'],vectors_sha256=expected,runtime_status=r['runtime_status'],phases={})
        for key,value in r['metrics'].items():
            if isinstance(value,dict) and 'cpu_s' in value:
                row['phases'][key]={k:v for k,v in value.items() if k!='value'}
                payload=value.get('value')
                if isinstance(payload,list) and payload and 'inputToTwoFramesMs' in payload[0]:
                    row['phases'][key]['input_to_two_frames_p50_ms']=statistics.median(x['inputToTwoFramesMs'] for x in payload)
                if isinstance(payload,dict) and 'actual_complete_s' in payload:
                    row['phases'][key]['actual_complete_s']=payload['actual_complete_s']
                    row['phases'][key]['foreground_input_p50_ms']=statistics.median(x['inputToTwoFramesMs'] for x in payload['bursts'])
        result['desktop'].append(row)
        if variant in groups:groups[variant].append(row)
    result['desktop_medians']={}
    for variant,rows in groups.items():
        phases={}
        for phase in rows[0]['phases']:
            phases[phase]={k:statistics.median(r['phases'][phase][k] for r in rows)
                for k,v in rows[0]['phases'][phase].items() if isinstance(v,(int,float))}
        result['desktop_medians'][variant]=phases
    for key,name in [('ui','ui-parity-opening-final-4.json'),('retention','ui-retention-final-4.json'),
        ('ordinary','ordinary-upgrade-final-4/result.json')]:
        r=load(name);assert r['completed']
        if key=='retention':
            for run in r['runs']:run.pop('samples',None)
        if key=='ordinary':r.pop('samples',None)
        if key=='ui':
            for run in r['parity']:
                for value in run['values']:
                    raw=json.dumps(value['rows'],sort_keys=True,separators=(',',':')).encode()
                    value['full_rows_sha256']=hashlib.sha256(raw).hexdigest()
                    value['rows']=[{k:v for k,v in row.items() if k not in ['icon','iconUrl','thumbnail']} for row in value['rows']]
        result[key]=r
    result['native_signatures']=load('native-search-final-4.json')
    assert result['native_signatures']['exactOrderedIdScoreParity']
    result['binaries']={p.name:dict(path=str(p),sha256=sha(p),bytes=p.stat().st_size) for p in sorted((base/'binaries').iterdir()) if p.is_file()}
    result['validation']=dict(typescript_passed=747,typescript_skipped=9,rust_passed=448,rust_ignored=8,
        package_pin_tests=5,release_policy_tests=16,typecheck=True,frontend=True,
        actual_ordinary_and_benchmark_app_packages=True,actual_packaged_bun_worker=True,
        windows_macos_intel_linux_execution='not executed; existing runtime defaults retained',
        developer_id_signing_notarization='not authorized/executed; signed release wrapper rejects local-only catalog')
    result['installed_models']={}
    modelroot=Path.home()/'Library/Application Support/com.tezbar.app/knowledge/models'
    official=next((ROOT/'build/p/runtime-rebuild-3/official').rglob('llama-server')).parent
    for directory,expected in history['delivery_verification']['installed_models_unchanged'].items():
        model=next((modelroot/directory).glob('*.gguf'));actual=sha(model);assert actual==expected
        runtime=next((modelroot/directory/'runtime').rglob('llama-server')).parent
        for file in official.iterdir():
            if file.is_file():assert sha(runtime/file.name)==sha(file),file.name
        result['installed_models'][directory]=dict(model_sha256=actual,all_runtime_files_match_pinned_official_archive=True)
    # Audit every recorded PID/start identity without signaling anything.
    identities=set();failures=[]
    for f in base.rglob('result.json'):
        data=json.loads(f.read_text())
        if data.get('completed') is False:failures.append(dict(path=str(f),error=data.get('error')))
        def walk(value):
            if isinstance(value,dict):
                if all(k in value for k in ['pid','start','footprint']):identities.add((value['pid'],value['start']))
                for child in value.values():walk(child)
            elif isinstance(value,list):
                for child in value:walk(child)
        walk(data)
    survivors=[]
    for pid,start in identities:
        survivors.extend(p for p in processes(pid) if p['pid']==pid and p['start']==start)
    assert not survivors
    result['ownership_audit']=dict(identities_checked=len(identities),survivors=survivors)
    result['exploratory_and_failed']=dict(failed_results=failures,
        excluded_old_tmp='/tmp/tezbar-beta17-perf.e1qzXU became absent; cause unestablished; original Git reports untouched; none of its missing raw timings are used here',
        excluded_build_profiles='early build/p desktop profiles had launcher-excluded build component; retained, not primary data',
        excluded_locked_display='locked session caused failed startup/frame waits; preflight now rejects visible timing while locked',
        superseded_embedded_executable='24 MB executable ZIP + eager install cost investigated; final thin executable uses pinned app resource and lazy materialization')
    source=['.gitignore','package.json','src-tauri/Cargo.toml','src-tauri/build.rs','src-tauri/tauri.conf.json',
        'src-tauri/runtime/embedding-macos-arm64.json','src-tauri/src/native_core/knowledge/runtime.rs',
        'src-tauri/src/native_core/knowledge/runtime_bundle.rs','src-tauri/src/native_core/knowledge/runtime_integrity.rs',
        'src-tauri/src/native_core/knowledge/runtime_models.rs','src-tauri/src/child_supervisor.rs',
        'scripts/build-backend.ts','scripts/build-release.mjs','scripts/build-embedding-runtime.py',
        'scripts/prepare-embedding-runtime.py','scripts/stage-embedding-runtime.py','scripts/test-embedding-runtime.py',
        'scripts/lib/embedding-runtime-policy.mjs','scripts/embedding-runtime-policy.test.mjs',
        'docs/performance/llama-b11474-embedding-output.patch','docs/performance/build-llama-memory.py',
        'docs/performance/summarize-runtime-package.py','licenses/llama.cpp-MIT.txt']
    result['source_sha256']={name:sha(ROOT/name) for name in source}
    result['evidence']={str(p.relative_to(base)):dict(sha256=sha(p),bytes=p.stat().st_size) for p in base.rglob('*')
        if p.is_file() and '.app' not in str(p) and p.relative_to(base).parts[0]!='profiles'
        and p.suffix in ['.json','.log','.txt']}
    result['package_sha256']=sha(ROOT/'build/embedding-runtime/resources/embedding-runtime.zip')
    assert result['package_sha256']==result['catalog']['archiveSha256']
    a.output.write_text(json.dumps(result,indent=2)+'\n')
    print('Saved',a.output,'audited',len(identities),'owned identities; zero survivors')


if __name__=='__main__':main()
