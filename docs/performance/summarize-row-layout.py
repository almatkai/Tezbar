#!/usr/bin/env python3
"""Audit R11 row-layout experiments after rollback; retain raw evidence by hash.

Refuses production drift, incomplete runs, changed reused evidence or survivors.
Does not infer acceptance from fewer reads or favorable footprint outliers.
"""
import argparse
import ctypes
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import shutil
import sqlite3
import statistics
import subprocess
from desktop import LIB, Usage, TIMEBASE

spec=importlib.util.spec_from_file_location('ui_summary',Path(__file__).with_name('summarize-ui-allocation.py'))
u=importlib.util.module_from_spec(spec)
spec.loader.exec_module(u)
def sha(path):
    digest=hashlib.sha256()
    with Path(path).open('rb') as f:
        for block in iter(lambda:f.read(1024*1024),b''):digest.update(block)
    return digest.hexdigest()


def write_new(path,value):
    with path.open('x') as f:f.write(json.dumps(value,indent=2)+'\n')


def frames_summary(result):
    out={}
    for name,step in result['steps'].items():
        frames=step['frames'] if isinstance(step,dict) else step
        events=step.get('events',[]) if isinstance(step,dict) else []
        mismatches=[]
        for i,f in enumerate(frames):
            if f.get('empty'):continue
            if abs(f['top']-f['highlightTop'])<=1 and abs(f['height']-f['highlightHeight'])<=1:continue
            later=[e for e in events if e['now']>=f['now']]
            mismatches.append(dict(index=i,frame=f.get('timestamp'),nextProductionRead=later[0] if later else None))
        out[name]=dict(samples=len(frames),initial=frames[0],last=frames[-1],earlyCallbackMismatches=mismatches)
    return out


