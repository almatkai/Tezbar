#!/usr/bin/env python3
"""Run the existing visible/memory protocol with an actually packaged app.

The opt-in executable has the same measurement-only harness on both sides.
Clone the complete locally built app before the existing isolated launcher runs.
The ordinary (non-harness) package receives a separate activation check.
"""
import argparse
import importlib.util
from pathlib import Path
import subprocess
import shutil
import sys
from desktop import Desktop


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--app', type=Path, required=True)
    p.add_argument('--model', required=True)
    p.add_argument('--output', required=True)
    p.add_argument('--long-chunks', action='store_true')
    a = p.parse_args()
    app = a.app.resolve(strict=True)
    binary = app / 'Contents/MacOS/tezbar'
    assert b'TEZBAR_BENCH_HOME' in binary.read_bytes()

    class PackagedDesktop(Desktop):
        def launch(self, executable):
            subprocess.run(['cp', '-cR', str(app), str(self.root / 'Tezbar Performance.app')], check=True)
            # The fixture gets the harness's separate bundle identity below.
            # Its copied executable retains its local ad-hoc code signature;
            # the original package's Info.plist seal no longer describes this
            # test wrapper. Keep both original built packages untouched.
            shutil.rmtree(self.root / 'Tezbar Performance.app/Contents/_CodeSignature', ignore_errors=True)
            super().launch(executable)

    spec = importlib.util.spec_from_file_location('memory_desktop', Path(__file__).with_name('memory-desktop.py'))
    memory = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(memory)
    memory.Desktop = PackagedDesktop
    sys.argv = [sys.argv[0], '--binary', str(binary), '--model', a.model, '--output', a.output]
    if a.long_chunks:
        sys.argv.append('--long-chunks')
    memory.main()


if __name__ == '__main__':
    main()
