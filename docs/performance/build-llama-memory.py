#!/usr/bin/env python3
"""Build isolated pinned llama control + unused-logits libraries, not an install.

Downloads source and a local CMake wheel only. No model downloads, system package
installation, profile edits or runtime catalog changes. Run outside timed work.
Only the libllama dylib is substituted in experimental runtime clones, retaining
the official server, common/ggml/Metal binaries and numerical kernels.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request

COMMIT = 'b9acf138a1e28ce1fc23b5a4fc4b12444b50f7ea'
ARCHIVE_SHA256 = '0b7309d385c523c636231c10f808b226166fec13332277406361463ee2dc885d'
CMAKE_WHEEL = 'cmake-3.31.6-py3-none-macosx_10_10_universal2.whl'
CMAKE_URL = 'https://files.pythonhosted.org/packages/7f/09/516b0d709672bc430eb13278f0316acd34869269447744f5d136daeef689/'+CMAKE_WHEEL
CMAKE_SHA256 = 'da9d4fd9abd571fd016ddb27da0428b10277010b23bb21e3678f8b9e96e1686e'


def materialize_library(source, target):
    """Use cloned stock ggml backends, never CMake's newly built kernel libs."""
    shutil.copy2(source, target)
    dependencies = subprocess.check_output(['otool', '-L', str(target)], text=True)
    for line in dependencies.splitlines()[1:]:
        dependency = line.strip().split(' (', 1)[0]
        if Path(dependency).name.startswith('libggml') and not dependency.startswith('@rpath/'):
            subprocess.run(['install_name_tool', '-change', dependency,
                            '@rpath/'+Path(dependency).name, str(target)], check=True)
    commands = subprocess.check_output(['otool', '-l', str(target)], text=True)
    rpaths = re.findall(r'cmd LC_RPATH\s+cmdsize \d+\s+path (.*?) \(offset', commands)
    for rpath in dict.fromkeys(rpaths):
        subprocess.run(['install_name_tool', '-delete_rpath', rpath, str(target)], check=True)
    subprocess.run(['install_name_tool', '-add_rpath', '@loader_path', '-id',
                    '@rpath/libllama.0.6.0.dylib', str(target)], check=True)
    subprocess.run(['codesign', '--force', '--sign', '-', '--timestamp=none', str(target)], check=True)
    return dict(path=str(target), sha256=hashlib.sha256(target.read_bytes()).hexdigest(),
                dependencies=subprocess.check_output(['otool', '-L', str(target)], text=True),
                load_commands=subprocess.check_output(['otool', '-l', str(target)], text=True))


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--cmake', type=Path, help='Reuse an already isolated CMake executable')
    p.add_argument('--source-archive', type=Path, help='Reuse an existing pinned source archive on a retry')
    a = p.parse_args()
    out = a.output.resolve()
    roots = [Path(tempfile.gettempdir()).resolve(), Path('/tmp').resolve(),
             Path(__file__).resolve().parents[2]/'build/embedding-runtime']
    if not any(root in out.parents for root in roots):
        raise ValueError('prototype builds belong in an isolated temporary directory')
    out.mkdir(exist_ok=False)
    result = dict(commit=COMMIT, completed=False)
    try:
        tools = out/'tools'
        if a.cmake:
            cmake = a.cmake.resolve()
        else:
            wheel = out/CMAKE_WHEEL
            with urllib.request.urlopen(CMAKE_URL) as r, wheel.open('wb') as f:
                shutil.copyfileobj(r, f)
            assert hashlib.sha256(wheel.read_bytes()).hexdigest() == CMAKE_SHA256
            subprocess.run([sys.executable, '-m', 'pip', '--disable-pip-version-check', 'install',
                            '--no-cache-dir', '--no-deps', '--no-compile',
                            '--target', str(tools), str(wheel)], check=True)
            cmake = tools/'cmake/data/bin/cmake'
        result['cmake_version'] = subprocess.check_output([str(cmake), '--version'], text=True)
        assert result['cmake_version'].splitlines()[0] == 'cmake version 3.31.6'
        result['cmake_executable_sha256'] = hashlib.sha256(cmake.read_bytes()).hexdigest()
        result['cmake_wheel_sha256'] = CMAKE_SHA256
        url = 'https://codeload.github.com/ggml-org/llama.cpp/tar.gz/'+COMMIT
        archive = out/'source.tar.gz'
        if a.source_archive:
            shutil.copy2(a.source_archive, archive)
        else:
            with urllib.request.urlopen(url) as r, archive.open('wb') as f:
                shutil.copyfileobj(r, f)
        result['source_url'] = url
        result['source_archive_sha256'] = hashlib.sha256(archive.read_bytes()).hexdigest()
        assert result['source_archive_sha256'] == ARCHIVE_SHA256, 'pinned source archive changed'
        with tarfile.open(archive) as tar:
            # This is a fixed public source archive; still reject traversal/links.
            for entry in tar.getmembers():
                path = (out/entry.name).resolve()
                if out not in path.parents or entry.issym() or entry.islnk():
                    raise ValueError('unexpected archive member')
            tar.extractall(out)
        source = out/('llama.cpp-'+COMMIT)
        build = out/'build'
        # macOS /tmp is an alias for /private/tmp; CMake/Clang can use either
        # spelling in __FILE__. Normalize both without changing generated code.
        prefix_flags = ' '.join('-ffile-prefix-map="'+p+'=/llama.cpp"' for p in
                                dict.fromkeys([str(source), str(source).removeprefix('/private')]))
        subprocess.run([str(cmake), '-S', str(source), '-B', str(build),
                        '-DCMAKE_BUILD_TYPE=Release', '-DBUILD_SHARED_LIBS=ON',
                        '-DCMAKE_CXX_FLAGS='+prefix_flags,
                        '-DCMAKE_C_FLAGS='+prefix_flags,
                        '-DCMAKE_BUILD_WITH_INSTALL_RPATH=ON',
                        '-DCMAKE_INSTALL_NAME_DIR=@rpath', '-DCMAKE_INSTALL_RPATH=@loader_path',
                        '-DLLAMA_BUILD_TESTS=OFF', '-DLLAMA_BUILD_TOOLS=OFF',
                        '-DLLAMA_BUILD_EXAMPLES=OFF', '-DLLAMA_BUILD_SERVER=OFF',
                        '-DGGML_METAL=ON', '-DGGML_METAL_EMBED_LIBRARY=OFF',
                        '-DGGML_ACCELERATE=ON'], check=True)

        def build_library(name):
            subprocess.run([str(cmake), '--build', str(build), '--target', 'llama', '-j', '8'], check=True)
            candidates = [x for x in build.rglob('libllama*.dylib') if not x.is_symlink()]
            assert len(candidates) == 1, candidates
            target = out/name
            result[name] = materialize_library(candidates[0], target)

        build_library('libllama-control.dylib')
        file = source/'src/llama-context.cpp'
        original = file.read_text()
        result['context_source_sha256'] = hashlib.sha256(file.read_bytes()).hexdigest()
        patch = Path(__file__).with_name('llama-b11474-embedding-output.patch')
        subprocess.run(['patch', '--batch', '--fuzz=0', '-p1', '-i', str(patch.resolve())], cwd=source, check=True)
        result['patch_sha256'] = hashlib.sha256(patch.read_bytes()).hexdigest()
        result['compiler_version'] = subprocess.check_output(['clang++', '--version'], text=True)
        result['cmake_cache'] = (build/'CMakeCache.txt').read_text()
        diff = subprocess.run(
            ['diff', '-u', '--label', 'a/src/llama-context.cpp', '--label', 'b/src/llama-context.cpp',
             '/dev/stdin', str(file)], input=original.encode(), capture_output=True)
        assert diff.returncode == 1
        (out/'unused-logits.patch').write_bytes(diff.stdout)
        build_library('libllama-no-logits.dylib')
        result['completed'] = True
    finally:
        (out/'result.json').write_text(json.dumps(result, indent=2)+'\n')


if __name__ == '__main__':
    main()
