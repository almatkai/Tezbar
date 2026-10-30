#!/usr/bin/env python3
"""Opus pass 1: unchanged broad-full.py workload plus a physical-input receipt.

R14's hidden idle windows had no guard against physical input elsewhere on the
machine: WebView trusted-event counters only see input delivered to Tezbar.
Each sample here also records the system HID idle time (seconds since the last
physical input, via read-only `ioreg`; never event content) and per-thread
CPU of the owned host (proc_pidinfo, no task suspension). summarize-quiet.py
excludes idle windows in which physical input occurred instead of letting
them enter a cohort. Workload, timers and all other guards are unchanged.
"""
import importlib.util
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).parent))
spec = importlib.util.spec_from_file_location('broad_full', Path(__file__).with_name('broad-full.py'))
bf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bf)
probe_spec = importlib.util.spec_from_file_location('post_search_probe', Path(__file__).with_name('post-search-probe.py'))
probe = importlib.util.module_from_spec(probe_spec)
probe_spec.loader.exec_module(probe)
renderer_loop = bf.full.r
original_packaged = renderer_loop.packaged


def packaged(app, root):
    d = original_packaged(app, root)
    original = d.sample
    def sample(label):
        value = original(label)
        value['hid_idle_s'] = probe.hid_idle_s()
        value['host_threads'] = probe.thread_times(d.pid) if d.pid else {}
        return value
    d.sample = sample
    return d


renderer_loop.packaged = packaged
if __name__ == '__main__':
    bf.full.main()
