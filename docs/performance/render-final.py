#!/usr/bin/env python3
"""Theme-aware standalone comparison HTML, generated only from final receipts."""
import argparse
import html
import json
from pathlib import Path

p = argparse.ArgumentParser()
p.add_argument('--input', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
r = json.loads(a.input.read_text())
rows = [(v['name'], v['p50_speedup']) for v in r['native']['summary']]
for name, phase in [('Simple response', 'simple_bursts'), ('Deep warm response', 'deep_model_bursts')]:
    rows.append((name, r['ui'][phase]['responseMs_p50']['ratio']))
rows += [('Durable indexing wall', r['indexing']['actual_complete_s']['ratio']),
         ('Durable indexing CPU', r['indexing']['durableCpuSeconds']['ratio']),
         ('Retained family footprint', r['memory']['active_model_idle']['footprint_median_bytes']['ratio']),
         ('Repeated family footprint', r['memory']['post_repeat_idle']['footprint_median_bytes']['ratio']),
         ('CJK family footprint (one pair)', r['longChunksSinglePair']['active_model_idle']['footprint_median_bytes']['ratio'])]
width = 960
left = 275
scale = 580 / max(6, max(v for _, v in rows))
height = 54 + len(rows) * 32
svg = [f'<svg viewBox="0 0 {width} {height}" role="img" aria-label="Original beta.17 divided by final, separately for each workload. Five times target shown as a vertical line.">']
for tick in [0, 1, 2, 3, 4, 5]:
    x = left + tick * scale
    svg.append(f'<line x1="{x}" y1="24" x2="{x}" y2="{height-5}" class="grid"/><text x="{x}" y="15" text-anchor="middle" class="muted">{tick}×</text>')
target = left + 5 * scale
svg.append(f'<line x1="{target}" y1="24" x2="{target}" y2="{height-5}" class="target"/>')
for i, (name, ratio) in enumerate(rows):
    y = 35 + i * 32
    svg.append(f'<text x="0" y="{y+13}">{html.escape(name)}</text><rect x="{left}" y="{y}" width="{max(1,ratio*scale)}" height="19" class="{"pass" if ratio>=5 else "bar"}"/><text x="{left+ratio*scale+8}" y="{y+14}">{ratio:.2f}×</text>')
svg.append('</svg>')
memory = []
for name, phase in [('Initial hidden', 'hidden_idle'), ('Model retained', 'active_model_idle'), ('After repeated deep queries', 'post_repeat_idle')]:
    v = r['memory'][phase]
    f = v['footprint_median_bytes']
    rss = v['rss_last_bytes']
    memory.append(f'<tr><th>{name}</th><td>{f["original"]["median"]/1048576:.1f} → {f["final"]["median"]/1048576:.1f}</td><td>{rss["original"]["median"]/1048576:.1f} → {rss["final"]["median"]/1048576:.1f}</td><td>{f["ratio"]:.2f}× · {"PASS" if f["target5xPassed"] else "MISS"}</td></tr>')
document = '''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Original beta.17 → final performance comparison</title>
<style>
:root{color-scheme:light dark;--ink:var(--color-text-primary,light-dark(#242424,#e6e6e6));--muted:var(--color-text-secondary,light-dark(#656565,#aaa));--line:var(--color-border-secondary,light-dark(#dedede,#444));--bar:var(--color-text-secondary,light-dark(#666,#aaa));--pass:var(--color-text-success,light-dark(#236e54,#73c2a1));--target:var(--color-text-warning,light-dark(#9c5f21,#d5a367))}
*{box-sizing:border-box}body{margin:0;color:var(--ink);font:14px/1.55 system-ui,sans-serif}h1{font-size:23px;line-height:1.2;margin:0 0 10px}p{max-width:90ch;margin:8px 0 16px}.muted{color:var(--muted);fill:var(--muted)}svg{display:block;width:100%;max-width:1100px;font-size:13px;fill:var(--ink)}.grid{stroke:var(--line);stroke-width:.65}.target{stroke:var(--target);stroke-width:2;stroke-dasharray:5 4}.bar{fill:var(--bar)}.pass{fill:var(--pass)}table{border-collapse:collapse;width:100%;max-width:1100px;font-variant-numeric:tabular-nums}th,td{text-align:left;padding:10px 10px 10px 0;border-bottom:1px solid var(--line)}thead th{font-weight:500;color:var(--muted)}.scroll{overflow-x:auto}a{color:inherit}
</style><h1>Original beta.17 → final</h1>
<p><strong>Uniform 5× speed and one-fifth RAM: not achieved.</strong> Each bar is one independent original/final ratio. Dashed line: 5× target. Ratios from prior rounds are never multiplied.</p>
<p class="muted">Same local release toolchain; original production source 9ff3663 plus measurement-only instrumentation. macOS arm64 synthetic workloads. Ordinary desktop cohort: AB/BA, two processes per build; CJK: one pair. Medians of process summaries, not confidence intervals.</p>
''' + ''.join(svg) + '''<div class="scroll"><table><thead><tr><th>Matched milestone</th><th>Summed footprint MiB, original → final</th><th>Settled summed RSS MiB, original → final</th><th>Footprint target</th></tr></thead><tbody>''' + ''.join(memory) + '''</tbody></table></div>
<p class="muted">Footprint is a sum of physical-footprint ledgers, not deduplicated RAM. RSS can count shared pages repeatedly. Two animation frames measure render opportunities, not pixel paint. OS/shader caches and unrelated desktop activity are uncontrolled; no builds/tests overlapped timings.</p>
<p>Per-workload p50/p95, CPU, ranges, exact parity, ownership and hashes: <a href="beta17-final-results.md">report</a> · <a href="beta17-final-results.json">machine-readable evidence</a>.</p></html>'''
a.output.write_text(document)
print(a.output)
