#!/usr/bin/env python3
"""Opus pass 1: summarize broad-full-quiet.py runs (control vs candidate).

An idle window is *input-contaminated* when any of its samples reports a system
HID idle time shorter than the time elapsed since the window started, i.e. a
physical input occurred during the window (anywhere on the machine). Such
windows are reported but excluded from quiet cohorts; nothing is deleted.
Per-role CPU/wakeups use the same (PID,start) accounting as desktop.summarize.
WebContent `WebKit Malloc` resident-minus-dirty (clean/reusable residency) is
read from the harness's own post-window vmmap receipts.
"""
import argparse
import json
import math
from pathlib import Path
import re
import statistics

IDLE = ['hidden_idle', 'post_search_idle', 'active_model_idle', 'post_repeat_idle']
BURSTS = ['simple_bursts', 'simple_broad_bursts', 'deep_lexical_bursts', 'deep_lexical_broad_bursts',
          'deep_model_bursts', 'deep_model_broad_bursts', 'repeat_deep_model_bursts']
MIB = 2**20


def pct(values, p):
    return sorted(values)[math.ceil(len(values)*p/100)-1]


def agg(values):
    return dict(median=statistics.median(values), range=[min(values), max(values)], values=values) if values else None


def window(d, phase):
    return [s for s in d['samples'] if s['phase'] == phase]


def contaminated(samples):
    t0 = samples[0]['t']
    hits = [round(s['t']-t0, 2) for s in samples if s.get('hid_idle_s') is not None and s['hid_idle_s'] < s['t']-t0+.05]
    return hits


def roles(samples):
    ids = {}
    for s in samples:
        for p in s['processes']: ids.setdefault((p['pid'], p['start']), []).append(p)
    initial = {(p['pid'], p['start']) for p in samples[0]['processes']}
    out = {}
    for key, v in ids.items():
        o = out.setdefault(v[0]['role'], dict(cpu_s=0., wakeups=0))
        o['cpu_s'] += v[-1]['cpu_s']-(v[0]['cpu_s'] if key in initial else 0)
        o['wakeups'] += v[-1]['wakeups']-(v[0]['wakeups'] if key in initial else 0)
    for role, o in out.items():
        o['footprint_median_mib'] = statistics.median(sum(p['footprint'] for p in s['processes'] if p['role'] == role) for s in samples)/MIB
        o['rss_last_mib'] = sum(p['rss'] for p in samples[-1]['processes'] if p['role'] == role)/MIB
    return out


def host_threads(samples):
    a, b = samples[0].get('host_threads') or {}, samples[-1].get('host_threads') or {}
    out = {}
    for tid, (name, cpu) in b.items():
        out[name or 'unnamed'] = out.get(name or 'unnamed', 0.)+cpu-(a.get(tid, [None, 0.])[1] if tid in a else 0.)
    return {k: round(v*1000, 2) for k, v in sorted(out.items(), key=lambda x: -x[1]) if v > 0}


