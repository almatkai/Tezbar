from pathlib import Path
import subprocess,os,json,hashlib,time
root=Path('/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent');out=Path('/tmp/tezbar-m11-followup-20261107');p=root/'crates/tezbar-update/src/native.rs';candidate=p.read_bytes()
base=subprocess.check_output(['git','show','HEAD:crates/tezbar-update/src/native.rs'],cwd=root,text=True)
def section(s):
 a=s.index('pub fn preflight_macos_install(');b=s.index('#[cfg(unix)]\nfn swap_bundle',a);return a,b
s=candidate.decode();a,b=section(s);c,d=section(base);old=s[:a]+base[c:d]+s[b:]
env={**os.environ,'CARGO_TARGET_DIR':'/tmp/tezbar-m9-c01-validation-Rffw07/target-msrv','CARGO_INCREMENTAL':'0','CARGO_BUILD_JOBS':'2'}
command=['cargo','+1.85.0','test','--locked','--offline','-j2','-p','tezbar-update','--lib','--','native::lifecycle_tests::review_m11_recheck_unwritable_bundle_is_refused_before_quit','--exact','--test-threads=1']
def run(name,command):
 files=['Cargo.toml','Cargo.lock','src-tauri/macos-signing.json',*sorted(str(f.relative_to(root)) for f in (root/'crates/tezbar-update').rglob('*') if f.is_file() and f.suffix in ['.rs','.toml','.pub','.sig'])]
 before={f:hashlib.sha256((root/f).read_bytes()).hexdigest() for f in files};start=time.monotonic()
 with (out/(name+'.log')).open('wb') as log: r=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 after={f:hashlib.sha256((root/f).read_bytes()).hexdigest() for f in files}
 (out/(name+'.json')).write_text(json.dumps({'command':command,'exit':r.returncode,'stable_compiled_inputs':before==after,'compiled_inputs':before,'target':env['CARGO_TARGET_DIR'],'seconds':time.monotonic()-start,'log_sha256':hashlib.sha256((out/(name+'.log')).read_bytes()).hexdigest()},indent=2)+'\n')
 print(name,r.returncode,before==after,flush=True);return r.returncode
try:
 p.write_text(old);(out/'preflight-before.rs').write_text(base[c:d]);(out/'preflight-after.rs').write_text(s[a:b])
 red=run('updater-permission-red-exact',command)
finally:p.write_bytes(candidate)
assert red==101, 'Expected the actual old preflight to fail new filesystem regression'
green=run('updater-185-green',['cargo','+1.85.0','test','--locked','--offline','-j2','-p','tezbar-update','--lib','--','--test-threads=2'])
assert green==0
assert p.read_bytes()==candidate
