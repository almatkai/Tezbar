#!/usr/bin/env python3
"""R14: unchanged lean full workload plus fair broad-final-query bursts.

Adds four broad simple/lexical/model bursts to both builds before the existing
memory windows. Keeps renderer-full's visibility, fresh reply, two-frame, DOM and
no-retained-reply guards. No profile/model/indexing budget changes.
"""
import importlib.util
from pathlib import Path

spec = importlib.util.spec_from_file_location('renderer_full', Path(__file__).with_name('renderer-full.py'))
full = importlib.util.module_from_spec(spec)
spec.loader.exec_module(full)
workload = full.r.b
original_scenario = workload.scenario
original_measured = workload.measured_bursts


def scenario(desktop, out):
    def measured(d, label, deep=False, repetitions=4):
        value = original_measured(d, label, deep, repetitions)
        if label in ['simple_bursts', 'deep_lexical_bursts', 'deep_model_bursts']:
            broad_label = label.replace('_bursts', '_broad_bursts')
            def run():
                queries = (['!search', '!search indexing', '!search indexing preserves'] if deep
                           else ['ru', 'run', 'runt', 'runtime', 'runtime-fixture'])
                return [workload.burst(d, queries, 2 if i % 2 == 0 else 20, deep)
                        for i in range(4)]
            out[broad_label] = workload.f.w.measure(d, broad_label, run)
        return value
    workload.measured_bursts = measured
    try:
        original_scenario(desktop, out)
    finally:
        workload.measured_bursts = original_measured


workload.scenario = scenario
if __name__ == '__main__':
    full.main()
