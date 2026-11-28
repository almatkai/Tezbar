#!/usr/bin/env python3
"""R9 diagnostic only: bounded counters on actual UI callsites, no retainer claims.

Use the temporary instrumented build. --canvas-cache is a one-variable diagnostic
ablation of repeated identical text/font measurements; exclude all runs from
latency/CPU acceptance. Production scripts and counters are not modified here.
"""
import importlib.util
import sys
from pathlib import Path
spec=importlib.util.spec_from_file_location('renderer_loop',Path(__file__).with_name('renderer-loop.py'))
r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)
cache='--canvas-cache' in sys.argv
if cache:sys.argv.remove('--canvas-cache')
original=r.packaged

def packaged(app,root):
    d=original(app,root);js=d.js
    def probe(script):
        if script==r.LEAN_INSTALL:
            js('''window.__uiStats={};const stats=window.__uiStats;
            const proto=CanvasRenderingContext2D.prototype;const font=Object.getOwnPropertyDescriptor(proto,'font');
            const lastFont=new WeakMap();Object.defineProperty(proto,'font',{...font,set(value){stats.fontAssignments=(stats.fontAssignments||0)+1;if(lastFont.get(this)===value)stats.sameFontAssignments=(stats.sameFontAssignments||0)+1;lastFont.set(this,value);font.set.call(this,value);}});
            const measure=proto.measureText;const lastMeasure=new WeakMap();proto.measureText=function(text){
              const before=performance.now();stats.measureCalls=(stats.measureCalls||0)+1;
              const previous=lastMeasure.get(this);const same=previous&&previous.text===text&&previous.font===this.font;
              if(same)stats.sameMeasures=(stats.sameMeasures||0)+1;
              const value='''+('same?previous.value:' if cache else '')+'''measure.call(this,text);
              lastMeasure.set(this,{text,font:this.font,value});stats.measureMs=(stats.measureMs||0)+performance.now()-before;return value;
            };
            const computed=window.getComputedStyle;window.getComputedStyle=function(node,...args){if(node.id==='command-input')stats.inputStyleReads=(stats.inputStyleReads||0)+1;return computed.call(this,node,...args);};return true;''')
        if 'const result = {' in script:
            script=script.replace('return result;','result.uiCounters={...window.__uiStats}; return result;')
        if 'dispatched:s.calls.length' in script:
            script=script.replace('maxActive:s.maxActive','maxActive:s.maxActive,uiCounters:window.__uiStats,diagnosticOnly:true,canvasCache:'+str(cache).lower())
        return js(script)
    d.js=probe
    return d
r.packaged=packaged
r.main()
