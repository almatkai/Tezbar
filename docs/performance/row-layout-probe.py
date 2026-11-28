#!/usr/bin/env python3
"""Diagnostic ONLY: row content-visibility in the actual frozen WKWebView.

The 36px unremembered intrinsic size is a corpus-only probe, not a production
height contract. All rows remain in DOM and the lean final-reply guard is intact.
No node/reply arrays are retained by this probe. Use new --output names.
"""
import importlib.util
import json
from pathlib import Path
import sys

spec = importlib.util.spec_from_file_location('layout_probe', Path(__file__).with_name('layout-probe.py'))
p = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)
r = p.r

SELECTOR = '.relative:has(> .glide-highlight--accent) > ul > li'
INSTALL = r'''
const selector=SELECTOR;
const state={events:0,skipped:0,unskipped:0,
 supportsAuto:CSS.supports('content-visibility','auto'),
 supportsIntrinsic:CSS.supports('contain-intrinsic-block-size','auto 36px'),
 userAgent:navigator.userAgent};
window.__rowLayoutProbe=state;
document.addEventListener('contentvisibilityautostatechange',event=>{
 if(!event.target.matches(selector))return;
 state.events++;state[event.skipped?'skipped':'unskipped']++;
},true);
if(ENABLE){const style=document.createElement('style');
 style.textContent=selector+'{content-visibility:auto;contain-intrinsic-block-size:auto 36px}';
 document.head.append(style);}
return state;
'''


def main():
    enable = '--skip-rows' in sys.argv
    if enable: sys.argv.remove('--skip-rows')
    short = '--short' in sys.argv
    if short: sys.argv.remove('--short')
    sample = '--sample' in sys.argv
    if sample: sys.argv.remove('--sample')
    original_packaged = r.packaged
    def packaged(app, root):
        d = original_packaged(app, root)
        js = d.js
        def probe(script):
            if script == r.LEAN_INSTALL:
                js(INSTALL.replace('SELECTOR', json.dumps(SELECTOR)).replace('ENABLE', str(enable).lower()))
                js(p.INSTALL)
            if 'dispatched:s.calls.length' in script:
                script = script.replace('maxActive:s.maxActive', '''maxActive:s.maxActive,
                    diagnosticOnly:true,layoutProbe:window.__layoutProbe,
                    rowLayoutProbe:{...window.__rowLayoutProbe,
                     rows:document.querySelectorAll('''+json.dumps(SELECTOR)+''').length}''')
            return js(script)
        d.js = probe
        return d
    r.packaged = packaged
    if sample:
        if short: raise ValueError('--sample needs the extended diagnostic')
        p.output = Path(sys.argv[sys.argv.index('--output') + 1])
        r.b.burst = p.burst
    elif not short:
        original_burst = r.b.burst
        r.b.burst = lambda *args, **kwargs: [original_burst(*args, **kwargs) for _ in range(16)]
    try:
        r.main()
    finally:
        receipts = []
        for proc, log, receipt in p.samplers:
            receipt['exit'] = proc.wait(timeout=30)
            log.close()
            receipts.append(receipt)
        if sample and p.output.exists():
            (p.output/'sample-receipts.json').write_text(json.dumps(receipts, indent=2)+'\n')


if __name__ == '__main__': main()
