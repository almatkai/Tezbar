#!/usr/bin/env python3
"""Freeze reproducible input-round summaries without rewriting historical evidence."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import statistics

REPO=Path(__file__).resolve().parents[2]
COHORT=[('baseline','input-baseline-3'),('candidate','input-final-candidate-1'),('candidate','input-final-candidate-2'),('baseline','input-baseline-7')]


def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def artifact(path):return dict(path=str(path),sha256=sha(path))
def stats(values):
    v=sorted(values)
    return dict(n=len(v),p50=statistics.median(v),p95=v[math.ceil(len(v)*.95)-1],min=v[0],max=v[-1])


def compact(d):
    out={}
    for name,m in d['metrics'].items():
        if not isinstance(m,dict) or 'wall_s' not in m:
            out[name]=m;continue
        value={k:v for k,v in m.items() if k!='value'}
        v=m.get('value')
        if isinstance(v,list):
            fields=['inputToPaintMs','inputToTwoFramesMs','inputToCorrectDomObservedMs','responseMs','ipcMs','dispatched','completed','obsoleteCompleted']
            value['summaries']={key:stats([s[key] for s in v]) for key in fields if key in v[0]}
            value['samples']=[{k:x for k,x in s.items() if k not in ['finalRows','domTitles','calls']} for s in v]
            value['ordered_rows_actions_sha256']=[hashlib.sha256(json.dumps(s.get('finalRows'),sort_keys=True).encode()).hexdigest() for s in v if 'finalRows' in s]
        elif isinstance(v,dict) and 'bursts' in v:
            value.update({k:x for k,x in v.items() if k!='bursts'})
            value['foreground_bursts']=[{k:x for k,x in s.items() if k not in ['finalRows','domTitles','calls']} for s in v['bursts']]
        else:value['value']=v
        out[name]=value
    return out


def aggregate(runs):
    result={}
    for variant in ['baseline','candidate']:
        selected=[r for r in runs if r['variant']==variant]
        phases={}
        for name in selected[0]['metrics']:
            records=[r['metrics'][name] for r in selected]
            if not isinstance(records[0],dict) or 'wall_s' not in records[0]:continue
            value={key:statistics.median(r[key] for r in records) for key in ['wall_s','cpu_s','cpu_percent_one_core','footprint_median_bytes','footprint_peak_bytes','rss_last_bytes']}
            if 'summaries' in records[0]:
                value['median_of_process_p50']={key:statistics.median(r['summaries'][key]['p50'] for r in records) for key in records[0]['summaries']}
                value['median_of_process_p95']={key:statistics.median(r['summaries'][key]['p95'] for r in records) for key in records[0]['summaries']}
            phases[name]=value
        result[variant]=phases
    return result


def main():
    p=argparse.ArgumentParser();p.add_argument('--artifacts',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
    if a.output.exists():raise ValueError('refusing to overwrite evidence; use a new output path')
    runs=[]
    for variant,name in COHORT:
        path=a.artifacts/(name+'.json');d=json.loads(path.read_text());assert d['completed']
        totals=d['metrics']['bridge_totals'];assert totals['active']==0 and totals['completed']==totals['dispatched'] and not totals['errors']
        for phase in ['simple_bursts','deep_lexical_bursts','deep_model_bursts','repeat_deep_model_bursts']:
            values=d['metrics'][phase]['value'];assert all(v['dispatched']==v['completed'] and not v['errors'] for v in values)
            if phase.startswith('deep') or phase.startswith('repeat'):
                assert all(v['finalRows']==values[0]['finalRows'] for v in values)
        runs.append(dict(variant=variant,raw=artifact(path),log=artifact(a.artifacts/(name+'.log')),binary_sha256=d['binary_sha256'],ready_ms=d['ready_ms'],first_ui_ms=d['first_ui_ms'],metrics=compact(d)))
    assert len({r['metrics']['vectors_sha256'] for r in runs})==1
    old=json.loads((REPO/'docs/performance/beta17-runtime-final-verification.json').read_text())
    unchanged={path:sha(REPO/path)==value for path,value in old['source_sha256'].items()}
    assert all(unchanged.values()),'accepted runtime/search sources unexpectedly changed'
    parity=json.loads((a.artifacts/'responsiveness-search-parity.json').read_text());assert parity['exactOrderedIdScoreParity']
    extra=json.loads((a.artifacts/'input-final-extra-checks.json').read_text());assert extra['completed'] and extra['exact_final_query_parity'] and extra['candidate_ime_hide_recovery']
    binaries={name:artifact(a.artifacts/name) for name in ['desktop-final-app','desktop-responsiveness-final-app','production-responsiveness-final-app','runtime-final-tests']}
    for name in ['desktop-responsiveness-final-app','production-responsiveness-final-app']:
        data=(a.artifacts/name).read_bytes();markers={s.decode():s in data for s in [b'TEZBAR_BENCH_HOME',b'__desktop_bench_reply']}
        assert all(markers.values()) if name.startswith('desktop') else not any(markers.values())
        binaries[name]['benchmark_markers']=markers
    failure_reasons={
        'input-baseline-1':'Harness incorrectly inferred the hidden Deep Search sentinel from presence of result DOM. Corrected helper; partial run retained.',
        'input-baseline-4':'Fresh final rows did not commit within 15s during warm-model UI workload. Cause not established.',
        'input-baseline-5':'Deadline awaiting two animation frames immediately after native show. Cause not established.',
        'input-baseline-6':'Final input was not dispatched as expected (observed truncated query). Cause not established; added anonymous trusted-input interference counter afterwards.',
    }
    files=['src/renderer/CommandBar.tsx','src/renderer/CommandBar.lifecycle.test.ts','src/renderer/latestSearch.ts','docs/performance/responsiveness-desktop.py','docs/performance/responsiveness-checks.py','docs/performance/summarize-responsiveness.py']
    logs=['responsiveness-final-typecheck.log','responsiveness-final-tests.log','responsiveness-final-frontend-build.log','responsiveness-final-desktop-build.log','responsiveness-final-production-build.log','responsiveness-rust-tests.log','responsiveness-search-parity.log','responsiveness-red.log','responsiveness-home-red.log','responsiveness-ime-hide-red.log']
    result=dict(schemaVersion=1,immediate_baseline='6b11da3622cb647fd3e191078629897bc7a27337',original_beta17='9ff3663342b48ab01be2e56ab30f6d7123b0ced8',
        scope='Actual macOS release React input to fresh native response plus two frames; ordered DOM verification after frames, not pixel paint/first commit. Whole responsible-PID + descendant family CPU/ledger footprint/summed RSS.',
        order=[name for _,name in COHORT],order_note='Completed processes in AB/BA order; baseline 4/5/6 failed between final candidate 2 and baseline 7. Initial baseline 3 precedes the IME-hide correction; its binary is unchanged. Builds/tests did not overlap measured processes.',
        targets=dict(uniform_fivefold_visible_search=False,whole_app_fivefold_runtime=False,one_fifth_total_ram=False),
        source_sha256={path:sha(REPO/path) for path in files},accepted_runtime_source_hashes_unchanged=unchanged,
        binaries=binaries,checks=dict(ts_passed=747,ts_skipped=9,rust_passed=441,rust_ignored=7,rust_note='Re-ran preserved accepted release test binary; all native/harness source hashes unchanged; no Rust changes this round.',exact_original_search_signatures=40,original_search_parity=artifact(a.artifacts/'responsiveness-search-parity.json'),opening_and_exact_ui_parity=artifact(a.artifacts/'input-final-extra-checks.json'),logs={name:artifact(a.artifacts/name) for name in logs}),
        summary=aggregate(runs),runs=runs,openings=extra['openings'],
        failures=[dict(reason=reason,raw=artifact(a.artifacts/(name+'.json')),log=artifact(a.artifacts/(name+'.log'))) for name,reason in failure_reasons.items()],
        exploratory=[dict(raw=artifact(a.artifacts/(name+'.json')),log=artifact(a.artifacts/(name+'.log')),metrics=compact(json.loads((a.artifacts/(name+'.json')).read_text()))) for name in ['input-baseline-2','input-candidate-1','input-candidate-2']],
        limitations=['Two successful processes per final version; small samples, failed desktop attempts and uncontrolled focus/system pressure prevent strong tail/reliability claims.',
          'More intermediate native queries complete. No explicit IPC abort; obsolete replies are ignored, pending inputs replaced. One running plus one latest pending per normal/deep lane and backend generation; mode switch can have two running.',
          'Model/Metal caches not flushed. Exploratory baseline 2 incurred new MTLCompilerService work: preserve its 20.92s / 26.05 CPU-s semantic phase, do not generalize warm indexing into cold first-use gains.',
          'Fresh independent profile paths create different source/chunk IDs and tied deep ordering. Timed per-process results are stable; exact cross-version IDs/scores/actions verified separately on APFS clones of the same completed index.',
          'Footprint sums are not deduplicated private RAM; RSS can double-count shared pages and varies with residency/compression. No measured memory fix retained.',
          'Full desktop work remains macOS-only; no Windows/provider/voice/active third-party extension session performance claim. Existing regressions run, not a new all-feature desktop certification.'])
    a.output.write_text(json.dumps(result,indent=2)+'\n')
    print('saved',a.output)
    for variant,phases in result['summary'].items():
        print(variant,{name:phase.get('median_of_process_p50',{}).get('inputToTwoFramesMs',phase.get('median_of_process_p50',{}).get('inputToPaintMs')) for name,phase in phases.items()})

if __name__=='__main__':main()
