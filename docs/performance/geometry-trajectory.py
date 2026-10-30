#!/usr/bin/env python3
"""R11's identical-load trajectory with bounded microtask/DOM counters.

Diagnostics only: not acceptance timing, no retained rows/trace arrays or GC.
"""
import importlib.util
from pathlib import Path

spec = importlib.util.spec_from_file_location('trajectory', Path(__file__).with_name('row-layout-trajectory.py'))
t = importlib.util.module_from_spec(spec)
spec.loader.exec_module(t)
r = t.r

COUNTERS = '''
const queue=window.queueMicrotask;
window.__geometryQueue={scheduled:0,finished:0,pending:0,maxPending:0};
window.queueMicrotask=function(callback){
 const state=window.__geometryQueue;
 state.scheduled++;state.pending++;state.maxPending=Math.max(state.maxPending,state.pending);
 queue.call(window,()=>{try{callback();}finally{state.pending--;state.finished++;}});
};
'''
READ = '''if(!window.__geometryQueue)return {notInstalled:true};return {queue:{...window.__geometryQueue},
 domNodes:document.querySelectorAll('*').length,
 rows:document.querySelectorAll('button.list-row[data-tone="accent"]').length,
 highlights:document.querySelectorAll('.glide-highlight').length,
 bridgeActive:window.__inputBench.active,
 ownsFinalReply:Object.hasOwn(window.__inputBench,'final')};'''


def main():
    r.LEAN_INSTALL = r.LEAN_INSTALL.replace('return true;', COUNTERS + 'return true;')
    packaged = r.packaged
    def instrumented(app, root):
        d = packaged(app, root)
        idle = d.idle
        def quiet(label, seconds):
            value = idle(label, seconds)
            value['geometryOwnership'] = d.js(READ)
            return value
        d.idle = quiet
        return d
    r.packaged = instrumented
    t.main()


if __name__ == '__main__':
    main()
