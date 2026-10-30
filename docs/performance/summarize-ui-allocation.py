#!/usr/bin/env python3
"""Freeze R9 causal experiments, rejected candidates and unchanged production.
Never rewrites historical evidence or infers a win from render counters alone.
"""
import argparse
import ctypes
import hashlib
import json
import math
from pathlib import Path
import shutil
import sqlite3
import statistics
from desktop import LIB, Usage


def sha(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def median(values): return statistics.median(values)
def percentile(values,p): return sorted(values)[max(0,math.ceil(len(values)*p)-1)]
def latency(values):
    return {k:{q:percentile([v[k] for v in values],p) for q,p in [('p50',.5),('p95',.95)]}
            for k in ['responseMs','inputToTwoFramesMs','ipcMs']}
def last_roles(run,phase=None):
    return [s for s in run['samples'] if phase is None or s['phase']==phase][-1]['processes']
def role_sum(roles,field='footprint',role=None):
    return sum(p[field] for p in roles if role is None or p['role']==role)
def phase_cpu(run,phase):
    samples=[s for s in run['samples'] if s['phase']==phase]
    first={(p['pid'],p['start']):p for p in samples[0]['processes']};last={};result={}
    for sample in samples:
        for proc in sample['processes']: last[(proc['pid'],proc['start'])]=proc
    for key,proc in last.items():
        role=proc['role'];result[role]=result.get(role,0)+proc['cpu_s']-first.get(key,{}).get('cpu_s',0)
    return result


def main():
    p=argparse.ArgumentParser();p.add_argument('--artifacts',type=Path,required=True);p.add_argument('--output',type=Path,required=True)
    a=p.parse_args();repo=Path(__file__).resolve().parents[2];root=a.artifacts.resolve(strict=True)
    if a.output.exists(): raise ValueError('refusing to overwrite evidence')
    raw={v.parent.name:json.loads(v.read_text()) for v in root.glob('*/result.json')}
    assert all(v['completed'] and not v.get('survivors') for v in raw.values())
    short={}
    for prototype,prefix in [('content',''),('icons','icons-')]:
        short[prototype]={}
        for variant in ['original','current','candidate']:
            values=[]
            for n in [1,2]:
                name=f'{prefix}{variant}-short-{n}';r=raw[name];m=r['metrics'];roles=last_roles(r)
                values.append(dict(name=name,cpu=m['simple_bursts']['cpu_s'],perRoleCpu=phase_cpu(r,'simple_bursts'),
                    latency=latency(m['simple_bursts']['value']),dispatches=[v['dispatched'] for v in m['simple_bursts']['value']],
                    before=m['before'],after=m['after'],lastFamilyFootprint=role_sum(roles),lastFamilyRss=role_sum(roles,'rss'),lastWebFootprint=role_sum(roles,role='com.apple.WebKit.WebContent')))
            short[prototype][variant]=values
    full={};databases={}
    for name in ['full-original-1','full-candidate-1','full-candidate-2','full-original-2','confirm-full-current','confirm-full-candidate']:
        r=raw[name];m=r['metrics'];timings={}
        for phase in ['simple_bursts','deep_lexical_bursts','deep_model_bursts','repeat_deep_model_bursts','source_index_with_bursts','semantic_index_with_bursts']:
            values=m[phase]['value'];values=values['bursts'] if isinstance(values,dict) else values
            timings[phase]=dict(cpu=m[phase]['cpu_s'],perRoleCpu=phase_cpu(r,phase),latency=latency(values),dispatches=[v['dispatched'] for v in values])
        full[name]=dict(firstUiMs=r['first_ui_ms'],viewport=r['viewport'],timings=timings,
            idle={phase:m[phase] for phase in ['hidden_idle','post_search_idle','active_model_idle','post_repeat_idle']},
            lastRoles={phase:last_roles(r,phase) for phase in ['hidden_idle','post_search_idle','active_model_idle','post_repeat_idle']},
            completion=m['semantic_index_with_bursts']['value'],openingCycles=m['search_open_close'])
        source=Path(r['root'])/'data/knowledge/knowledge.sqlite3';wal=Path(str(source)+'-wal')
        assert not wal.exists() or wal.stat().st_size==0
        dest=root/'closed-fixtures'/name/'knowledge.sqlite3';dest.parent.mkdir(parents=True,exist_ok=True)
        if not dest.exists(): shutil.copyfile(source,dest)
        assert sha(source)==sha(dest)
        with sqlite3.connect('file:'+str(dest)+'?mode=ro&immutable=1',uri=True) as db:
            counts={t:db.execute('SELECT count(*) FROM '+t).fetchone()[0] for t in ['knowledge_sources','knowledge_chunks','knowledge_gemma_embeddings']}
            identity=db.execute('SELECT model_key,count(*),sum(length(vector)) FROM knowledge_gemma_embeddings GROUP BY model_key').fetchall();digest=hashlib.sha256()
            for path,offset,vector in db.execute('SELECT s.path,c.start_offset,g.vector FROM knowledge_gemma_embeddings g JOIN knowledge_chunks c ON c.id=g.chunk_id JOIN knowledge_sources s ON s.id=c.source_id ORDER BY s.path,c.start_offset'):
                digest.update(Path(path).name.encode());digest.update(str(offset).encode());digest.update(vector)
            assert counts==dict(knowledge_sources=96,knowledge_chunks=192,knowledge_gemma_embeddings=192)
            assert digest.hexdigest()==m['vectors_sha256']=='27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c'
        databases[name]=dict(path=str(dest),sha256=sha(dest),counts=counts,identity=identity,vectorsSha256=digest.hexdigest())
    assert len({json.dumps(v['identity']) for v in databases.values()})==1
    parity=json.loads((root/'parity-opening.json').read_text());retention=json.loads((root/'retention90.json').read_text())
    assert parity['completed'] and parity['exact_final_query_parity'] and parity['candidate_ime_hide_recovery']
    assert retention['completed'] and retention['exact_ordered_parity'] and all(v['same_model_process'] for v in retention['runs'])
    identities={}
    def collect(value):
        if isinstance(value,dict):
            if all(k in value for k in ['pid','start','role']): identities[(value['pid'],value['start'])]={k:value[k] for k in ['pid','start','role']}
            else:
                for child in value.values(): collect(child)
        elif isinstance(value,list):
            for child in value: collect(child)
    collect(raw);collect(parity);collect(retention)
    survivors=[]
    for (pid,start),value in identities.items():
        usage=Usage()
        if LIB.proc_pid_rusage(pid,4,ctypes.byref(usage))==0 and usage.proc_start_abstime==start: survivors.append(value)
    assert not survivors,survivors
    frozen=repo/'build/performance-runtime-20261030/cumulative-20261030'
    old=json.loads((frozen/'accepted-source-sha256.json').read_text());changed={n:sha(repo/n) for n,v in old.items() if sha(repo/n)!=v}
    assert set(changed)=={'src/renderer/CommandBar.lifecycle.test.ts'},changed
    assert sha(repo/'src/renderer/CommandBar.tsx')==old['src/renderer/CommandBar.tsx']
    binaries={};apps={}
    paths={'original-bench':frozen/'original-bench.app','current-bench':frozen/'accepted-bench.app','current-ordinary':frozen/'accepted-ordinary.app',
           'rejected-content-bench':root/'candidate-bench.app','rejected-content-ordinary':root/'candidate-ordinary.app',
           'rejected-icons-bench':root/'icons-bench.app','rejected-icons-ordinary':root/'icons-ordinary.app',
           'diagnostic-bench':root/'diagnostic-bench.app','diagnostic-memo-bench':root/'diagnostic-memo-bench.app','diagnostic-content-bench':root/'diagnostic-content-bench.app'}
    for label,path in paths.items():
        exe=path/'Contents/MacOS/tezbar';binaries[label]=dict(path=str(exe),sha256=sha(exe))
        apps[label]={str(v.relative_to(path)):sha(v) for v in sorted(path.rglob('*')) if v.is_file()}
        data=exe.read_bytes();assert all((marker in data)==label.endswith('bench') for marker in [b'TEZBAR_BENCH_HOME',b'__desktop_bench_reply'])
    assert binaries['original-bench']['sha256']=='5a219f1bcff77ffb3dc70100ef21258795ccd7a7867b9e8333ba9f0b26139f74'
    assert binaries['current-bench']['sha256']=='1cbb16091ef42dc0fe72d009d0d84046663ead874e27b3cbb30ec6fc5923ffac'
    for label in ['candidate','icons']:
        for kind in ['bench','ordinary']:
            assert apps['rejected-'+('content' if label=='candidate' else 'icons')+'-'+kind]==json.loads((root/f'{label}-{kind}-receipt.json').read_text())
    assert raw['ordinary-activation']['old_manifests_unchanged']
    for name,needles in {'final-restored-typescript.log':['749 passed','9 skipped'],'final-harness-tests.log':['Ran 8 tests','OK'],'icons-regression-red.log':['1 failed'],'icons-regression-green.log':['2 passed']}.items():
        assert all(s in (root/name).read_text() for s in needles),name
    installed=json.loads((frozen/'installed-models-before.json').read_text())
    for model in installed.values():
        weights=Path(model['weights']);assert sha(weights)==model['weightsSha256']
        for name,value in model['files'].items(): assert sha(weights.parent/name)==value
    previous=json.loads((repo/'docs/performance/beta17-runtime-package-results.json').read_text());exact={}
    assert previous['catalog']==json.loads((repo/'src-tauri/runtime/embedding-macos-arm64.json').read_text())
    for name,record in previous['evidence'].items():
        if name.startswith(('exact-','bounds-')):
            path=Path(previous['artifacts'])/name;assert sha(path)==record['sha256'];exact[str(path)]=record
    assert sha(repo/'docs/performance/beta17-results-report.html')=='1ee1b0742261dbfb1ad8071817778b5881cb82c084bda482684117279f45816d'
    diagnostic={n:dict(cpu=r['metrics']['simple_bursts']['cpu_s'],after=r['metrics']['after'],lastRoles=last_roles(r),bridge=r['bridge']) for n,r in raw.items() if n.startswith('diag-')}
    decisions={}
    for prototype,values in short.items():
        decisions[prototype]=dict(cpuRatio=median([v['cpu'] for v in values['candidate']])/median([v['cpu'] for v in values['current']]),
            webRatio=median([v['lastWebFootprint'] for v in values['candidate']])/median([v['lastWebFootprint'] for v in values['current']]),accepted=False)
    out=dict(schemaVersion=1,startHead='56e94837d20c968a84a5c7f195e42c58d01fb24c',original='9ff3663342b48ab01be2e56ab30f6d7123b0ced8',
        productionChanges=False,acceptedCandidate=None,goalAchieved=False,regressionFixed=False,
        decisions=decisions,short=short,rejectedContentFull=full,diagnosticOnly=diagnostic,databases=databases,
        rejectedContentBehavior=dict(parity=str(root/'parity-opening.json'),retention=str(root/'retention90.json'),ordinary=str(root/'ordinary-activation/result.json'),
            exactOrderedParity=True,imeHideRecovery=True,retentionSameModelPid=True),
        receipts=dict(changedBuildInputs=changed,sourceSha256={n:sha(repo/n) for n in old},binaries=binaries,appFilesSha256=apps,
            prototypeBuildInputs={label:json.loads((root/f'{label}-source-sha256.json').read_text()) for label in ['diagnostic','diagnostic-memo','diagnostic-content','candidate','icons']},
            nativeReuse=json.loads((root/'reused-native.json').read_text()),installedModelsVerified=installed,reusedModelBoundaryEvidence=exact,
            reusedNative40Signatures=dict(path=str(frozen/'native-comparison.json'),sha256=sha(frozen/'native-comparison.json')),
            harnessSourceSha256={str(v.relative_to(repo)):sha(v) for v in sorted((repo/'docs/performance').glob('*.py'))},
            artifactsSha256={str(v.relative_to(root)):sha(v) for v in sorted(root.rglob('*')) if v.is_file() and not any(x.endswith('.app') for x in v.relative_to(root).parts)},
            ownedIdentities=list(identities.values()),survivors=survivors,unrelatedHtmlSha256=sha(repo/'docs/performance/beta17-results-report.html')))
    a.output.write_text(json.dumps(out,indent=2)+'\n');print(json.dumps(dict(decisions=decisions,ownedIdentities=len(identities),survivors=survivors,reusedExactArtifacts=len(exact))))

if __name__=='__main__': main()
