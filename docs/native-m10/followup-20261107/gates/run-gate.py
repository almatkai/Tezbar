from pathlib import Path
import os,sys,json,subprocess,hashlib,time
root=Path('/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent')
out=Path('/tmp/tezbar-m810-root-repair-20261107')
base=json.loads((root/'docs/native-m09/root-validation/20261107-c01-root-test-patch-Rffw07/patched-native-normal.json').read_text())
env=os.environ.copy();env.update(base['environment'])
if os.environ.get('TEZBAR_GATE_TARGET'):env['CARGO_TARGET_DIR']=os.environ['TEZBAR_GATE_TARGET']
if os.environ.get('TEZBAR_GATE_PREPARED'):
 prepared=Path(os.environ['TEZBAR_GATE_PREPARED']);data=json.loads(prepared.read_text());bundle=Path(data['bundle'])
 env.update(TEZBAR_M09_PREPARED=str(prepared),TEZBAR_TEST_RESOURCES=str(bundle.parent),TEZBAR_TEST_WORKER_BUNDLE=str(bundle/'extension-worker.js'))
def freeze():
 files=subprocess.check_output(['git','ls-files','-co','--exclude-standard','-z'],cwd=root).decode().split('\0')
 return {p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in sorted(set(files)) if p and (root/p).is_file()}
name=sys.argv[1]; cmd=sys.argv[2:]; before=freeze(); t=time.monotonic()
with (out/(name+'.log')).open('wb') as f: code=subprocess.run(cmd,cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT).returncode
after=freeze(); stable=before==after
(out/(name+'.json')).write_text(json.dumps({'command':cmd,'environment':{k:env[k] for k in base['environment']},'exit':code,'seconds':round(time.monotonic()-t,3),'stable':stable,'files':len(before),'before_aggregate':hashlib.sha256(json.dumps(before,sort_keys=True).encode()).hexdigest(),'after_aggregate':hashlib.sha256(json.dumps(after,sort_keys=True).encode()).hexdigest(),'log_sha256':hashlib.sha256((out/(name+'.log')).read_bytes()).hexdigest()},indent=2)+'\n')
(out/(name+'-sources.json')).write_text(json.dumps(before,indent=2)+'\n')
print(name,code,'stable',stable,flush=True)
print('\n'.join((out/(name+'.log')).read_text(errors='replace').splitlines()[-22:]),flush=True)
sys.exit(code if stable else 99)
