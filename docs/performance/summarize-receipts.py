#!/usr/bin/env python3
"""Opus pass 2: summarize post-search-receipts.py runs (matched control vs candidate).

Pre-registered before the cohort ran:
- window verdict from hid_receipt.classify (2 s pre-window guard, timed receipts);
- host transient = host interrupt wakeups > 550 in the 30 s window (pass-1 rule);
- RAM primary = WebContent summed RSS at the end of post_search_idle; secondary =
  WebContent RSS growth across simple_broad_bursts, its max footprint there, and
  post-window vmmap (footprint, lifetime peak, WebKit Malloc resident/dirty/
  allocated). Exact two-sided permutation p over all label assignments.
Every run is reported; incomplete runs and input windows are never deleted.
"""
import argparse
from itertools import combinations
import importlib.util
import json
import math
from pathlib import Path
import re
import statistics
import sys

sys.path.insert(0, str(Path(__file__).parent))
import hid_receipt

spec = importlib.util.spec_from_file_location('quiet_summary', Path(__file__).with_name('summarize-quiet.py'))
quiet = importlib.util.module_from_spec(spec)
spec.loader.exec_module(quiet)
MIB = 2**20
WC = 'com.apple.WebKit.WebContent'


def size(v):
    return float(v[:-1])*{'K': 1/1024, 'M': 1, 'G': 1024}[v[-1]] if v[-1] in 'KMG' else float(v)/MIB


def vmmap(path):
    if not path.exists(): return None
    t, o = path.read_text(), {}
    for key, pat in [('footprint', r'Physical footprint:\s+(\S+)'), ('peak', r'Physical footprint \(peak\):\s+(\S+)')]:
        m = re.search(pat, t); o[key] = size(m.group(1)) if m else None
    m = re.search(r'^WebKit Malloc\s+\S+\s+(\S+)\s+(\S+)', t, re.M)
    if m: o['wk_resident'], o['wk_dirty'] = size(m.group(1)), size(m.group(2))
    m = re.search(r'^WebKit Malloc_0x\S+\s+\S+\s+\S+\s+\S+\s+\S+\s+(\d+)\s+(\S+)', t, re.M)
    if m: o['wk_allocations'], o['wk_allocated'] = int(m.group(1)), size(m.group(2))
    return o


def role_sum(sample, role, key):
    return sum(p[key] for p in sample['processes'] if p['role'] == role)/MIB


def permutation_p(a, b):
    """Exact two-sided p for difference in means under label exchange."""
    if not a or not b: return None
    pool, n = a+b, len(a)
    obs = abs(statistics.mean(a)-statistics.mean(b))
    total = hits = 0
    for idx in combinations(range(len(pool)), n):
        x = [pool[i] for i in idx]; y = [pool[i] for i in range(len(pool)) if i not in idx]
        total += 1; hits += abs(statistics.mean(x)-statistics.mean(y)) >= obs-1e-12
    return hits/total


def steps(watch, t0, t1):
    out = []
    for a, b in zip(watch, watch[1:]):
        if not (t0 <= b['t'] <= t1): continue
        th = {}
        for k, (name, cpu) in b['threads'].items():
            th[name or 'unnamed'] = th.get(name or 'unnamed', 0.)+(cpu-a['threads'].get(k, [None, 0.])[1])*1000
        total = sum(th.values())
        if total > 3:
            out.append(dict(t=round(b['t']-t0, 2), ms=round(total, 1),
                            owners=sorted(((round(v, 1), k) for k, v in th.items() if v > .7), reverse=True)[:3]))
    return out


def run_summary(path):
    d = json.loads((path/'result.json').read_text())
    o = dict(label=d['label'], completed=d['completed'], binary_sha256=d['binary_sha256'], survivors=len(d['survivors']))
    if not d['completed']:
        o['error'] = d.get('error'); return o
    phase = lambda name: [s for s in d['samples'] if s['phase'] == name]
    S = phase('post_search_idle')
    roles = quiet.roles(S)
    o['window'] = hid_receipt.classify(S)
    # Denser 0.25 s watcher receipts inside the same window, anchored at its first sample.
    o['window']['watch_verdict'] = hid_receipt.classify(S[:1]+[dict(t=w['t'], hid=w['hid']) for w in d['watch']
                                                             if S[0]['t'] <= w['t'] <= S[-1]['t']])['verdict']
    o['host'] = dict(cpu_ms=roles['host']['cpu_s']*1000, wakeups=roles['host']['wakeups'],
                     threads_ms=quiet.host_threads(S), steps_gt3ms=steps(d['watch'], S[0]['t'], S[-1]['t']))
    o['host']['transient'] = o['host']['wakeups'] > 550
    o['family_cpu_ms'] = d['metrics']['post_search_idle']['cpu_s']*1000
    simple, broad = phase('simple_bursts'), phase('simple_broad_bursts')
    o['webcontent'] = dict(
        rss_end_simple=role_sum(simple[-1], WC, 'rss'), rss_end_broad=role_sum(broad[-1], WC, 'rss'),
        rss_growth_broad=role_sum(broad[-1], WC, 'rss')-role_sum(simple[-1], WC, 'rss'),
        fp_max_simple=max(role_sum(s, WC, 'footprint') for s in simple), fp_max_broad=max(role_sum(s, WC, 'footprint') for s in broad),
        rss_post=role_sum(S[-1], WC, 'rss'), fp_post_median=statistics.median(role_sum(s, WC, 'footprint') for s in S),
        vmmap=vmmap(path/f'post_search_idle-{WC}-vmmap.txt'))
    o['host_vmmap'] = vmmap(path/'post_search_idle-host-vmmap.txt')
    bursts = {}
    for case in ['simple_bursts', 'simple_broad_bursts']:
        V = d['metrics'][case]['value']
        bursts[case] = dict(dispatched=sum(v['dispatched'] for v in V), obsolete=sum(v.get('obsoleteCompleted', 0) for v in V),
                            rows=sum(c['count'] for v in V for c in v['calls']), response_p50_ms=quiet.pct([v['responseMs'] for v in V], 50),
                            frame_p50_ms=quiet.pct([v['inputToTwoFramesMs'] for v in V], 50), wall_s=d['metrics'][case]['wall_s'])
    o['bursts'] = bursts
    o['reply_descriptor'] = d['reply_descriptor']
    o['frontend_assets'] = {a['path'] or '/': a['sha256'] for a in d['frontend_fingerprint']['assets']}
    o['covariates'] = dict(load_before=d['load_before'][0], load_after=d['load_after_window'][0], windowserver_cpu_s=d['windowserver_cpu_s'])
    return o


