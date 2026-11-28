#!/usr/bin/env python3
"""Freeze this investigation's explicitly named cohort and exactness evidence.

No glob selects successful retries or silently replaces failures. Raw evidence
remains local; summaries, hashes and reproduction parameters are committed.
"""
import argparse
import hashlib
import json
from pathlib import Path
import statistics


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def roles(samples, phase):
    rows = [s for s in samples if s['phase'] == phase]
    identities = {}
    initial = {(p['pid'], p['start']) for p in rows[0]['processes']}
    for s in rows:
        for p in s['processes']:
            identities.setdefault((p['pid'], p['start']), []).append(p)
    result = {}
    for key, values in identities.items():
        role = values[-1]['role']
        entry = result.setdefault(role, dict(cpu_s=0, interrupt_wakeups=0, settled_footprint_bytes=0, settled_rss_bytes=0))
        first, last = values[0], values[-1]
        entry['cpu_s'] += last['cpu_s'] - (first['cpu_s'] if key in initial else 0)
        entry['interrupt_wakeups'] += last['wakeups'] - (first['wakeups'] if key in initial else 0)
        final = next((p for p in rows[-1]['processes'] if (p['pid'], p['start']) == key), None)
        if final:
            entry['settled_footprint_bytes'] += final['footprint']
            entry['settled_rss_bytes'] += final['rss']
    return result


