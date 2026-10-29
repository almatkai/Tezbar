#!/usr/bin/env python3
"""Repeat fresh-profile opening milestones; run after other desktop fixtures exit.
This does not clear OS/framework caches and is not a cold-install benchmark.
"""
import argparse
import hashlib
import json
from pathlib import Path
import tempfile
from desktop import Desktop, prepare

p = argparse.ArgumentParser()
p.add_argument('--artifacts', type=Path, required=True)
p.add_argument('--model', required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
runs = []
for variant in ['baseline', 'candidate', 'candidate', 'baseline', 'baseline', 'candidate']:
    binary = a.artifacts / f'desktop-{variant}-app'
    root = tempfile.mkdtemp(prefix='tezbar-desktop-')
    prepare(root, a.model)
    d = Desktop(root)
    try:
        d.launch(binary)
        runs.append(dict(variant=variant, root=root,
                         binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                         ready_ms=d.ready_ms, first_ui_ms=d.first_ui_ms,
                         viewport=d.js('return {width:innerWidth,height:innerHeight,pixelRatio:devicePixelRatio};')))
        print(variant, round(d.ready_ms, 1), round(d.first_ui_ms, 1), flush=True)
    finally:
        d.close()
        a.output.write_text(json.dumps(dict(scope='Fresh isolated core profiles; warm OS/test WebKit caches; 200ms readiness polling', runs=runs), indent=2)+'\n')
