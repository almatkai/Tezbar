"""Fail closed on stale binaries, compiler drift, and stale staged resources."""
import hashlib,json,pathlib,re,subprocess
REPO=pathlib.Path(__file__).resolve().parents[3]
def sha(path):return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
def frontend():
    root=REPO/'dist'
    if not (root/'index.html').is_file():raise RuntimeError('Built frontend missing')
    return {str(p.relative_to(root)):sha(p) for p in sorted(root.rglob('*')) if p.is_file()}
def expected_compiler():
    text=subprocess.check_output(['rustc','+1.98.1','-vV'],text=True)
    return next(line.split(': ',1)[1] for line in text.splitlines() if line.startswith('commit-hash: '))
def compiler(binary):
    hashes=set(re.findall(rb'[/\\]rustc[/\\]([0-9a-f]{40})[/\\]',pathlib.Path(binary).read_bytes()))
    if hashes!={expected_compiler().encode()}:raise RuntimeError(f'Unexpected compiler identities: {hashes}')
    return next(iter(hashes)).decode()
def resources():
    result={}
    for label,root in [('dist-backend',REPO/'dist-backend'),('embedding-runtime',REPO/'build/embedding-runtime/resources')]:
        for path in sorted(root.rglob('*')):
            if path.is_file():result[f'{label}/{path.relative_to(root)}']=sha(path)
    result['trayIconTemplate@2x.png']=sha(REPO/'resources/trayIconTemplate@2x.png')
    return result
def native_version(binary,source=None):
    version=json.loads(subprocess.check_output([str(pathlib.Path(binary).resolve()),'--version'],text=True))
    if version['dirty']!='false' or not version['bench_control']:raise RuntimeError('Need clean bench-control B binary')
    if source is not None and version['source']!=source:raise RuntimeError('B binary source differs from fixture')
    if version['rustc_commit']!=compiler(binary):raise RuntimeError('B compiler metadata mismatch')
    return version
def baseline_version(binary,source):
    binary=pathlib.Path(binary).resolve()
    receipt=json.loads(binary.with_name(binary.name+'.build-receipt.json').read_text())
    if receipt['source']!=source or receipt['dirty'] or receipt['binary_sha256']!=sha(binary):raise RuntimeError('A receipt source/clean/hash mismatch')
    if receipt['rustc_commit']!=compiler(binary):raise RuntimeError('A compiler mismatch')
    if receipt['resource_files']!=resources():raise RuntimeError('A resources changed since build')
    if receipt['frontend_files']!=frontend():raise RuntimeError('A frontend changed since build')
    return receipt
