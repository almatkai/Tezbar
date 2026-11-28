#!/usr/bin/env python3
"""Freeze R8 focused/lean results and audit observed PID/start identities.
No new timing claims are generated from historical incremental ratios.
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


def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def median(values):return statistics.median(values)
def percentile(values,p):return sorted(values)[max(0,math.ceil(len(values)*p)-1)]

def latency(values):
    return {key:dict(p50=percentile([v[key] for v in values],.5),p95=percentile([v[key] for v in values],.95)) for key in ['responseMs','inputToTwoFramesMs','ipcMs']}


def main():
    p=argparse.ArgumentParser()
    p.add_argument('--artifacts',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True)
    p.add_argument('--assert-no-regression',action='store_true')
    a=p.parse_args();repo=Path(__file__).resolve().parents[2]
    if a.output.exists():raise ValueError('refusing to overwrite evidence')
    root=a.artifacts.resolve(strict=True);frozen=repo/'build/performance-runtime-20261030/cumulative-20261030'
    raw={v.parent.name:json.loads(v.read_text()) for v in root.glob('*/result.json')}
    assert all(v['completed'] and not v.get('survivors') for v in raw.values())
    source=json.loads((frozen/'accepted-source-sha256.json').read_text())
    assert all(sha(repo/path)==digest for path,digest in source.items()),'accepted source inputs changed'
    binaries={}
    for name,expected in {
        'original-bench':'5a219f1bcff77ffb3dc70100ef21258795ccd7a7867b9e8333ba9f0b26139f74',
        'original-ordinary':'230a98986d9657b688fd44d9f6e78c641ceac6b91feba0f29699af47fee4e200',
        'original-native-tests':'5cc9ce69dcb765b5646f8c33906c299fe34f95ad6e594e83aa9b1c59946b64a8',
        'accepted-bench':'1cbb16091ef42dc0fe72d009d0d84046663ead874e27b3cbb30ec6fc5923ffac',
        'accepted-ordinary':'d8c789fb6266b8a5544ac89add5017a0900d64cd5433ace907b982722ef09727',
        'accepted-native-tests':'1062aa8299fd0127a8a540a7b09a7a4d6b87b5b008e425fac964b6229eedc7e4',
    }.items():
        assert sha(frozen/name)==expected,name
        binaries[name]=dict(path=str(frozen/name),sha256=expected)
        if 'tests' not in name:
            assert sha(frozen/(name+'.app/Contents/MacOS/tezbar'))==expected
            data=(frozen/name).read_bytes()
            markers={marker.decode():marker in data for marker in [b'TEZBAR_BENCH_HOME',b'__desktop_bench_reply']}
            assert all(value==name.endswith('bench') for value in markers.values())
            binaries[name]['markers']=markers
    pin=json.loads((repo/'src-tauri/runtime/embedding-macos-arm64.json').read_text())
    for name in ['accepted-ordinary.app','accepted-bench.app']:
        assert sha(frozen/name/'Contents/Resources/embedding-runtime/embedding-runtime.zip')==pin['archiveSha256']
    focused={}
    for protocol,names in {'old8':['old-1','old-2'],'lean8':['lean-1','lean-2'],'lean30':['lean30-3','lean30-4']}.items():
        focused[protocol]={}
        for variant in ['original','accepted']:
            runs=[raw[variant+'-'+n] for n in names]
            focused[protocol][variant]=[
                dict(name=variant+'-'+name,cpu_s=run['metrics']['simple_bursts']['cpu_s'],before=run['metrics']['before'],after=run['metrics']['after'],latency=latency(run['metrics']['simple_bursts']['value']),
                     dispatches=[v['dispatched'] for v in run['metrics']['simple_bursts']['value']],
                     steps=[dict(phase=s['phase'],familyFootprint=sum(p['footprint'] for p in s['processes']),familyRss=sum(p['rss'] for p in s['processes']),webContentFootprint=sum(p['footprint'] for p in s['processes'] if p['role']=='com.apple.WebKit.WebContent')) for s in run['samples'] if s['phase'].startswith('after_burst_')])
                for name,run in zip(names,runs)]
    full={}
    for variant in ['original','accepted']:
        full[variant]=[]
        for n in [1,2]:
            name=f'full-{variant}-{n}';run=raw[name];metrics=run['metrics'];timings={}
            for phase in ['simple_bursts','deep_lexical_bursts','deep_model_bursts','repeat_deep_model_bursts']:
                timings[phase]=dict(familyCpu=metrics[phase]['cpu_s'],latency=latency(metrics[phase]['value']),dispatches=[v['dispatched'] for v in metrics[phase]['value']])
            for phase in ['source_index_with_bursts','semantic_index_with_bursts']:
                timings[phase]=dict(familyCpu=metrics[phase]['cpu_s'],latency=latency(metrics[phase]['value']['bursts']))
            # Preserve closed, synthetic authoritative DB evidence after shutdown.
            original=Path(run['root'])/'data/knowledge/knowledge.sqlite3'
            dest=root/'closed-fixtures'/name/'knowledge.sqlite3';dest.parent.mkdir(parents=True,exist_ok=True)
            wal=Path(str(original)+'-wal')
            assert not wal.exists() or wal.stat().st_size==0,'uncheckpointed live fixture WAL'
            if not dest.exists():shutil.copyfile(original,dest)
            assert sha(original)==sha(dest)
            # These are checkpointed post-exit copies, not live databases.
            with sqlite3.connect('file:'+str(dest)+'?mode=ro&immutable=1',uri=True) as db:
                counts={t:db.execute('SELECT count(*) FROM '+t).fetchone()[0] for t in ['knowledge_sources','knowledge_chunks','knowledge_gemma_embeddings']}
                identity=db.execute('SELECT model_key,count(*),sum(length(vector)) FROM knowledge_gemma_embeddings GROUP BY model_key').fetchall()
                digest=hashlib.sha256()
                for path,offset,vector in db.execute('SELECT s.path,c.start_offset,g.vector FROM knowledge_gemma_embeddings g JOIN knowledge_chunks c ON c.id=g.chunk_id JOIN knowledge_sources s ON s.id=c.source_id ORDER BY s.path,c.start_offset'):
                    digest.update(Path(path).name.encode());digest.update(str(offset).encode());digest.update(vector)
                assert counts==dict(knowledge_sources=96,knowledge_chunks=192,knowledge_gemma_embeddings=192)
                assert digest.hexdigest()==metrics['vectors_sha256']=='27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c'
            full[variant].append(dict(name=name,first_ui_ms=run['first_ui_ms'],viewport=run['viewport'],timings=timings,
                idle={k:metrics[k] for k in ['hidden_idle','post_search_idle','active_model_idle','post_repeat_idle']},
                perRoleLast={k:[s for s in run['samples'] if s['phase']==k][-1]['processes'] for k in ['hidden_idle','post_search_idle','active_model_idle','post_repeat_idle']},
                semanticCompletion=metrics['semantic_index_with_bursts']['value'],
                knowledge=dict(counts=counts,modelIndexIdentity=identity,vectorsSha256=digest.hexdigest(),database=dict(path=str(dest),sha256=sha(dest)))))
    assert len({json.dumps(run['knowledge']['modelIndexIdentity']) for runs in full.values() for run in runs})==1,'model/index identities changed'
    retention=json.loads((root/'retention90-lean.json').read_text());parity=json.loads((root/'parity-opening.json').read_text())
    assert retention['completed'] and parity['completed'] and retention['exact_ordered_parity'] and parity['exact_final_query_parity'] and parity['candidate_ime_hide_recovery']
    identities={}
    def collect(value):
        if isinstance(value,dict):
            if all(k in value for k in ['pid','start','role']):identities[(value['pid'],value['start'])]=dict(pid=value['pid'],start=value['start'],role=value['role'])
            else:
                for child in value.values():collect(child)
        elif isinstance(value,list):
            for child in value:collect(child)
    collect(list(raw.values()));collect(retention);collect(parity)
    survivors=[]
    for (pid,start),value in identities.items():
        usage=Usage()
        if LIB.proc_pid_rusage(pid,4,ctypes.byref(usage))==0 and usage.proc_start_abstime==start:survivors.append(value)
    assert not survivors,survivors
    regression=dict(cpuRatio=median([v['timings']['simple_bursts']['familyCpu'] for v in full['accepted']])/median([v['timings']['simple_bursts']['familyCpu'] for v in full['original']]),
        retainedWebContentRatio=median([sum(p['footprint'] for p in v['perRoleLast']['post_repeat_idle'] if p['role']=='com.apple.WebKit.WebContent') for v in full['accepted']])/median([sum(p['footprint'] for p in v['perRoleLast']['post_repeat_idle'] if p['role']=='com.apple.WebKit.WebContent') for v in full['original']]))
    out=dict(schemaVersion=1,base='9ff3663342b48ab01be2e56ab30f6d7123b0ced8',accepted='b427ea1ad9ddeaa57836876e7bcba585714c9786',productionChanges=False,regressionFixed=False,
        focused=focused,full=full,regression=regression,
        diagnostic={k:v for k,v in raw.items() if 'direct' in k or 'owners' in k or 'payload-bytes' in k},
        retention=retention,parityOpening=parity,
        receipts=dict(productionSourceSha256=source,binaries=binaries,
            appPayloadSha256={name:{str(p.relative_to(frozen/name)):sha(p) for p in sorted((frozen/name).rglob('*')) if p.is_file()} for name in ['original-bench.app','accepted-bench.app','original-ordinary.app','accepted-ordinary.app']},
            harnessSourceSha256={str(path.relative_to(repo)):sha(path) for path in (repo/'docs/performance').glob('*.py')},
            artifactSha256={str(path.relative_to(repo)):sha(path) for path in sorted(root.rglob('*')) if path.is_file()},
            ownedIdentities=list(identities.values()),survivors=survivors,
            reusedRust=dict(passed=450,ignored=8,sourceUnchanged=True,log=str(frozen/'accepted-rust-suite.log'),sha256=sha(frozen/'accepted-rust-suite.log')),
            unrelatedHtmlSha256=sha(repo/'docs/performance/beta17-results-report.html')))
    a.output.write_text(json.dumps(out,indent=2)+'\n')
    print(json.dumps(dict(regression=regression,observedOwnedIdentities=len(identities),survivors=survivors)))
    if a.assert_no_regression:
        assert regression['cpuRatio']<=1.05 and regression['retainedWebContentRatio']<=1.05,'RED: simple CPU and/or retained renderer regression remains (5% envelope)'

if __name__=='__main__':main()