def main():
    p=argparse.ArgumentParser()
    p.add_argument('--artifacts',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True)
    a=p.parse_args();repo=Path(__file__).resolve().parents[2];root=a.artifacts.resolve(strict=True)
    raw={p.parent.name:json.loads(p.read_text()) for p in root.glob('*/result.json')}
    assert raw and all(r['completed'] and not r['survivors'] for r in raw.values())
    phases=['simple_bursts','deep_lexical_bursts','deep_model_bursts','repeat_deep_model_bursts','source_index_with_bursts','semantic_index_with_bursts']
    quiet=['hidden_idle','post_search_idle','active_model_idle','post_repeat_idle']
    diagnostics={};short={};full={};frames={};trajectory={};details={};dbs={}
    for name,r in raw.items():
        if name.startswith('frames-'):
            frames[name]=frames_summary(r)
            continue
        if name.startswith('trajectory-'):
            blocks=[]
            for i,b in enumerate(r['metrics']['simple_bursts']['blocks']):
                values=b['work']['value']
                assert all(not v['trustedInputEvents'] and not v['errors'] and v['completed']==v['dispatched'] for v in values)
                roles=u.last_roles(r,'trajectory_quiet_'+str(i))
                blocks.append(dict(familyCpu=b['work']['cpu_s'],perRoleCpu=u.phase_cpu(r,'trajectory_work_'+str(i)),latency=u.latency(values),dispatches=[v['dispatched'] for v in values],quietFamilyMiB=b['quiet']['footprint_median_bytes']/2**20,quietWebMiB=u.role_sum(roles,role='com.apple.WebKit.WebContent')/2**20))
            trajectory[name]=dict(app=r['app'],binarySha256=r['binary_sha256'],diagnosticOnly=True,blocks=blocks)
            trajectory[name]['final30s']=dict(familyMiB=r['metrics']['after']['footprint_median_bytes']/2**20,webMiB=u.role_sum(u.last_roles(r),role='com.apple.WebKit.WebContent')/2**20,cpu=r['metrics']['after']['cpu_s'])
            continue
        if r.get('bridge',{}).get('diagnosticOnly'):
            path=root/name/'com.apple.WebKit.WebContent-sample.txt'
            text=path.read_text().split('Total number in stack')[0] if path.exists() else ''
            symbols=['WebCore::jsHTMLElement_offsetTop','WebCore::Document::updateLayout(','WebCore::Page::updateRendering(','WebCore::Document::updateStyleIfNeeded(']
            diagnostics[name]=dict(bridge=r['bridge'],familyCpu=r['metrics']['simple_bursts']['cpu_s'],perRoleCpu=u.phase_cpu(r,'simple_bursts'),nativeInclusiveSamples={fn:sum(int(n) for n in re.findall(r'(\d+) '+re.escape(fn),text)) for fn in symbols})
            continue
        timings={}
        for phase in phases:
            if phase not in r['metrics']:continue
            m=r['metrics'][phase];values=m['value'];values=values['bursts'] if isinstance(values,dict) else values
            assert all(not v['trustedInputEvents'] and not v['errors'] and v['completed']==v['dispatched'] for v in values)
            cpu=u.phase_cpu(r,phase);assert abs(sum(cpu.values())-m['cpu_s'])<1e-8
            timings[phase]=dict(familyCpu=m['cpu_s'],perRoleCpu=cpu,latency=u.latency(values),dispatches=[v['dispatched'] for v in values])
        details[name]=dict(app=r['app'],binarySha256=r['binary_sha256'],timings=timings,firstUiMs=r['first_ui_ms'])
        if '-short-' in name:
            roles=u.last_roles(r)
            short[name]=dict(**timings['simple_bursts'],webMiB=u.role_sum(roles,role='com.apple.WebKit.WebContent')/2**20,familyMiB=u.role_sum(roles)/2**20)
            details[name].update(before=r['metrics']['before'],after=r['metrics']['after'],lastRoles=roles)
            continue
        details[name].update(quiet={q:r['metrics'][q] for q in quiet},quietPerRoleCpu={q:u.phase_cpu(r,q) for q in quiet},quietLastRoles={q:u.last_roles(r,q) for q in quiet},completion=r['metrics']['semantic_index_with_bursts']['value'],openingCycles=r['metrics']['search_open_close'])
        full[name]=dict(firstUiMs=r['first_ui_ms'],timings=timings,quiet={q:dict(familyMiB=r['metrics'][q]['footprint_median_bytes']/2**20,webMiB=u.role_sum(u.last_roles(r,q),role='com.apple.WebKit.WebContent')/2**20,cpu=r['metrics'][q]['cpu_s']) for q in quiet},completion={k:v for k,v in r['metrics']['semantic_index_with_bursts']['value'].items() if k!='bursts'})
        source=Path(r['root'])/'data/knowledge/knowledge.sqlite3';wal=Path(str(source)+'-wal')
        assert not wal.exists() or wal.stat().st_size==0
        dest=root/'closed-fixtures'/name/'knowledge.sqlite3';dest.parent.mkdir(parents=True,exist_ok=True)
        if not dest.exists():shutil.copyfile(source,dest)
        assert sha(source)==sha(dest)
        with sqlite3.connect('file:'+str(dest)+'?mode=ro&immutable=1',uri=True) as db:
            counts={t:db.execute('SELECT count(*) FROM '+t).fetchone()[0] for t in ['knowledge_sources','knowledge_chunks','knowledge_gemma_embeddings']}
            identity=db.execute('SELECT model_key,count(*),sum(length(vector)) FROM knowledge_gemma_embeddings GROUP BY model_key').fetchall();digest=hashlib.sha256()
            for path,offset,vector in db.execute('SELECT s.path,c.start_offset,g.vector FROM knowledge_gemma_embeddings g JOIN knowledge_chunks c ON c.id=g.chunk_id JOIN knowledge_sources s ON s.id=c.source_id ORDER BY s.path,c.start_offset'):
                assert len(vector)==256*4
                digest.update(Path(path).name.encode());digest.update(str(offset).encode());digest.update(vector)
        assert counts==dict(knowledge_sources=96,knowledge_chunks=192,knowledge_gemma_embeddings=192)
        assert digest.hexdigest()==r['metrics']['vectors_sha256']=='27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c'
        dbs[name]=dict(path=str(dest.relative_to(repo)),sha256=sha(dest),counts=counts,identity=identity,vectorsSha256=digest.hexdigest())
    assert len({json.dumps(v['identity']) for v in dbs.values()})==1
    old=json.loads((repo/'build/performance-layout-20261030/evidence-manifest.json').read_text())
    source={n:sha(repo/n) for n in old['sourceSha256']}
    changed={n:value for n,value in source.items() if value!=old['sourceSha256'][n]}
    assert set(changed)=={'src/renderer/CommandBar.lifecycle.test.ts'},'production was not restored to R10'
    added={n:sha(repo/n) for n in subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','src'],cwd=repo,text=True).splitlines() if n not in source}
    for n,value in old['reused'].items():
        if isinstance(value,str):assert sha(repo/n)==value,n
    for n,value in old['reusedExactness'].items():assert sha(Path(n))==value,n
    for model in old['installedModels'].values():
        weights=Path(model['weights']);assert sha(weights)==model['weightsSha256']
        for n,value in model['files'].items():assert sha(weights.parent/n)==value
    zipfile=repo/'build/embedding-runtime/resources/embedding-runtime.zip';assert sha(zipfile)==old['zipSha256']
    r7=json.loads((repo/'docs/performance/beta17-final-results.json').read_text())
    frozen=repo/'build/performance-runtime-20261030/cumulative-20261030'
    for app,files in r7['appFiles'].items():
        path=frozen/app
        assert {str(f.relative_to(path)):sha(f) for f in path.rglob('*') if f.is_file()}=={n:v['sha256'] for n,v in files.items()}
    apps={}
    for receipt in root.glob('*-receipt.json'):
        app=root/(receipt.name.removesuffix('-receipt.json')+'.app')
        files={str(f.relative_to(app)):sha(f) for f in app.rglob('*') if f.is_file()}
        assert files==json.loads(receipt.read_text()),app
        apps[app.name]=files
    identities={}
    def collect(v):
        if isinstance(v,dict):
            if all(k in v for k in ['pid','start','role']):identities[(v['pid'],v['start'])]={k:v[k] for k in ['pid','start','role']}
            else:
                for child in v.values():collect(child)
        elif isinstance(v,list):
            for child in v:collect(child)
    collect(raw)
    for (pid,start),value in identities.items():
        usage=Usage();assert not (LIB.proc_pid_rusage(pid,4,ctypes.byref(usage))==0 and usage.proc_start_abstime==start),value
    assert sha(repo/'docs/performance/beta17-results-report.html')=='1ee1b0742261dbfb1ad8071817778b5881cb82c084bda482684117279f45816d'
    assert all(v['exit']==0 for v in json.loads((root/'qa-final-gates.json').read_text()))
    for n,value in json.loads((root/'qa-final-source.json').read_text()).items():assert sha(repo/n)==value
    assert all(s in (root/'qa-final-typescript.log').read_text() for s in ['751 passed','9 skipped'])
    med=statistics.median
    def ratios(group):
        pre=[v for n,v in group.items() if n.endswith('prefix')];cand=[v for n,v in group.items() if not n.endswith('prefix')]
        return dict(familySimpleCpu=med(v['familyCpu'] for v in cand)/med(v['familyCpu'] for v in pre),webSimpleCpu=med(v['perRoleCpu']['com.apple.WebKit.WebContent'] for v in cand)/med(v['perRoleCpu']['com.apple.WebKit.WebContent'] for v in pre))
    comparison=dict(short=ratios(short),full=ratios({n:v['timings']['simple_bursts'] for n,v in full.items()}))
    detail_path=root/(a.output.stem+'-details.json');manifest_path=root/(a.output.stem+'-manifest.json')
    write_new(detail_path,dict(processes=details,frames=frames,trajectory=trajectory))
    manifest=dict(sourceSha256=source,changedInputs=changed,addedSourceSha256=added,reused=old['reused'],installedModels=old['installedModels'],reusedExactness=old['reusedExactness'],zipSha256=sha(zipfile),rejectedAppFiles=apps,ownedIdentities=list(identities.values()),survivors=[],harnessSha256={str(f.relative_to(repo)):sha(f) for f in (repo/'docs/performance').glob('*.py')},rawFiles={str(f.relative_to(root)):sha(f) for f in root.rglob('*') if f.is_file() and not any(part.endswith('.app') for part in f.relative_to(root).parts)},databases=dbs)
    write_new(manifest_path,manifest)
    out=dict(schemaVersion=1,startHead='e94d0e050a0013771faf1b9b1ce2a04164d7ef53',productionChanges=False,acceptedCandidate=None,goalAchieved=False,regressionFixed=False,decisions=dict(rowSkipping=dict(accepted=False,reason='Actual supported skipping increases forced-layout time and CPU'),batching=dict(accepted=False,reason='Late-rAF commit updates highlight in the next frame; safe retained RAM not established',fullSimpleCpuImproves=True,ramCausalLeakProven=False)),diagnostic=diagnostics,short=short,full=full,trajectory=trajectory,comparison=comparison,accountingTimebase=dict(numer=TIMEBASE.numer,denom=TIMEBASE.denom),receipts=dict(artifacts=str(root.relative_to(repo)),manifestPath=str(manifest_path.relative_to(repo)),manifestSha256=sha(manifest_path),detailsPath=str(detail_path.relative_to(repo)),detailsSha256=sha(detail_path),productionInputCount=len(source),changedBuildInputs=changed,reusedRustPassed=450,reusedRustIgnored=8,reusedNativeSignatures=40,reusedExactArtifacts=len(old['reusedExactness']),ownedIdentities=len(identities),survivors=[],durableIndexes=len(dbs)))
    write_new(a.output,out)
    print(json.dumps(dict(comparison=comparison,receipts=out['receipts'])))


if __name__=='__main__':main()
