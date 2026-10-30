#!/usr/bin/env python3
"""Freeze R10 negative layout experiments; no final/original matrix is invented.

Detailed per-process/per-role results, source pins and raw-file manifest stay in
ignored artifacts. The tracked summary points to those receipts by SHA-256.
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
from desktop import LIB, Usage, TIMEBASE

spec=importlib.util.spec_from_file_location('ui_summary',Path(__file__).with_name('summarize-ui-allocation.py'))
u=importlib.util.module_from_spec(spec);spec.loader.exec_module(u)
sha=u.sha


def write_new(path,value):
    if path.exists():raise ValueError('refusing to overwrite '+str(path))
    path.write_text(json.dumps(value,indent=2)+'\n')


def main():
    p=argparse.ArgumentParser();p.add_argument('--artifacts',type=Path,required=True);p.add_argument('--output',type=Path,required=True)
    a=p.parse_args();repo=Path(__file__).resolve().parents[2];root=a.artifacts.resolve(strict=True)
    frozen=repo/'build/performance-runtime-20261030/cumulative-20261030'
    raw={path.parent.name:json.loads(path.read_text()) for path in root.glob('*/result.json')}
    assert len(raw)==18,len(raw)
    assert all(r['completed'] and not r['survivors'] for r in raw.values())
    phases=['simple_bursts','deep_lexical_bursts','deep_model_bursts','repeat_deep_model_bursts','source_index_with_bursts','semantic_index_with_bursts']
    quiet=['hidden_idle','post_search_idle','active_model_idle','post_repeat_idle']
    details={};short={};full={};diagnostic={};dbs={}
    for name,r in raw.items():
        if name.startswith('diag-'):
            text=(root/name/'com.apple.WebKit.WebContent-sample.txt').read_text().split('Total number in stack')[0]
            stacks={fn:sum(int(n) for n in re.findall(r'(\d+) '+re.escape(fn),text)) for fn in ['WebCore::jsHTMLElement_offsetTop','WebCore::Document::updateLayout(','WebCore::Page::updateRendering(']}
            diagnostic[name]=dict(bridge=r['bridge'],familyCpu=r['metrics']['simple_bursts']['cpu_s'],perRoleCpu=u.phase_cpu(r,'simple_bursts'),nativeInclusiveSamples=stacks)
            continue
        timings={}
        for phase in phases:
            if phase not in r['metrics']:continue
            m=r['metrics'][phase];values=m['value'];values=values['bursts'] if isinstance(values,dict) else values
            assert all(not v['trustedInputEvents'] and not v['errors'] and v['completed']==v['dispatched'] for v in values)
            cpu=u.phase_cpu(r,phase)
            assert abs(sum(cpu.values())-m['cpu_s'])<1e-8
            timings[phase]=dict(familyCpu=m['cpu_s'],perRoleCpu=cpu,latency=u.latency(values),dispatches=[v['dispatched'] for v in values])
        details[name]=dict(app=r['app'],binarySha256=r['binary_sha256'],timings=timings,firstUiMs=r['first_ui_ms'])
        if '-short-' in name:
            roles=u.last_roles(r)
            value=dict(name=name,**timings['simple_bursts'],webMiB=u.role_sum(roles,role='com.apple.WebKit.WebContent')/2**20,familyMiB=u.role_sum(roles)/2**20)
            short[name]=value
            details[name].update(before=r['metrics']['before'],after=r['metrics']['after'],lastRoles=roles)
        else:
            details[name].update(idle={phase:r['metrics'][phase] for phase in quiet},idlePerRoleCpu={phase:u.phase_cpu(r,phase) for phase in quiet},lastRoles={phase:u.last_roles(r,phase) for phase in quiet},completion=r['metrics']['semantic_index_with_bursts']['value'],openingCycles=r['metrics']['search_open_close'])
            full[name]=dict(name=name,firstUiMs=r['first_ui_ms'],simple=timings['simple_bursts'],quiet={phase:dict(familyMiB=r['metrics'][phase]['footprint_median_bytes']/2**20,webMiB=u.role_sum(u.last_roles(r,phase),role='com.apple.WebKit.WebContent')/2**20,cpu=r['metrics'][phase]['cpu_s']) for phase in quiet})
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
    # Verify final restored production, not the rejected apps labelled "final".
    old=json.loads((frozen/'accepted-source-sha256.json').read_text())
    source={n:sha(repo/n) for n in old}
    changed={n:v for n,v in source.items() if v!=old[n]}
    assert set(changed)=={'src/renderer/CommandBar.lifecycle.test.ts'},changed
    r7=json.loads((repo/'docs/performance/beta17-final-results.json').read_text())
    reuse={}
    for label,record in r7['binaries'].items():
        path=frozen/label;assert sha(path)==record['sha256'];reuse[str(path.relative_to(repo))]=sha(path)
    for label,files in r7['appFiles'].items():
        path=frozen/label
        assert {str(f.relative_to(path)) for f in path.rglob('*') if f.is_file()}==set(files)
        for name,record in files.items():assert sha(path/name)==record['sha256']
        reuse[str(path.relative_to(repo))]=dict(files=len(files),manifestSha256=hashlib.sha256(json.dumps(files,sort_keys=True).encode()).hexdigest())
    r9=repo/'build/performance-ui-allocation-20261030/reused-native.json';native=json.loads(r9.read_text())
    for n,value in native['sourceSha256'].items():assert sha(repo/n)==value
    assert sha(frozen/'accepted-native-tests')==native['testBinarySha256']
    assert sha(frozen/'accepted-rust-suite.log')==native['testLogSha256']
    assert sha(frozen/'native-comparison.json')==r7['evidence']['native-comparison.json']['sha256']
    assert r7['native']['exactOrderedIdScoreParity']
    reuse[str(r9.relative_to(repo))]=sha(r9)
    reuse[str((frozen/'accepted-rust-suite.log').relative_to(repo))]=sha(frozen/'accepted-rust-suite.log')
    reuse[str((frozen/'native-comparison.json').relative_to(repo))]=sha(frozen/'native-comparison.json')
    installed=json.loads((frozen/'installed-models-before.json').read_text())
    for model in installed.values():
        weights=Path(model['weights']);assert sha(weights)==model['weightsSha256']
        for n,value in model['files'].items():assert sha(weights.parent/n)==value
    previous=json.loads((repo/'docs/performance/beta17-runtime-package-results.json').read_text());exact={}
    assert previous['catalog']==json.loads((repo/'src-tauri/runtime/embedding-macos-arm64.json').read_text())
    for n,record in previous['evidence'].items():
        if n.startswith(('exact-','bounds-')):
            path=Path(previous['artifacts'])/n;assert sha(path)==record['sha256'];exact[str(path)]=record['sha256']
    zipfile=repo/'build/embedding-runtime/resources/embedding-runtime.zip'
    assert sha(zipfile)=='75698aa84eb35398cee7b3afb6fee7b9bdb1db7a3aedc3cc43af25780100e475'
    apps={}
    for label,kinds in [('geometry',['bench']),('width',['bench']),('final',['ordinary','bench'])]:
        for kind in kinds:
            path=root/(label+'-'+kind+'.app');files={str(f.relative_to(path)):sha(f) for f in path.rglob('*') if f.is_file()}
            assert files==json.loads((root/(label+'-'+kind+'-receipt.json')).read_text())
            apps[path.name]=files
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
        usage=Usage()
        assert not (LIB.proc_pid_rusage(pid,4,ctypes.byref(usage))==0 and usage.proc_start_abstime==start),value
    assert sha(repo/'docs/performance/beta17-results-report.html')=='1ee1b0742261dbfb1ad8071817778b5881cb82c084bda482684117279f45816d'
    assert all(v['exit']==0 for v in json.loads((root/'qa-final-gates.json').read_text()))
    for name,needles in {'qa-final-typescript.log':['750 passed','9 skipped'],'qa-final-harness.log':['Ran 8 tests','OK'],'qa-final-probe.log':['Ran 2 tests','OK']}.items():
        assert all(s in (root/name).read_text() for s in needles),name
    median=statistics.median
    decisions={}
    for prototype in ['geometry','width']:
        group=[v for n,v in short.items() if n.startswith(prototype+'-')]
        pre=[v for v in group if v['name'].endswith('prefix')];cand=[v for v in group if not v['name'].endswith('prefix')]
        decisions[prototype]=dict(accepted=False,shortFamilyCpuRatio=median(v['familyCpu'] for v in cand)/median(v['familyCpu'] for v in pre),shortWebCpuRatio=median(v['perRoleCpu']['com.apple.WebKit.WebContent'] for v in cand)/median(v['perRoleCpu']['com.apple.WebKit.WebContent'] for v in pre))
    pre=[v for n,v in full.items() if n.endswith('prefix')];cand=[v for n,v in full.items() if not n.endswith('prefix')]
    decisions['width']['fullRatios']=dict(familySimpleCpu=median(v['simple']['familyCpu'] for v in cand)/median(v['simple']['familyCpu'] for v in pre),postRepeatWeb=median(v['quiet']['post_repeat_idle']['webMiB'] for v in cand)/median(v['quiet']['post_repeat_idle']['webMiB'] for v in pre),postRepeatFamily=median(v['quiet']['post_repeat_idle']['familyMiB'] for v in cand)/median(v['quiet']['post_repeat_idle']['familyMiB'] for v in pre))
    manifest=dict(sourceSha256=source,changedInputs=changed,reused=reuse,installedModels=installed,reusedExactness=exact,zipSha256=sha(zipfile),rejectedAppFiles=apps,ownedIdentities=list(identities.values()),survivors=[],harnessSha256={str(f.relative_to(repo)):sha(f) for f in (repo/'docs/performance').glob('*.py')},rawFiles={str(f.relative_to(root)):sha(f) for f in root.rglob('*') if f.is_file() and not any(part.endswith('.app') for part in f.relative_to(root).parts)},databases=dbs)
    manifest_path=root/(a.output.stem+'-manifest.json');detail_path=root/(a.output.stem+'-details.json')
    write_new(detail_path,dict(processes=details,diagnostic=diagnostic))
    manifest['rawFiles'][detail_path.name]=sha(detail_path)
    write_new(manifest_path,manifest)
    out=dict(schemaVersion=1,startHead='1803f3d8a36be91ba727e3c6051b7520306ba025',productionChanges=False,acceptedCandidate=None,goalAchieved=False,regressionFixed=False,decisions=decisions,diagnostic=diagnostic,short=short,full=full,accountingTimebase=dict(numer=TIMEBASE.numer,denom=TIMEBASE.denom),receipts=dict(artifacts=str(root.relative_to(repo)),manifestPath=str(manifest_path.relative_to(repo)),manifestSha256=sha(manifest_path),detailsPath=str(detail_path.relative_to(repo)),detailsSha256=sha(detail_path),productionInputCount=len(source),changedBuildInputs=changed,reusedRustPassed=450,reusedRustIgnored=8,reusedNativeSignatures=40,reusedExactArtifacts=len(exact),ownedIdentities=len(identities),survivors=[],durableIndexes=len(dbs)),limitations=['All candidates rejected and reverted. No new original/final acceptance matrix or 90s retention claim.', 'First four full receipts retain default production_label=current, including two rejected candidate apps; app path/binary SHA and command timeline, not that default metadata, identify each build.', 'Diagnostic sample is a ten-second inclusive native stack census, not an exact CPU percentage or a symbolized React JS calltree.', 'Timers are quantized wall time with diagnostic overhead; no retainer graph, live leak or stable causal memory regression is proved.'])
    write_new(a.output,out)
    print(json.dumps(dict(decisions=decisions,receipts=out['receipts'])))


if __name__=='__main__':main()
