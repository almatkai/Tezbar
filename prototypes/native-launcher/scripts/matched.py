#!/usr/bin/env python3
"""Execute the spec's per-condition A B B A A B, with C before/after.
Run only after Opus review. Full session takes over an hour. Fresh identical
fixture for every run; raw evidence is never overwritten or mixed with references.
"""
import argparse,hashlib,json,pathlib,shutil,subprocess,tempfile
from build_identity import native_version,baseline_version
p=argparse.ArgumentParser();p.add_argument('--a-binary',required=True,type=pathlib.Path);p.add_argument('--b-binary',required=True,type=pathlib.Path);p.add_argument('--template',required=True,type=pathlib.Path);p.add_argument('--output',required=True,type=pathlib.Path);p.add_argument('--protocol',choices=['strict-idle','background-idle'],default='strict-idle');p.add_argument('--continue-on-error',action='store_true',help='Preserve failed runs and attempt the rest; session still fails');a=p.parse_args()
py='/opt/homebrew/bin/python3';scripts=pathlib.Path(__file__).parent;a.output.mkdir(parents=True,exist_ok=False)
manifest=json.loads((a.template/'fixture-manifest.json').read_text());source=(a.template/'.tezbar-native-launcher-prototype').read_text()
version=native_version(a.b_binary,source);baseline=baseline_version(a.a_binary,source)
order=[('C','hidden')]+[(m,c) for c in ['hidden','visible','retained'] for m in ['A','B','B','A','A','B']]+[('C','hidden')]
# Stage a dedicated Resources reference for the whole session, with the same A payload.
reference=pathlib.Path(tempfile.mkdtemp(prefix='tezbar-native-reference-')).resolve();shutil.copytree(a.template,reference,dirs_exist_ok=True)
subprocess.run([py,str(scripts/'stage-a.py'),'--local-preview','--root',str(reference),'--binary',str(a.a_binary.resolve())],check=True)
resources=reference/'Tezbar Performance.app/Contents/Resources'
failures=[]
for i,(mode,condition) in enumerate(order):
    root=pathlib.Path(tempfile.mkdtemp(prefix=f'tezbar-native-{mode}-{i}-')).resolve();shutil.copytree(a.template,root,dirs_exist_ok=True)
    for name,digest in manifest['files'].items():assert hashlib.sha256((root/name).read_bytes()).hexdigest()==digest
    if mode=='A':subprocess.run([py,str(scripts/'stage-a.py'),'--local-preview','--root',str(root),'--binary',str(a.a_binary.resolve())],check=True)
    else:subprocess.run([py,str(scripts/'stage-resources.py'),str(root),'--from-resources',str(resources)],check=True)
    binary=a.a_binary.resolve() if mode=='A' else a.b_binary.resolve()
    if mode in ['B','C']:
        subprocess.run([str(scripts/'bundle-macos.sh'),'--local-preview',str(root),'--binary',str(a.b_binary.resolve())],check=True)
        binary=root/'Tezbar Launcher Prototype.app/Contents/MacOS/tezbar-launcher-prototype'
        assert native_version(binary,source)==version
    with (a.output/f'{i:02}-{mode}-{condition}.log').open('w') as log:
        run=subprocess.run([py,str(scripts/'measure.py'),'--protocol',a.protocol,'--mode',mode,'--condition',condition,'--root',str(root),'--binary',str(binary),'--resources',str(root/'resources'),'--output',str((a.output/f'{i:02}-{mode}-{condition}.json').resolve())],stdout=log,stderr=subprocess.STDOUT)
        if run.returncode:
            failures.append(dict(index=i,mode=mode,condition=condition,returncode=run.returncode))
            if not a.continue_on_error:run.check_returncode()
# Fail rather than compare different search datasets.
runs=[json.loads(p.read_text()) for p in sorted(a.output.glob('[0-9]*.json'))]
indexed=[run['indexed_counts'] for run in runs if 'indexed_counts' in run]
assert indexed and all(value==indexed[0] for value in indexed), 'Indexed counts differ across variants'
# No ratios/verdict synthesized here. Each raw run plus per-process maps is inspectable.
(a.output/'session.json').write_text(json.dumps(dict(source=source,version=version,baseline=baseline,order=order,failures=failures,protocol=a.protocol,strict_cpu_gate_passed=False if a.protocol=='background-idle' else not failures,fixture_manifest=manifest,reference=str(reference),note='A UI latency: run preserved responsiveness-desktop.py separately; all-process idle runs here'),indent=2))

if failures:raise RuntimeError(f'{len(failures)} matrix runs failed; see preserved evidence')