def desktop_record(d, name, variant):
    phases = ['hidden_idle', 'post_search_idle', 'active_model_idle', 'post_repeat_idle']
    r = dict(name=name, variant=variant, binary_sha256=d['binary_sha256'], first_ui_ms=d['first_ui_ms'],
             experimental_library=d.get('experimental_library'),
             phases={k: d['metrics'][k] for k in phases},
             components={k: roles(d['samples'], k) for k in phases},
             inspections=d['inspections'], bridge_totals=d['metrics']['bridge_totals'])
    for key in ['search_open_close', 'simple_bursts', 'source_index_with_bursts', 'deep_lexical_bursts',
                'semantic_index_with_bursts', 'deep_model_bursts', 'repeat_deep_model_bursts']:
        p = d['metrics'][key]
        r[key] = {k: v for k, v in p.items() if k != 'value'}
        r[key]['components'] = roles(d['samples'], key)
        values = p['value'] if isinstance(p['value'], list) else p['value'].get('bursts', [])
        if values:
            field = 'inputToTwoFramesMs' if 'inputToTwoFramesMs' in values[0] else 'inputToPaintMs'
            timings = sorted(v[field] for v in values)
            r[key]['two_frame_p50_ms'] = statistics.median(timings)
            r[key]['two_frame_p95_ms'] = timings[(len(timings)*95+99)//100-1]
            for field in ['responseMs', 'ipcMs']:
                if field in values[0]:
                    r[key][field+'_p50'] = statistics.median(v[field] for v in values)
            r[key]['dispatched'] = sum(v.get('dispatched', 0) for v in values)
        if isinstance(p['value'], dict):
            r[key]['completion'] = {k: v for k, v in p['value'].items() if k != 'bursts'}
    return r


def same_payloads(a, b):
    def signatures(d):
        return {v['name']: (v.get('raw_sha256'), v.get('normalized_f32_sha256'), v.get('error')) for v in d['workloads']}
    assert signatures(a) == signatures(b), 'raw/norm payload identity or input-boundary behavior changed'


def row_signature(rows):
    return dict(count=len(rows), sha256=hashlib.sha256(json.dumps(rows, sort_keys=True,
        separators=(',', ':'), ensure_ascii=False).encode()).hexdigest())


def compact_ui(d):
    result = {k: v for k, v in d.items() if k != 'parity'}
    result['parity'] = []
    for run in d.get('parity', []):
        values = [dict(query=v['query'], ordered_rows=row_signature(v['rows']),
                       dispatched=v['dispatched'], dom_titles=row_signature(v['domTitles'])) for v in run['values']]
        result['parity'].append(dict(variant=run['variant'], root=run['root'], values=values))
    return result


def compact_retention(d):
    result = {k: v for k, v in d.items() if k != 'runs'}
    result['runs'] = []
    for run in d['runs']:
        value = {k: v for k, v in run.items() if k not in ['samples', 'first', 'recovery', 'ordered_parity']}
        for key in ['first', 'recovery']:
            value[key] = {k: v for k, v in run[key].items() if k not in ['finalRows', 'domTitles']}
            value[key]['ordered_rows'] = row_signature(run[key]['finalRows'])
        if 'ordered_parity' in run:
            value['ordered_parity'] = [dict(query=v['query'], ordered_rows=row_signature(v['rows'])) for v in run['ordered_parity']]
        result['runs'].append(value)
    return result


def model_record(d):
    return {k: v for k, v in d.items() if k not in ['samples', 'props']} | dict(
        workload_components={w['name']: roles(d['samples'], w['name'])
            for w in d['workloads'] if 'error' not in w})


def compact_failure(d):
    result = {k: v for k, v in d.items() if k not in ['samples', 'metrics']}
    result['metrics'] = {}
    for key, phase in d.get('metrics', {}).items():
        result['metrics'][key] = {k: v for k, v in phase.items() if k != 'value'}
        if isinstance(phase.get('value'), dict):
            result['metrics'][key]['completion'] = {k: v for k, v in phase['value'].items() if k != 'bursts'}
    return result


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--artifacts', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    if a.output.exists():
        raise ValueError('refusing to overwrite evidence')
    root = a.artifacts.resolve()
    evidence = {}

    def read(relative):
        path = root / relative
        evidence[relative] = dict(path=str(path), sha256=sha(path))
        return json.loads(path.read_text())

    result = dict(baseline_source='bcef608c65b6bbbaf53326d2fdcceb1600ae1701',
                  scope='macOS release, same Q4/Q8 models, no global pressure/cache manipulation',
                  memory_accounting='Summed physical-footprint ledgers, not deduplicated private RAM; RSS can double-count clean/shared pages.',
                  models=[], modes=[], desktop=[], checks={}, evidence=evidence)
    reference = read('memory-model-baseline-2/vectors.json')
    for config in ['baseline', 'ubatch2048', 'no-warmup', 'poll0', 'http1', 'flash-on', 'no-residency']:
        name = 'memory-model-baseline-2' if config == 'baseline' else 'memory-model-'+config+'-1'
        d = read(name+'/result.json')
        assert d['completed'] and not d['owned_survivors']
        vectors = read(name+'/vectors.json')
        exact = {k: values == reference[k] for k, values in vectors.items()}
        if config != 'ubatch2048':
            assert vectors == reference, config+' changed embedding identity'
        result['models'].append(model_record(d)
                                | dict(exact_vectors=exact, component_costs={phase: roles(d['samples'], phase)
                                    for phase in ['ready_idle', 'indexed_idle', 'long_input_idle']}))
    # Failed first boundary construction is retained; never mixed into successful timings.
    result['failed_model_attempt'] = read('memory-model-baseline-1/result.json')
    result['failed_model_attempt'].pop('samples')
    for variant in ['q4', 'q8']:
        for threads in [2, 4, 8]:
            runs = {}
            vectors = {}
            for config in ['baseline', 'no-residency']:
                name = f'memory-mode-{variant}-t{threads}-{config}-1'
                d = read(name+'/result.json')
                assert d['completed'] and not d['owned_survivors']
                runs[config] = model_record(d)
                vectors[config] = read(name+'/vectors.json')
            assert vectors['baseline'] == vectors['no-residency']
            result['modes'].append(dict(variant=variant, threads=threads, exact_vectors=True, runs=runs))
    result['q8_full'] = []
    q8_vectors = []
    for config in ['baseline', 'no-residency']:
        name = 'memory-q8-full-'+config+'-1'
        d = read(name+'/result.json')
        assert d['completed'] and not d['owned_survivors']
        q8_vectors.append(read(name+'/vectors.json'))
        result['q8_full'].append({k: v for k, v in d.items() if k not in ['samples', 'props']})
    assert q8_vectors[0] == q8_vectors[1]

    for variant, number in [('baseline', 1), ('candidate', 1), ('candidate', 2), ('baseline', 2)]:
        name = f'memory-desktop-{variant}-{number}'
        d = read(name+'/result.json')
        cleanup = read(name+'/cleanup.json')
        assert d['completed'] and not cleanup['owned_survivors'], name
        assert d['metrics']['vectors_sha256'] == '27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c'
        phases = ['hidden_idle', 'post_search_idle', 'active_model_idle', 'post_repeat_idle']
        summary = dict(variant=variant, name=name, binary_sha256=d['binary_sha256'],
                       first_ui_ms=d['first_ui_ms'], phases={key: d['metrics'][key] for key in phases},
                       components={key: roles(d['samples'], key) for key in phases},
                       inspections=d['inspections'])
        for key in ['search_open_close', 'simple_bursts', 'source_index_with_bursts', 'deep_lexical_bursts',
                    'semantic_index_with_bursts', 'deep_model_bursts', 'repeat_deep_model_bursts']:
            phase = d['metrics'][key]
            summary[key] = {k: v for k, v in phase.items() if k != 'value'}
            values = phase['value'] if isinstance(phase['value'], list) else phase['value'].get('bursts', [])
            if values:
                field = 'inputToTwoFramesMs' if 'inputToTwoFramesMs' in values[0] else 'inputToPaintMs'
                summary[key]['two_frame_p50_ms'] = statistics.median(v[field] for v in values)
            if isinstance(phase['value'], dict):
                summary[key]['completion'] = {k: v for k, v in phase['value'].items() if k != 'bursts'}
        if 'retained_first_query' in d['metrics']:
            summary['retained_first_query'] = d['metrics']['retained_first_query']
            summary['retained_quiet'] = d['metrics']['retained_quiet']
        summary['bridge_totals'] = d['metrics']['bridge_totals']
        result['desktop'].append(summary)

    result['aggregate'] = {}
    for variant in ['baseline', 'candidate']:
        runs = [r for r in result['desktop'] if r['variant'] == variant]
        result['aggregate'][variant] = {phase: {
            field: statistics.median(r['phases'][phase][field] for r in runs)
            for field in ['cpu_s', 'cpu_percent_one_core', 'interrupt_wakeups', 'footprint_median_bytes', 'footprint_peak_bytes', 'rss_last_bytes']
        } for phase in phases}
    result['original_target_met'] = False
    result['checks']['desktop'] = compact_ui(read('memory-responsiveness-checks.json'))
    result['checks']['search'] = read('memory-search-parity.json')
    result['checks']['builds'] = read('memory-builds.json')
    for filename in ['model-metadata.json', 'real-profile-open-attempt.json', 'real-profile-file-metadata.json', 'source-commit.json']:
        result[filename[:-5]] = read('memory-runtime-01/'+filename)
    result['scaled_cache'] = read('memory-scaled-cache.json')
    result['residency_experiment_status'] = 'Rejected: delayed first query after quiet retention; not enabled in final production source.'
    result['retention_native'] = []
    for config in ['baseline', 'no-residency']:
        d = read('memory-retention-'+config+'-1/result.json')
        assert d['completed'] and not d['owned_survivors']
        result['retention_native'].append({k: v for k, v in d.items() if k not in ['samples', 'props']})
    result['final_production'] = []
    for variant, number in [('baseline', 2), ('candidate', 1), ('candidate', 2), ('baseline', 3)]:
        name = f'memory-cpu-desktop-{variant}-{number}'
        d = read(name+'/result.json')
        assert d['completed'] and not read(name+'/cleanup.json')['owned_survivors']
        assert d['metrics']['vectors_sha256'] == '27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c'
        result['final_production'].append(desktop_record(d, name, variant))
    result['final_aggregate'] = {}
    for variant in ['baseline', 'candidate']:
        runs = [r for r in result['final_production'] if r['variant'] == variant]
        result['final_aggregate'][variant] = {phase: {
            field: statistics.median(r['phases'][phase][field] for r in runs)
            for field in ['cpu_s', 'cpu_percent_one_core', 'interrupt_wakeups', 'footprint_median_bytes', 'footprint_peak_bytes', 'rss_last_bytes']
        } for phase in phases}
    result['final_checks'] = {k: read(v) for k, v in {
        'builds':'memory-cpu-builds.json', 'ui':'memory-cpu-responsiveness-checks.json',
        'visible_retention':'memory-cpu-retention-visible.json', 'search':'memory-cpu-search-parity.json'}.items()}
    assert result['final_checks']['ui']['completed'] and result['final_checks']['visible_retention']['completed']
    result['final_models'] = []
    result['prototype_models'] = []
    for variant in ['q4', 'q8']:
        for threads in [2, 4, 8]:
            ref = read(f'memory-mode-{variant}-t{threads}-baseline-1/result.json')
            for kind, prefix in [('final_models','memory-poll-mode'), ('prototype_models','memory-prototype-mode')]:
                d = read(f'{prefix}-{variant}-t{threads}-1/result.json')
                assert d['completed'] and not d['owned_survivors']
                same_payloads(d, ref)
                result[kind].append(model_record(d))
    result['prototype_build'] = read('memory-llama-prototype-2/result.json')
    result['prototype_libraries'] = read('memory-llama-libraries/audit.json')
    result['prototype_full'] = []
    for variant, refname in [('q4','memory-model-baseline-2'), ('q8','memory-q8-full-baseline-1')]:
        name = 'memory-prototype-full-'+variant+'-1'
        d = read(name+'/result.json')
        assert d['completed'] and not d['owned_survivors']
        same_payloads(d, read(refname+'/result.json'))
        assert 'memory-llama-prototype-2/build/' not in Path(d['vmmap']['path']).read_text(), 'new ggml kernel library unexpectedly loaded'
        result['prototype_full'].append(model_record(d))
    control = read('memory-prototype-control-q4-1/result.json')
    same_payloads(control, read('memory-mode-q4-t8-baseline-1/result.json'))
    result['prototype_control'] = model_record(control)
    same_payloads(read('memory-poll-full-q8-1/result.json'), read('memory-q8-full-baseline-1/result.json'))
    same_payloads(read('memory-model-poll0-1/result.json'), read('memory-model-baseline-2/result.json'))
    name = 'memory-cpu-desktop-prototype-2'
    d = read(name+'/result.json')
    assert d['completed'] and not read(name+'/cleanup.json')['owned_survivors']
    assert d['metrics']['vectors_sha256'] == '27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c'
    result['prototype_desktop'] = desktop_record(d, name, 'experimental-runtime')
    result['failed_desktop_attempt'] = compact_failure(read('memory-cpu-desktop-baseline-1/result.json'))
    assert not read('memory-cpu-desktop-baseline-1/cleanup.json')['owned_survivors']
    d = read('memory-cpu-desktop-prototype-1/result.json')
    result['failed_prototype_desktop'] = compact_failure(d)
    result['failed_prototype_desktop']['visibility_during_failed_burst'] = sorted({s['main_on_screen'] for s in d['samples'] if s['phase'] == 'simple_bursts'})
    assert not read('memory-cpu-desktop-prototype-1/cleanup.json')['owned_survivors']
    result['prototype_native_parity'] = read('memory-prototype-parity.json')
    assert result['prototype_native_parity']['completed']
    result['final_checks']['ui'] = compact_ui(result['final_checks']['ui'])
    result['final_checks']['visible_retention'] = compact_retention(result['final_checks']['visible_retention'])
    result['prototype_native_parity'] = compact_retention(result['prototype_native_parity'])
    result['delivery_verification'] = read('memory-delivery-verification.json')
    assert result['delivery_verification']['complete'] and not result['delivery_verification']['owned_survivors']
    for filename in ['memory-red-supervisor.log','memory-red-poll-policy.log','memory-cpu-release-tests.log',
                     'memory-real-extension-worker.log','memory-cpu-typecheck.log','memory-cpu-vitest.log',
                     'memory-cpu-frontend.log','memory-llama-prototype-build-1.log','memory-rejected-residency.patch']:
        file = root/filename
        evidence[filename] = dict(path=str(file), sha256=sha(file))
    a.output.write_text(json.dumps(result, indent=2)+'\n')


if __name__ == '__main__':
    main()
