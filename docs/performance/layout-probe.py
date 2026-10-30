#!/usr/bin/env python3
"""Diagnostic ONLY: actual renderer stacks + bounded synchronous layout timers.

Repeats the lean real-input bursts; not an acceptance timing/RAM cohort. No GC,
visual ablation, result suppression, remote inspector or global permissions.
Attach sample only to PID/start-verified members of this synthetic app family.
"""
import ctypes
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

spec = importlib.util.spec_from_file_location('renderer_loop', Path(__file__).with_name('renderer-loop.py'))
r = importlib.util.module_from_spec(spec)
spec.loader.exec_module(r)
output = None
original_packaged, original_burst = r.packaged, r.b.burst
samplers = []
contain = False

INSTALL = r'''
const stats={}; window.__layoutProbe=stats;
function timed(name,fn){const t=performance.now();try{return fn();}finally{
 const ms=performance.now()-t;const s=stats[name]??={count:0,ms:0,max:0,over1ms:0};s.count++;s.ms+=ms;s.max=Math.max(s.max,ms);if(ms>=1)s.over1ms++;
}}
const computed=window.getComputedStyle;
window.getComputedStyle=function(node,...args){
 const style=timed(node.id==='command-input'?'input.getComputedStyle':'other.getComputedStyle',()=>computed.call(this,node,...args));
 if(node.id!=='command-input')return style;
 return new Proxy(style,{get(target,key){return timed('input.style.'+String(key),()=>Reflect.get(target,key,target));}});
};
for(const key of ['offsetTop','offsetHeight']){
 const d=Object.getOwnPropertyDescriptor(HTMLElement.prototype,key);
 Object.defineProperty(HTMLElement.prototype,key,{...d,get(){return timed(this.tagName==='LI'?'row.'+key:'other.'+key,()=>d.get.call(this));}});
}
const scroll=Element.prototype.scrollIntoView;
Element.prototype.scrollIntoView=function(...args){return timed('scrollIntoView',()=>scroll.apply(this,args));};
const measure=CanvasRenderingContext2D.prototype.measureText;
CanvasRenderingContext2D.prototype.measureText=function(...args){return timed('measureText',()=>measure.apply(this,args));};
return true;
'''

def packaged(app, root):
    d=original_packaged(app,root)
    js=d.js
    def probe(script):
        if script==r.LEAN_INSTALL:
            if contain:
                js("const s=document.createElement('style');s.textContent='.relative:has(> .glide-highlight){contain:layout}';document.head.append(s);return true;")
            js(INSTALL)
        if 'dispatched:s.calls.length' in script:
            script=script.replace('maxActive:s.maxActive','maxActive:s.maxActive,layoutProbe:window.__layoutProbe,diagnosticOnly:true')
        return js(script)
    d.js=probe
    return d

started=False

def burst(driver,*args,**kwargs):
    global started
    if not started:
        started=True
        d=driver.d
        for p in r.processes(d.pid):
            if p['role'] not in ['com.apple.WebKit.WebContent','host']:continue
            usage=r.Usage()
            if r.LIB.proc_pid_rusage(p['pid'],4,ctypes.byref(usage))!=0 or usage.proc_start_abstime!=p['start']:
                raise RuntimeError('sample target PID/start changed')
            name=p['role']+'-sample'
            log=(output/(name+'.log')).open('w')
            cmd=['/usr/bin/sample',str(p['pid']),'10','1','-mayDie','-file',str(output/(name+'.txt'))]
            proc=subprocess.Popen(cmd,stdout=log,stderr=subprocess.STDOUT)
            samplers.append((proc,log,dict(command=cmd,target=p)))
    values=[original_burst(driver,*args,**kwargs) for _ in range(16)]
    return values

def main():
    global output, contain
    if '--output' not in sys.argv:
        r.main()  # Delegate --help / required-argument errors to the shared CLI.
        return
    output = Path(sys.argv[sys.argv.index('--output') + 1])
    contain = '--contain-layout' in sys.argv
    if contain: sys.argv.remove('--contain-layout')
    r.packaged=packaged
    r.b.burst=burst
    try:
        r.main()
    finally:
        receipts=[]
        for p,log,receipt in samplers:
            receipt['exit']=p.wait(timeout=30)
            log.close()
            receipts.append(receipt)
        if output.exists():(output/'sample-receipts.json').write_text(json.dumps(receipts,indent=2)+'\n')


if __name__=='__main__': main()
