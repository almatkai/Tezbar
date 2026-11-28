import pathlib,subprocess,json,hashlib,os,sys,time
ROOT=pathlib.Path('/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent'); OUT=pathlib.Path(__file__).parent; PIN='c01fa313fb04b5b53a97780c03ce2cb1838bb352'
os.chdir(ROOT)
os.environ["GIT_OPTIONAL_LOCKS"]="0"
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def git(*args):return subprocess.check_output(['git',*args]).decode().strip()
def snapshot():
 paths=set(git('ls-files').splitlines())|set(git('ls-files','--others','--exclude-standard').splitlines())
 for top in ['apps','crates','src','src-tauri','scripts','tests','resources','packaging','native/input']:
  for d,ds,fs in os.walk(ROOT/top):
   ds[:]=[x for x in ds if x not in ['target','node_modules','.git','.sc-build','__pycache__']]
   for n in fs:
    p=pathlib.Path(d)/n
    if not p.is_symlink():paths.add(str(p.relative_to(ROOT)))
 paths={p for p in paths if not p.startswith('docs/native-m09/root-validation/')}
 return {p:sha(ROOT/p) for p in sorted(paths) if (ROOT/p).is_file()}
def write(n,d):(OUT/n).write_text(json.dumps(d,indent=2)+'\n')
def validate():
 assert git('rev-parse','HEAD')==PIN
 assert git('branch','--show-current')=='feat/native-gpui-app'
 f=json.loads(((OUT/'active-freeze.json') if (OUT/'active-freeze.json').exists() else ROOT/'docs/native-m09/root-repairs/source-freeze.json').read_text()); mism=[p for p,h in f['files'].items() if sha(ROOT/p)!=h];assert not mism,mism
 initial=json.loads(((OUT/'patched-all-files.json') if (OUT/'patched-all-files.json').exists() else OUT/'initial-all-files.json').read_text());current=snapshot();changed=sorted(p for p in set(initial)|set(current) if initial.get(p)!=current.get(p));assert not changed,changed
 return {'source_label':f.get('label','clean c01 PIN'),'head':PIN,'branch':git('branch','--show-current'),'status':git('status','--porcelain','--untracked-files=all'),'file_count':len(current),'root_freeze_files':len(f['files']),'root_freeze_aggregate':f['aggregate_sha256'],'aggregate':hashlib.sha256(json.dumps(current,sort_keys=True).encode()).hexdigest()}
def run(name,cmd,extra=None):
 before=validate();write(name+'-before.json',before);env=dict(os.environ,CARGO_TARGET_DIR=str(OUT/'target'),CARGO_INCREMENTAL='0',CARGO_BUILD_JOBS='2',RUST_TEST_THREADS='2',CARGO_NET_OFFLINE='true',TEZBAR_M09_BUN='/opt/homebrew/bin/bun',TEZBAR_TEST_BUN='/opt/homebrew/bin/bun',TZ='Asia/Almaty')
 for k in ['TEZBAR_NATIVE_SOURCE_SHA','TEZBAR_SIGNING_TEST_IDENTITY','TEZBAR_RELEASE_NOTES_PATH','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','TAURI_CONFIG']:env.pop(k,None)
 pp=OUT/'prepare.log'
 if pp.exists() and pp.read_text().strip():
  prepared=pp.read_text().strip(); f=json.loads(pathlib.Path(prepared).read_text());env.update(TEZBAR_M09_PREPARED=prepared,TEZBAR_TEST_RESOURCES=str(pathlib.Path(f['bundle']).parent),TEZBAR_TEST_WORKER_BUNDLE=str(pathlib.Path(f['bundle'])/'extension-worker.js'))
 if extra:env.update(extra)
 start=time.time();print('START '+name,flush=True)
 with (OUT/(name+'.log')).open('wb') as log:r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT,env=env,cwd=ROOT)
 after=validate();write(name+'-after.json',after);rec={'command':cmd,'environment':{k:env[k] for k in ['CARGO_TARGET_DIR','CARGO_INCREMENTAL','CARGO_BUILD_JOBS','RUST_TEST_THREADS','TEZBAR_M09_BUN','TEZBAR_TEST_BUN','TZ','TEZBAR_M09_PREPARED','TEZBAR_TEST_RESOURCES','TEZBAR_TEST_WORKER_BUNDLE'] if k in env},'exit_code':r.returncode,'seconds':round(time.time()-start,2),'log_sha256':sha(OUT/(name+'.log')),'before':before,'after':after};write(name+'.json',rec);print(json.dumps({k:rec[k] for k in ['exit_code','seconds','log_sha256']}),flush=True);return r.returncode
if __name__=='__main__':
 if sys.argv[1]=='init':
  write('initial-all-files.json',snapshot());print(json.dumps(validate()))
 elif sys.argv[1]=='validate':print(json.dumps(validate()))
 else:sys.exit(run(sys.argv[1],sys.argv[2:]))
