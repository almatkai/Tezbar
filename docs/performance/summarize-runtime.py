#!/usr/bin/env python3
"""Summarize preserved desktop AB/BA runs without changing their raw artifacts."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import statistics
import subprocess


def stats(values):
    if not values:
        return None
    first = values[0]
    values = sorted(values)
    return dict(n=len(values), first_ms=first, min_ms=values[0], max_ms=values[-1],
                p50_ms=statistics.median(values), p95_ms=values[(len(values)*95+99)//100-1])


def compact_phase(phase):
    result = {k: v for k, v in phase.items() if k != 'value'}
    value = phase.get('value')
    if isinstance(value, list) and value and isinstance(value[0], dict):
        if 'ms' in value[0]:
            by_query = {}
            for sample in value:
                by_query.setdefault(sample['query'], []).append(sample)
            result['queries'] = {
                query: dict(latency=stats([s['ms'] for s in samples]), counts=[s['count'] for s in samples],
                            ordered_result_hashes=[hashlib.sha256(json.dumps([s['ids'], s['scores']], sort_keys=True).encode()).hexdigest() for s in samples])
                for query, samples in by_query.items()
            }
        elif 'inputToPaintMs' in value[0]:
            result['input_to_paint'] = stats([s['inputToPaintMs'] for s in value])
            result['ipc'] = stats([s['ipcMs'] for s in value])
            result['counts'] = [s['count'] for s in value]
        else:
            result['value'] = value
    elif isinstance(value, dict):
        result['value'] = {k: v for k, v in value.items() if k not in ['searches', 'foreground_searches']}
        for name in ['searches', 'foreground_searches']:
            if name in value:
                result[name] = stats([s['ms'] for s in value[name]])
                result[name+'_counts'] = [s['count'] for s in value[name]]
    else:
        result['value'] = value
    return result


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--artifacts', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--candidate-source-ref', default='9ad23a9', help='Source of the frozen measured candidate, independently of the current checkout')
    args = p.parse_args()
    runs = []
    for number, (name, variant) in enumerate([
            ('baseline-1', 'baseline'), ('candidate-1', 'candidate'),
            ('candidate-2', 'candidate'), ('baseline-2', 'baseline')], 1):
        path = args.artifacts / f'final-desktop-{name}.json'
        raw = json.loads(path.read_text())
        assert raw['completed']
        assert raw['protocol'] == 'visibility-and-durable-completion-v2'
        assert raw['launcher_counts']['files'] == 5000
        assert raw['knowledge_counts']['knowledge_sources'] == 96
        assert raw['knowledge_counts']['knowledge_chunks'] == 192
        assert raw['metrics']['vector_bytes'] == 196608
        assert raw['metrics']['vectors_sha256'] == '27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c'
        assert raw['metrics']['semantic_index']['value']['indexedChunks'] == raw['metrics']['semantic_index']['value']['totalChunks']
        assert raw['metrics']['model_after_images'] == 'ready'
        for phase, visible in [('hidden_idle',False),('visible_idle',True),('post_features',False),('active_model_idle',False)]:
            assert all(s['main_on_screen'] == visible for s in raw['samples'] if s['phase'] == phase)
        phases = {}
        for key, value in raw['metrics'].items():
            if key == 'image_retention':
                assert value['mounted']['loaded'] == 96
                assert value['mounted']['created'] == 96
                assert value['released']['unrevoked'] == (96 if variant == 'baseline' else 0)
                phases[key] = value
            elif isinstance(value, dict) and 'wall_s' in value:
                phases[key] = compact_phase(value)
            else:
                phases[key] = value
        run = dict(order=number, name=name, variant=variant, raw_json=str(path), raw_log=str(path.with_suffix('.log')),
                   raw_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
                   binary_sha256=raw['binary_sha256'], ready_ms=raw['ready_ms'], first_ui_ms=raw['first_ui_ms'],
                   launcher_counts=raw['launcher_counts'], knowledge_counts=raw['knowledge_counts'], phases=phases,
                   component_memory={})
        for phase in ['hidden_idle','visible_idle','post_features','active_model_idle']:
            samples = [s for s in raw['samples'] if s['phase'] == phase]
            roles = sorted({p['role'] for s in samples for p in s['processes']})
            run['component_memory'][phase] = {role: {
                field+'_median_bytes': statistics.median(sum(p[field] for p in s['processes'] if p['role'] == role) for s in samples)
                for field in ['footprint','rss']}
                for role in roles}
        runs.append(run)
    assert all(r['launcher_counts'] == runs[0]['launcher_counts'] for r in runs)
    for variant in ['baseline','candidate']:
        assert len({r['binary_sha256'] for r in runs if r['variant'] == variant}) == 1
    summaries = {}
    for phase in ['hidden_idle','visible_idle','post_features','active_model_idle','index_with_search','semantic_index']:
        summaries[phase] = {}
        for field in ['cpu_s','cpu_percent_one_core','interrupt_wakeups','idle_wakeups','footprint_median_bytes','footprint_peak_bytes','footprint_last_bytes','rss_last_bytes','wall_s']:
            by = {v: [r['phases'][phase][field] for r in runs if r['variant'] == v] for v in ['baseline','candidate']}
            a, b = (statistics.median(by[v]) for v in ['baseline','candidate'])
            summaries[phase][field] = dict(baseline=a,candidate=b,baseline_over_candidate=a/b if b else None,baseline_runs=by['baseline'],candidate_runs=by['candidate'])
    root = Path(__file__).resolve().parents[2]
    source_ref = subprocess.check_output(['git','rev-parse',args.candidate_source_ref], cwd=root, text=True).strip()
    changed = subprocess.check_output(['git','diff','354689e',source_ref,'--name-only'], cwd=root, text=True).splitlines()
    fingerprints = {name: hashlib.sha256(subprocess.check_output(['git','show',source_ref+':'+name],cwd=root)).hexdigest() for name in sorted(set(changed)) if name.startswith(('src/','src-tauri/'))}
    parity_path = args.artifacts/'runtime-search-parity.json'
    parity = json.loads(parity_path.read_text())
    assert parity['exactOrderedIdScoreParity'] is True
    assert len(parity['runs']) == 4 and len(parity['signatures']) == 10
    result = dict(schemaVersion=1, createdAt=datetime.now(timezone.utc).isoformat(),
        runtimeBase='354689e', originalBeta17Base='9ff3663342b48ab01be2e56ab30f6d7123b0ced8',
        branch='fix/beta17-runtime-search-memory',
        measured_source_commit=source_ref,
        host=dict(model='Apple M5 Pro (Mac17,9)',memory_bytes=25769803776,os='macOS 26.3 (25D2125)',rust='1.98.0',node='26.7.0',pnpm='11.17.0',sqlite='bundled 3.46'),
        scope='Release Tauri/WKWebView desktop, synthetic home (5000 files), 96 real extracted text documents, installed Q4 model cloned to an isolated profile; accepted search changes already in runtime base.',
        protocol='visibility-and-durable-completion-v2',
        accounting=dict(ownership='Exact macOS responsible PID plus recursive PPID descendants, all process names; unrelated installed apps excluded.',
                        memory='Sum of proc_pid_rusage v4 physical footprints, including model/GPU/XPC; not unique deduplicated private bytes. Summed RSS reported separately.',
                        cpu='Mach absolute ticks converted with mach_timebase_info (125/3 ns per tick), verified against getrusage.',
                        sampling_seconds=0.5,limitations='Sampled peaks; short-lived processes or final CPU after last sample can be missed. Shared ledger charges may overlap. No host-only or cache-estimate RAM claim.'),
        runs=runs, summary=summaries, changed_source_sha256=fingerprints,
        exact_search_parity=dict(artifact=str(parity_path),sha256=hashlib.sha256(parity_path.read_bytes()).hexdigest(),all_40_case_signatures_match=True),
        production_binary=dict(path=str(args.artifacts/'production-candidate-app'),sha256=hashlib.sha256((args.artifacts/'production-candidate-app').read_bytes()).hexdigest()),
        checks=dict(rust='440 passed, 7 ignored; complete final serialized suite',typescript='736 passed, 9 skipped',typecheck='passed',frontend_build='passed',desktop_bench_build='passed',production_app_build='passed',
                    failures='One complete final Rust run timed out in existing native_core::extensions::launcher::tests::timed_out_launcher_query_settles_and_recovers; isolated rerun and subsequent complete suite passed.'),
        excluded_desktop_attempts=[dict(artifact=str(args.artifacts/'desktop-candidate-6-ui-timeout.json'),reason='Earlier UI-search automation timeout; retry passed.'),
                                   dict(artifact=str(args.artifacts/'final-desktop-candidate-2-visibility-failed.json'),reason='Native window disappeared during visible idle; partial run excluded, unchanged-code retry required.')],
        text_probes=dict(baseline_log=str(args.artifacts/'runtime-text-baseline.log'),candidate_log=str(args.artifacts/'runtime-text-candidate.log')))
    opening_path = args.artifacts/'runtime-opening.json'
    result['opening'] = dict(artifact=str(opening_path),sha256=hashlib.sha256(opening_path.read_bytes()).hexdigest(),**json.loads(opening_path.read_text()))
    args.output.write_text(json.dumps(result,indent=2)+'\n')
    print('saved',args.output)
    for phase,fields in summaries.items():
        print(phase, 'CPU %',round(fields['cpu_percent_one_core']['baseline'],3),'->',round(fields['cpu_percent_one_core']['candidate'],3),'footprint MiB',round(fields['footprint_median_bytes']['baseline']/2**20,2),'->',round(fields['footprint_median_bytes']['candidate']/2**20,2))


if __name__ == '__main__':
    main()
