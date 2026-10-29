#!/usr/bin/env python3
"""Create a self-contained comparison chart from the measured desktop JSON."""
import argparse
import html
import json
from pathlib import Path

p=argparse.ArgumentParser();p.add_argument('results',type=Path);p.add_argument('output',type=Path);a=p.parse_args()
x=json.loads(a.results.read_text())
labels=[('hidden_idle','Hidden idle'),('visible_idle','Visible idle'),('post_features','After features'),('active_model_idle','Model active')]
def chart(field,caption,unit,scale):
    values=[x['summary'][key][field] for key,_ in labels]
    maximum=max(max(v['baseline_runs']+v['candidate_runs'])/scale for v in values)*1.16
    parts=[f'<figure><figcaption>{caption}</figcaption><svg viewBox="0 0 700 270" role="img" aria-label="{caption}">']
    for i,((key,label),value) in enumerate(zip(labels,values)):
        y=24+i*56
        parts.append(f'<text x="0" y="{y+12}" class="label">{label}</text>')
        for j,(variant,color) in enumerate([('baseline','var(--chart-1, #6b8e73)'),('candidate','var(--chart-2, #b58150)')]):
            number=value[variant]/scale; width=number/maximum*440; yy=y+j*17
            low=min(value[variant+'_runs'])/scale/maximum*440
            high=max(value[variant+'_runs'])/scale/maximum*440
            title=html.escape(f'{label}, {variant}: {number:.2f} {unit}; runs '+', '.join(f'{v/scale:.2f}' for v in value[variant+'_runs']))
            parts.append(f'<rect x="118" y="{yy}" width="{width:.3f}" height="10" fill="{color}"><title>{title}</title></rect>')
            parts.append(f'<path d="M {118+low:.3f} {yy+5} H {118+high:.3f}" class="range"/>')
            parts.append(f'<text x="580" y="{yy+9}" class="value">{number:.2f}</text>')
        target=value['baseline']/scale/5/maximum*440+118
        parts.append(f'<path d="M {target:.3f} {y-3} V {y+30}" class="target"><title>One-fifth of baseline: {value["baseline"]/scale/5:.2f} {unit}</title></path>')
    parts.append(f'<text x="118" y="258" class="note">0</text><text x="558" y="258" text-anchor="end" class="note">{maximum:.1f} {unit}</text></svg></figure>')
    return ''.join(parts)
body='''<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><style>
html,body{margin:0;padding:0;background:transparent;color:var(--foreground, #1e293b);font-family:var(--font-sans,system-ui)}
figure{margin:18px 0 8px}figcaption{font-size:14px;font-weight:600;margin-bottom:6px}svg{width:100%;height:270px;overflow:visible}
text{fill:var(--foreground, #1e293b);font-family:var(--font-sans,system-ui)}.label,.value{font-size:12px}.note{font-size:11px;fill:var(--muted-foreground, #64748b)}
.range{stroke:var(--foreground, #1e293b);stroke-width:2;opacity:.65}.target{stroke:var(--foreground, #1e293b);stroke-width:1;stroke-dasharray:2 2;opacity:.5}
.legend{display:flex;gap:18px;flex-wrap:wrap;font-size:12px}.key{display:inline-block;width:14px;height:8px;margin-right:5px}.caption{font-size:12px;line-height:1.5;color:var(--muted-foreground, #64748b);margin:6px 0}
@media(max-width:500px){svg{height:225px}}
</style></head><body><div class="legend"><span><i class="key" style="background:var(--chart-1, #6b8e73)"></i>Baseline</span><span><i class="key" style="background:var(--chart-2, #b58150)"></i>Candidate</span><span>Dashed tick: 5× target</span></div>'''
body+=chart('cpu_percent_one_core','Full desktop idle CPU','% of one core',1)
body+=chart('footprint_median_bytes','Summed physical footprints, including owned helpers and model','MiB',2**20)
body+='''<p class="caption">Two release processes per version; each idle window lasts 30 seconds. Bars show the median of the two run summaries; thin lines show their range, not a confidence interval. Physical footprint totals are pressure-accounting ledgers, not deduplicated resident RAM. RSS, model details, latency and limitations are in the report. The app-wide 5× goals are not established.</p></body></html>'''
a.output.write_text(body)
print(a.output)