def webkit_malloc(path):
    if not path.exists(): return None
    for line in path.read_text().splitlines():
        m = re.match(r'WebKit Malloc\s+(\S+)\s+(\S+)\s+(\S+)', line)
        if m:
            size = lambda v: float(v[:-1])*{'K': 1/1024, 'M': 1, 'G': 1024}[v[-1]] if v[-1] in 'KMG' else float(v)/MIB
            resident, dirty = size(m.group(2)), size(m.group(3))
            return dict(resident_mib=resident, dirty_mib=dirty, clean_resident_mib=resident-dirty)
    return None


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--root', type=Path, required=True)
    p.add_argument('--runs', nargs='+', required=True, help='label=dir, label in control|candidate')
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    runs = []
    for item in a.runs:
        label, name = item.split('=')
        d = json.loads((a.root/name/'result.json').read_text())
        runs.append((label, name, d))
    per_run = {}
    for label, name, d in runs:
        r = per_run[name] = dict(label=label, completed=d['completed'], survivors=len(d['survivors']), binary_sha256=d['binary_sha256'], idle={}, bursts={})
        if not d['completed']:
            r['error'] = d.get('error'); continue
        for phase in IDLE:
            S = window(d, phase)
            m = d['metrics'][phase]
            r['idle'][phase] = dict(input_at_s=contaminated(S), cpu_s=m['cpu_s'], wakeups=m['interrupt_wakeups'],
                                    footprint_median_mib=m['footprint_median_bytes']/MIB, rss_last_mib=m['rss_last_bytes']/MIB,
                                    roles=roles(S), host_threads_ms=host_threads(S),
                                    webcontent_webkit_malloc=webkit_malloc(a.root/name/f'{phase}-com.apple.WebKit.WebContent-vmmap.txt'))
        for case in BURSTS:
            m = d['metrics'][case]
            r['bursts'][case] = dict(cpu_s=m['cpu_s'], response_p50_ms=pct([v['responseMs'] for v in m['value']], 50),
                                     response_p95_ms=pct([v['responseMs'] for v in m['value']], 95),
                                     frame_p50_ms=pct([v['inputToTwoFramesMs'] for v in m['value']], 50),
                                     frame_p95_ms=pct([v['inputToTwoFramesMs'] for v in m['value']], 95),
                                     dispatched=sum(v['dispatched'] for v in m['value']))
        r['durable_index_s'] = d['metrics']['semantic_index_with_bursts']['value']['actual_complete_s']
        r['durable_index_family_cpu_s'] = d['metrics']['semantic_index_with_bursts']['value']['durable_family']['cpu_s']
        r['vectors_sha256'] = d['metrics']['vectors_sha256']
    cohorts = {}
    for label in ['control', 'candidate']:
        done = [(n, per_run[n]) for l, n, d in runs if l == label and per_run[n]['completed']]
        c = cohorts[label] = dict(runs=[n for n, _ in done], idle={}, bursts={})
        for phase in IDLE:
            quiet = [r['idle'][phase] for _, r in done if not r['idle'][phase]['input_at_s']]
            c['idle'][phase] = dict(quiet_windows=len(quiet), contaminated_windows=len(done)-len(quiet),
                                    **{k: agg([q[k] for q in quiet]) for k in ['cpu_s', 'wakeups', 'footprint_median_mib', 'rss_last_mib']},
                                    host_cpu_s=agg([q['roles']['host']['cpu_s'] for q in quiet]),
                                    host_wakeups=agg([q['roles']['host']['wakeups'] for q in quiet]),
                                    webcontent_rss_last_mib=agg([q['roles'].get('com.apple.WebKit.WebContent', {}).get('rss_last_mib', 0) for q in quiet]),
                                    webcontent_footprint_median_mib=agg([q['roles'].get('com.apple.WebKit.WebContent', {}).get('footprint_median_mib', 0) for q in quiet]))
        for case in BURSTS:
            c['bursts'][case] = {k: agg([r['bursts'][case][k] for _, r in done]) for k in ['cpu_s', 'response_p50_ms', 'response_p95_ms', 'frame_p50_ms', 'frame_p95_ms', 'dispatched']}
        c['durable_index_s'] = agg([r['durable_index_s'] for _, r in done])
        c['durable_index_family_cpu_s'] = agg([r['durable_index_family_cpu_s'] for _, r in done])
    a.output.write_text(json.dumps(dict(per_run=per_run, cohorts=cohorts), indent=2)+'\n')
    for name, r in per_run.items():
        if not r['completed']: print(name, 'INCOMPLETE', r.get('error')); continue
        print(name, r['label'], ' '.join(f"{ph[:6]}:{'IN' if v['input_at_s'] else 'q'} {v['cpu_s']:.3f}/{v['roles']['host']['cpu_s']:.3f}h/{v['roles']['host']['wakeups']}w" for ph, v in r['idle'].items()))


if __name__ == '__main__': main()