def agg(values):
    values = [v for v in values if v is not None]
    return dict(median=statistics.median(values), range=[min(values), max(values)], values=values) if values else None


METRICS = {
    'host_cpu_ms': lambda r: r['host']['cpu_ms'], 'host_wakeups': lambda r: r['host']['wakeups'],
    'family_cpu_ms': lambda r: r['family_cpu_ms'],
    'wc_rss_post_mib': lambda r: r['webcontent']['rss_post'], 'wc_rss_growth_broad_mib': lambda r: r['webcontent']['rss_growth_broad'],
    'wc_fp_max_broad_mib': lambda r: r['webcontent']['fp_max_broad'], 'wc_fp_post_median_mib': lambda r: r['webcontent']['fp_post_median'],
    'wc_vmmap_peak_mib': lambda r: (r['webcontent']['vmmap'] or {}).get('peak'),
    'wc_vmmap_footprint_mib': lambda r: (r['webcontent']['vmmap'] or {}).get('footprint'),
    'wc_wk_resident_mib': lambda r: (r['webcontent']['vmmap'] or {}).get('wk_resident'),
    'wc_wk_dirty_mib': lambda r: (r['webcontent']['vmmap'] or {}).get('wk_dirty'),
    'wc_wk_allocated_mib': lambda r: (r['webcontent']['vmmap'] or {}).get('wk_allocated'),
}
RAM = [k for k in METRICS if k.startswith('wc_')]


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--root', type=Path, required=True)
    p.add_argument('--runs', nargs='+', required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    per = {n: run_summary(a.root/n) for n in a.runs}
    done = {n: r for n, r in per.items() if r['completed']}
    cohorts = {}
    for label in ['control', 'candidate']:
        rs = [r for r in done.values() if r['label'] == label]
        q = [r for r in rs if r['window']['verdict'] == 'quiet']
        cohorts[label] = dict(completed=len(rs), quiet_windows=len(q), transients_all=sum(r['host']['transient'] for r in rs),
                              transients_quiet=sum(r['host']['transient'] for r in q),
                              all_windows={k: agg([f(r) for r in rs]) for k, f in METRICS.items()},
                              quiet_windows_only={k: agg([f(r) for r in q]) for k, f in METRICS.items() if not k.startswith('wc_')})
    tests = {}
    for k, f in METRICS.items():
        c = [f(r) for r in done.values() if r['label'] == 'control' and f(r) is not None]
        x = [f(r) for r in done.values() if r['label'] == 'candidate' and f(r) is not None]
        tests[k] = dict(all_windows_p=permutation_p(c, x))
        if not k.startswith('wc_'):
            cq = [f(r) for r in done.values() if r['label'] == 'control' and r['window']['verdict'] == 'quiet']
            xq = [f(r) for r in done.values() if r['label'] == 'candidate' and r['window']['verdict'] == 'quiet']
            tests[k]['quiet_p'] = permutation_p(cq, xq)
    identity = dict(frontend_assets_identical=len({json.dumps(r['frontend_assets'], sort_keys=True) for r in done.values()}) == 1,
                    reply_lengths_identical=len({json.dumps([(x['query'], x['rows'], x['json_chars']) for x in r['reply_descriptor']]) for r in done.values()}) == 1,
                    ordered_reply_digests_by_label={l: sorted({json.dumps([x['ordered_sha256'][:12] for x in r['reply_descriptor']]) for r in done.values() if r['label'] == l}) for l in ['control', 'candidate']})
    a.output.write_text(json.dumps(dict(per_run=per, cohorts=cohorts, permutation_tests=tests, identity=identity), indent=2)+'\n')
    for n, r in per.items():
        if not r['completed']: print(n, 'INCOMPLETE', r.get('error')); continue
        w = r['webcontent']
        print(f"{n:16s} {r['window']['verdict'][:9]:9s} m={r['window']['margin_s']:8.1f} host {r['host']['cpu_ms']:6.1f}ms {r['host']['wakeups']:4d}w{' T' if r['host']['transient'] else '  '}"
              f" | WC simple {w['rss_end_simple']:4.0f} broad {w['rss_end_broad']:4.0f} (+{w['rss_growth_broad']:3.0f}) fpmax {w['fp_max_broad']:4.0f} post {w['rss_post']:4.0f}"
              f" | load {r['covariates']['load_before']:.1f} ws {r['covariates']['windowserver_cpu_s']}")
    for k in METRICS:
        print(f"{k:24s} control {cohorts['control']['all_windows'][k] and round(cohorts['control']['all_windows'][k]['median'], 1)}"
              f" candidate {cohorts['candidate']['all_windows'][k] and round(cohorts['candidate']['all_windows'][k]['median'], 1)} p={tests[k]}")
    print(json.dumps(identity))


if __name__ == '__main__': main()
