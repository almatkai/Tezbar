#!/usr/bin/env python3
"""Assemble a locally signed b11474 memory-fix payload. Never publish or install.

The official server and ALL numerical backends are checksum-pinned upstream
bytes. Only libllama is rebuilt. --build reuses the pinned source recipe;
--library-build accepts its audited, immutable output (not an arbitrary dylib).
The generated catalog is a review candidate; never overwrites the committed pin.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
COMMIT = 'b9acf138a1e28ce1fc23b5a4fc4b12444b50f7ea'
OFFICIAL = 'llama-b11474-bin-macos-arm64.tar.gz'
OFFICIAL_SHA = 'ae7f002f2b477d14fea8ac4d93438684344cd18ec10d2b0ee9d5518b2a74def5'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--library-build', type=Path)
    p.add_argument('--source-archive', type=Path)
    p.add_argument('--cmake', type=Path)
    p.add_argument('--official-archive', type=Path)
    p.add_argument('--signing-identity', help='Only with separate distribution-signing approval; never used automatically')
    a = p.parse_args()
    assert platform.system() == 'Darwin' and platform.machine() == 'arm64', 'only macOS arm64 recipe validated'
    out = a.output.resolve()
    out.mkdir(exist_ok=False)
    libraries = a.library_build
    if libraries is None:
        libraries = out/'library-build'
        cmd = [sys.executable, str(ROOT/'docs/performance/build-llama-memory.py'), '--output', str(libraries)]
        for flag, value in [('--source-archive', a.source_archive), ('--cmake', a.cmake)]:
            if value:
                cmd += [flag, str(value.resolve())]
        subprocess.run(cmd, check=True)
    libraries = libraries.resolve()
    provenance = json.loads((libraries/'result.json').read_text())
    assert provenance['completed'] and provenance['commit'] == COMMIT
    assert provenance['source_archive_sha256'] == '0b7309d385c523c636231c10f808b226166fec13332277406361463ee2dc885d'
    patch = ROOT/'docs/performance/llama-b11474-embedding-output.patch'
    # diff line/context metadata can differ; the exact removed/added source must agree.
    changes = lambda b: [x for x in b.splitlines() if x[:1] in ['+', '-'] and not x.startswith(('+++', '---'))]
    assert changes(patch.read_text()) == changes((libraries/'unused-logits.patch').read_text())
    library = libraries/'libllama-no-logits.dylib'
    assert sha(library) == provenance[library.name]['sha256']
    archive = out/OFFICIAL
    if a.official_archive:
        shutil.copyfile(a.official_archive, archive)
    else:
        url = 'https://github.com/ggml-org/llama.cpp/releases/download/b11474/'+OFFICIAL
        subprocess.run(['curl', '--fail', '--location', '--retry', '2', '--output', str(archive), url], check=True)
    assert archive.stat().st_size == 12011955 and sha(archive) == OFFICIAL_SHA
    stock = out/'official'
    stock.mkdir()
    # Pinned public archive has internal library symlinks. No extraction outside root.
    with tarfile.open(archive) as t:
        for m in t.getmembers():
            target = (stock/m.name).resolve()
            assert target.is_relative_to(stock) and not m.islnk()
            if m.issym():
                assert (target.parent/m.linkname).resolve().is_relative_to(stock)
        t.extractall(stock)
    source = next(stock.rglob('llama-server')).parent
    payload = out/'payload'
    payload.mkdir()
    # Preserve every upstream dylib and alias, including dynamically loaded backends.
    # Dereference internal aliases so the installed payload has no symlinks.
    for f in sorted(source.iterdir()):
        if f.name == 'llama-server' or f.name.endswith('.dylib') or f.name == 'LICENSE' or f.suffix in ['.metal', '.metallib']:
            shutil.copyfile(library if f.name.startswith('libllama.') else f, payload/f.name)
            (payload/f.name).chmod(0o755 if f.name != 'LICENSE' else 0o644)
    distribution = 'local-ad-hoc'
    if a.signing_identity:
        assert a.signing_identity.startswith('Developer ID Application:'), 'distribution requires Developer ID Application'
        for f in sorted(payload.iterdir()):
            if f.name == 'llama-server' or f.suffix == '.dylib':
                subprocess.run(['codesign', '--force', '--options', 'runtime', '--timestamp', '--sign', a.signing_identity, str(f)], check=True)
                subprocess.run(['codesign', '--verify', '--strict', str(f)], check=True)
                details = subprocess.run(['codesign', '-dv', '--verbose=4', str(f)], capture_output=True, text=True, check=True).stderr
                assert 'Authority=Developer ID Application:' in details and 'TeamIdentifier=not set' not in details
        distribution = 'developer-id'
    (payload/'TEZBAR-PROVENANCE.json').write_text(json.dumps(dict(
        upstream_commit=COMMIT, upstream_archive=OFFICIAL, upstream_archive_sha256=OFFICIAL_SHA,
        source_archive_sha256=provenance['source_archive_sha256'], patch_sha256=sha(patch),
        library_sha256=sha(library), signing=('local ad-hoc; not Developer ID/notarized' if not a.signing_identity else 'Developer ID Application; app notarization still required'),
        cmake=provenance['cmake_version'], numerical_backends='unmodified upstream archive bytes'), indent=2)+'\n')
    dependencies = {}
    for f in sorted(payload.iterdir()):
        if f.name == 'llama-server' or f.suffix == '.dylib':
            text = subprocess.check_output(['otool', '-L', str(f)], text=True)
            dependencies[f.name] = text.replace(str(payload), '<payload>')
            for line in text.splitlines()[1:]:
                dep = line.strip().split(' (', 1)[0]
                assert dep.startswith(('@rpath/', '/usr/lib/', '/System/Library/')), dep
                if dep.startswith('@rpath/'):
                    assert (payload/Path(dep).name).exists(), dep
    (out/'dependencies.json').write_text(json.dumps(dependencies, indent=2)+'\n')
    files = {f.name: dict(bytes=f.stat().st_size, sha256=sha(f)) for f in sorted(payload.iterdir())}
    package = out/'embedding-runtime.zip'
    with zipfile.ZipFile(package, 'x', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for f in sorted(payload.iterdir()):
            info = zipfile.ZipInfo(f.name, (2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (0o100755 if f.name == 'llama-server' or f.suffix == '.dylib' else 0o100644) << 16
            z.writestr(info, f.read_bytes(), compresslevel=9)
    catalog = dict(schema=1, distribution=distribution, target='aarch64-apple-darwin', id='b11474-tezbar-mean-no-logits-v1',
        archiveSha256=sha(package), archiveBytes=package.stat().st_size, executable='llama-server',
        upstreamCommit=COMMIT, upstreamArchiveSha256=OFFICIAL_SHA, patchSha256=sha(patch), files=files)
    (out/'catalog.json').write_text(json.dumps(catalog, indent=2)+'\n')
    shutil.copyfile(libraries/'result.json', out/'library-build-provenance.json')
    print(json.dumps(dict(package=str(package), sha256=sha(package), bytes=package.stat().st_size), indent=2))


if __name__ == '__main__':
    main()
