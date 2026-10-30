#!/usr/bin/env python3
"""Build hook: verify cached pin, or reproducibly assemble the local-only payload.

Never access signing credentials or silently change a catalog. Distribution
packages must be approved/staged before the release build; this hook cannot sign.
"""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import uuid

ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('stage',ROOT/'scripts/stage-embedding-runtime.py')
stage=importlib.util.module_from_spec(spec);spec.loader.exec_module(stage)


def main():
    pin=json.loads((ROOT/'src-tauri/runtime/embedding-macos-arm64.json').read_text())
    destination=ROOT/'build/embedding-runtime/resources/embedding-runtime.zip'
    if destination.exists():
        # Wrong existing bytes fail closed; never silently replace a staged input.
        stage.verify(destination,pin)
        print('Embedding runtime matches committed pin')
        return
    if pin.get('distribution')!='local-ad-hoc':
        raise RuntimeError('Stage the approved signed runtime ZIP before this distribution build; automatic signing is forbidden')
    output=destination.parent.parent/('assembly-'+uuid.uuid4().hex)
    output.mkdir(parents=True)
    package=output/'package'
    with (output/'build.log').open('x') as log:
        subprocess.run([sys.executable,str(ROOT/'scripts/build-embedding-runtime.py'),'--output',str(package)],
            stdout=log,stderr=subprocess.STDOUT,check=True)
    subprocess.run([sys.executable,str(ROOT/'scripts/stage-embedding-runtime.py'),str(package/'embedding-runtime.zip')],check=True)
    print('Auditable source/package build:',output)


if __name__=='__main__':main()
