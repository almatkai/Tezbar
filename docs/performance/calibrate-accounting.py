#!/usr/bin/env python3
"""Cross-check this host's proc_pid_rusage CPU units against getrusage.

Run only after timed workloads, never concurrently with app measurements.
Raw benchmark files remain immutable even if an accounting scale is corrected.
"""
import argparse
import ctypes
import json
import os
from pathlib import Path
import resource
import time
from desktop import LIB, Usage, TICK_SECONDS

p = argparse.ArgumentParser()
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
assert not a.output.exists()

def raw():
    usage = Usage()
    assert LIB.proc_pid_rusage(os.getpid(), 4, ctypes.byref(usage)) == 0
    return usage.user_time + usage.system_time

def cpu():
    r = resource.getrusage(resource.RUSAGE_SELF)
    return r.ru_utime + r.ru_stime

start_cpu, start_raw = cpu(), raw()
start = time.monotonic()
value = 0
while time.monotonic() - start < .35:
    value = (value + 17) % 100003
end_raw, end_cpu = raw(), cpu()
reference = end_cpu - start_cpu
delta = end_raw - start_raw
methods = dict(nanoseconds=delta / 1e9, mach_ticks=delta * TICK_SECONDS)
selected = min(methods, key=lambda k: abs(methods[k] / reference - 1))
assert abs(methods[selected] / reference - 1) < .05
scale = 1e-9 / TICK_SECONDS if selected == 'nanoseconds' else 1.
r = dict(referenceGetrusageCpuSeconds=reference, procRawDelta=delta,
         machTickSeconds=TICK_SECONDS, interpretations=methods, selected=selected,
         originalHarnessCpuSeconds=methods['mach_ticks'], recordedCpuScaleToSeconds=scale,
         method='Independent .35s self CPU run after all timed app phases; no app or clipboard operation')
a.output.write_text(json.dumps(r, indent=2) + '\n')
print(json.dumps(r))
