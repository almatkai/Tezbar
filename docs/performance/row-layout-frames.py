#!/usr/bin/env python3
"""Diagnostic actual WKWebView first-frame highlight/scroll geometry.

No performance or pixel-paint claim: bounds reads deliberately flush layout.
Only synthetic owned app input/result DOM is inspected; no desktop screenshots.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import tempfile
import time

spec = importlib.util.spec_from_file_location('renderer_loop', Path(__file__).with_name('renderer-loop.py'))
r = importlib.util.module_from_spec(spec)
spec.loader.exec_module(r)

FRAMES = r'''
const input=document.querySelector('#command-input');
const frames=[];
window.__rowFrameEvents=[];
function capture(timestamp){
 window.__rowReadingProbe=true;
 try {
 const button=document.querySelector('button.list-row[data-tone="accent"][data-selected="true"]');
 if(!button)return {empty:true};
 const row=button.parentElement, ul=row.parentElement, wrapper=ul.parentElement;
 const highlight=wrapper.querySelector('.glide-highlight');
 const rect=row.getBoundingClientRect(), bounds=wrapper.getBoundingClientRect();
 const top=row.offsetTop,height=row.offsetHeight;
 const target=highlight.style.transform.match(/translateY\(([-.\d]+)px\)/);
 const actual=highlight.getBoundingClientRect();
 return {timestamp,now:performance.now(),row:button.textContent, index:Array.from(ul.children).indexOf(row),rows:ul.children.length,
   top,height,highlightTop:target?Number(target[1]):null,
   highlightHeight:parseFloat(highlight.style.height),opacity:highlight.style.opacity,
   rowBounds:{y:rect.y,height:rect.height},highlightBounds:{y:actual.y,height:actual.height},
   scrollTop:wrapper.scrollTop,visible:rect.bottom>bounds.top&&rect.top<bounds.bottom,
   focus:document.activeElement===input};
 } finally {window.__rowReadingProbe=false;}
}
ACTION;
for(let i=0;i<20;i++){const timestamp=await new Promise(requestAnimationFrame);frames.push(capture(timestamp));}
return {frames,events:window.__rowFrameEvents};
'''

INSTALL = r'''
window.__rowFrameEvents=[];
const raf=window.requestAnimationFrame;
window.requestAnimationFrame=function(callback){return raf.call(this,timestamp=>{
 window.__rowCurrentRAF=timestamp;
 try{return callback(timestamp);}finally{window.__rowCurrentRAF=null;}
});};
const offset=Object.getOwnPropertyDescriptor(HTMLElement.prototype,'offsetTop');
Object.defineProperty(HTMLElement.prototype,'offsetTop',{...offset,get(){
 const value=offset.get.call(this);
 if(this.tagName==='LI'&&!window.__rowReadingProbe&&window.__rowFrameEvents.length<2000)
   window.__rowFrameEvents.push({kind:'productionGeometry',now:performance.now(),
    timestamp:window.__rowCurrentRAF??null,top:value});
 return value;
}});
return true;
'''


def main():
    p=argparse.ArgumentParser()
    p.add_argument('--app',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True)
    p.add_argument('--extended',action='store_true')
    p.add_argument('--late-raf',action='store_true')
    a=p.parse_args()
    r.require_unlocked_display()
    a.output.mkdir(parents=True,exist_ok=False)
    root=Path(tempfile.mkdtemp(prefix='tb-row-frames-'))
    r.prepare(root)
    if a.extended:
        (root/'home/Documents'/('runtime-fixture-long-'+'wide-title-'*17+'.txt')).write_text('Synthetic long title layout fixture.\n')
    d=r.packaged(a.app.resolve(),root)
    result=dict(app=str(a.app.resolve()),root=str(root),diagnosticOnly=True,steps={},completed=False)
    try:
        d.launch(a.app.resolve()/'Contents/MacOS/tezbar')
        d.sample('initial')
        d.wait(lambda:d.request('core:launcher:status')['status']=='ready')
        if time.monotonic()-d.started<12:time.sleep(12-(time.monotonic()-d.started))
        r.b.f.visible(d)
        d.js(r.LEAN_INSTALL)
        d.js(INSTALL)
        for query in ['runtime','2+2','#ff0000','100 usd to eur','runtime-fixture']:
            result['steps'][query]=d.js(FRAMES.replace('ACTION', 'window.__inputBench.input('+json.dumps(query)+','+json.dumps(query)+');'))
            d.sample('after-'+query)
        for name,action in [
            ('keyboard-last',"for(let i=0;i<95;i++)input.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));"),
            ('keyboard-first',"for(let i=0;i<95;i++)input.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowUp',bubbles:true}));"),
        ]:
            result['steps'][name]=d.js(FRAMES.replace('ACTION',action))
            d.sample('after-'+name)
        if a.late_raf:
            action="""const initial=await new Promise(resolve=>requestAnimationFrame(timestamp=>{
              input.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));
              queueMicrotask(()=>resolve({...capture(timestamp),stage:'input-committed-in-raf'}));
            }));frames.push(initial);"""
            result['steps']['input-committed-in-raf']=d.js(FRAMES.replace('ACTION',action))
            d.sample('after-input-committed-in-raf')
        if a.extended:
            actions=[
                ('native-resize',"await window.tezbar.setLauncherLayout({width:900,height:700});"),
                ('native-zoom',"await window.tezbar.setLauncherLayout({zoom:1.3});"),
                ('long-title',"window.__inputBench.input('runtime-fixture-long','runtime-fixture-long');"),
                ('font-change',"const style=document.createElement('style');style.id='row-fixture-font';style.textContent='.list-row__title{font-family:Georgia;font-size:18px;line-height:1.6}';document.head.append(style);"),
                ('font-query-commit',"window.__inputBench.input('runtime-fixture-long-','runtime-fixture-long-');"),
                ('restore-font',"document.querySelector('#row-fixture-font').remove();window.__inputBench.input('runtime-fixture','runtime-fixture');"),
                ('wheel-suppression',"window.dispatchEvent(new WheelEvent('wheel',{deltaY:80}));input.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));"),
            ]
            for name,action in actions:
                result['steps'][name]=d.js(FRAMES.replace('ACTION',action))
                d.sample('after-'+name)
            d.command('hide')
            d.wait(lambda:not r.b.f.main_on_screen(d.pid),5)
            r.b.f.visible(d)
            result['steps']['reopen']=d.js(FRAMES.replace('ACTION',"window.__inputBench.input('runtime-fixture-long','runtime-fixture-long');"))
            d.sample('after-reopen')
        result['completed']=True
    except Exception as error:
        result['error']=str(error)
        raise
    finally:
        try:
            if d.pid:
                try:d.js("await window.__TAURI_INTERNALS__.invoke('set_suppress_blur_hide',{value:false});return true;")
                finally:d.close()
        finally:
            result['survivors']=r.processes(d.pid) if d.pid else []
            result['samples']=d.samples
            (a.output/'result.json').write_text(json.dumps(result,indent=2)+'\n')


if __name__=='__main__':main()
