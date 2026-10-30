#!/usr/bin/env python3
"""Stage only a payload matching the committed catalog; no downloads or pin edits."""
import argparse
import hashlib
import json
from pathlib import Path
import os
import shutil
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def verify(package, pin):
    raw = package.read_bytes()
    if len(raw) != pin['archiveBytes'] or hashlib.sha256(raw).hexdigest() != pin['archiveSha256']:
        raise ValueError('archive does not match committed runtime pin')
    with zipfile.ZipFile(package) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)) or set(names) != set(pin['files']):
            raise ValueError('runtime inventory differs from pin')
        for name in names:
            if '/' in name or '\\' in name or name in ['.', '..']:
                raise ValueError('unsafe runtime entry')
            payload = archive.read(name)
            file = pin['files'][name]
            if len(payload) != file['bytes'] or hashlib.sha256(payload).hexdigest() != file['sha256']:
                raise ValueError('runtime file differs from pin: '+name)


def main():
    p = argparse.ArgumentParser()
    p.add_argument('package', type=Path)
    a = p.parse_args()
    pin = json.loads((ROOT/'src-tauri/runtime/embedding-macos-arm64.json').read_text())
    verify(a.package, pin)
    destination = ROOT/'build/embedding-runtime/resources/embedding-runtime.zip'
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_suffix('.'+str(os.getpid())+'.tmp')
    try:
        shutil.copyfile(a.package, temporary)
        verify(temporary, pin)
        temporary.replace(destination)
    finally:
        temporary.unlink(missing_ok=True)
    print('Staged verified embedded payload:', pin['archiveSha256'])


if __name__ == '__main__':
    main()
