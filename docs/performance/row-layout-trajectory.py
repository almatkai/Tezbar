#!/usr/bin/env python3
"""Diagnostic allocation trajectory, separate from R8 acceptance workload.

Four identical eight-burst blocks, each followed by 15s genuinely hidden quiet.
Lean replies remain temporarily owned only during the fresh ordered DOM guard.
No GC, model, repeated snapshots of full replies, or diagnostic getter timers.
"""
import importlib.util
from pathlib import Path

spec=importlib.util.spec_from_file_location('renderer_loop',Path(__file__).with_name('renderer-loop.py'))
r=importlib.util.module_from_spec(spec)
spec.loader.exec_module(r)


def main():
    measure=r.b.f.w.measure
    def trajectory(d,label,work,*args,**kwargs):
        if label!='simple_bursts':return measure(d,label,work,*args,**kwargs)
        blocks=[]
        for i in range(4):
            burst=measure(d,'trajectory_work_'+str(i),work,*args,**kwargs)
            d.command('hide')
            d.wait(lambda:not r.main_on_screen(d.pid),5)
            quiet=d.idle('trajectory_quiet_'+str(i),15)
            assert all(not s['main_on_screen'] for s in d.samples if s['phase']=='trajectory_quiet_'+str(i))
            blocks.append(dict(work=burst,quiet=quiet))
            r.b.f.visible(d)
        return dict(diagnosticOnly=True,blocks=blocks)
    r.b.f.w.measure=trajectory
    r.main()


if __name__=='__main__':main()
