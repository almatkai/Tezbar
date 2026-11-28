#!/usr/bin/env python3
"""Extract original beta.17 into ignored artifacts; add measurement code only.

No checkout, branch, worktree or installed application is changed. The archived
production source remains the original reference, not an intermediate executor.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile
import io

ROOT = Path(__file__).resolve().parents[2]
BASE = '9ff3663342b48ab01be2e56ab30f6d7123b0ced8'
PROBES = 'd874f1f4bbfe059f6e81f4242864b2c21f1b721e'


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    out = a.output.resolve()
    assert out.is_relative_to(ROOT / 'build/performance-runtime-20261030')
    out.mkdir(exist_ok=False)
    source = out / 'original-source'
    source.mkdir()
    # Exclude checked-in native build outputs, not source inputs. Keep exact
    # Git blobs in the source archive, including the original lockfiles.
    paths = [s for s in git('ls-tree', '-r', '--name-only', BASE).decode().splitlines()
             if '/target/' not in s and not s.endswith('.DS_Store')]
    archive = git('archive', '--format=tar', BASE, '--', *paths)
    (out / 'original-source.tar').write_bytes(archive)
    with tarfile.open(fileobj=io.BytesIO(archive)) as t:
        t.extractall(source)
    original = {p: hashlib.sha256((source / p).read_bytes()).hexdigest()
                for p in paths if (source / p).is_file()}
    patch = git('diff', BASE, PROBES, '--', 'src-tauri/src')
    patch += git('diff', BASE, 'ac14c06', '--', 'src-tauri/src/lib.rs')
    patch += git('diff', BASE, 'ac14c06', '--', 'src-tauri/src/desktop_bench.rs')
    # Cargo's only harness change is the opt-in feature. No new dependencies,
    # runtime package, launcher/semantic optimization or allocator is imported.
    cargo = source / 'src-tauri/Cargo.toml'
    old = cargo.read_text()
    new = old.replace('[build-dependencies]', '[features]\ndesktop-bench = []\n\n[build-dependencies]', 1)
    import difflib
    patch += ''.join(difflib.unified_diff(old.splitlines(True), new.splitlines(True),
                         fromfile='a/src-tauri/Cargo.toml', tofile='b/src-tauri/Cargo.toml')).encode()
    (out / 'measurement-only.patch').write_bytes(patch)
    subprocess.run(['patch', '-p1', '-i', str(out / 'measurement-only.patch')], cwd=source, check=True)
    changed = {p: hashlib.sha256((source / p).read_bytes()).hexdigest()
               for p, sha in original.items() if hashlib.sha256((source / p).read_bytes()).hexdigest() != sha}
    assert set(changed) == {'src-tauri/Cargo.toml', 'src-tauri/src/lib.rs',
        'src-tauri/src/native_core/mod.rs', 'src-tauri/src/native_core/knowledge/search_cache.rs',
        'src-tauri/src/native_core/launcher/db.rs'}
    additions = {str(p.relative_to(source)): hashlib.sha256(p.read_bytes()).hexdigest()
                 for p in source.rglob('*') if p.is_file() and str(p.relative_to(source)) not in original}
    assert set(additions) == {'src-tauri/src/desktop_bench.rs', 'src-tauri/src/native_core/performance.rs'}
    assert (source / 'src-tauri/src/desktop_bench.rs').read_bytes() == (ROOT / 'src-tauri/src/desktop_bench.rs').read_bytes()
    assert (source / 'src-tauri/src/native_core/performance.rs').read_bytes() == (ROOT / 'src-tauri/src/native_core/performance.rs').read_bytes()
    (source / 'node_modules').symlink_to(ROOT / 'node_modules', target_is_directory=True)
    receipt = dict(base=BASE, nativeProbes=PROBES, desktopHarnessSource='ac14c06',
        archiveSha256=hashlib.sha256(archive).hexdigest(), patchSha256=hashlib.sha256(patch).hexdigest(),
        originalFiles=original, changedFiles=changed, addedFiles=additions,
        omitted='tracked native target outputs and .DS_Store only',
        audit='All remaining original blobs verified before build; exactly five changed and two added files; no accepted production optimization imported')
    (out / 'reference-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(source)


if __name__ == '__main__':
    main()
