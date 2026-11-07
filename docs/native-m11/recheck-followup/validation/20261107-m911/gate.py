from pathlib import Path
import os,sys,json,subprocess,hashlib,time,signal,re
ROOT=Path('/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent')
OUT=Path(__file__).parent
PIN='6ffa4e50fc0a45927319f9d791677ed40f78a948'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def freeze():
 names=subprocess.check_output(['git','--no-optional-locks','ls-files','-co','--exclude-standard','-z'],cwd=ROOT).decode().split('\0'); result={}
 for name in sorted(set(names)):
  if not name:continue
  p=ROOT/name
  if p.is_symlink(): result[name]={'kind':'symlink','sha256':hashlib.sha256(os.readlink(p).encode()).hexdigest()}
  elif p.is_file(): result[name]={'kind':'file','sha256':sha(p)}
 return result
def aggregate(files):return hashlib.sha256(json.dumps(files,sort_keys=True).encode()).hexdigest()
def write(p,data):
 assert not p.exists(),f'Preserve prior evidence: {p}'
 with p.open('x') as f: json.dump(data,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def external():
 pointer=Path('/tmp/tezbar-followup-prepared-20261107.txt'); prepared=Path(pointer.read_text().strip());data=json.loads(prepared.read_text()); paths=[pointer,prepared,Path(__file__)]
 for key in ['seed','bundle']:
  paths.extend(p for p in Path(data[key]).rglob('*') if p.is_file())
 return {str(p):sha(p) for p in sorted(set(paths))}
def workers():
 bundle=json.loads(Path(Path('/tmp/tezbar-followup-prepared-20261107.txt').read_text().strip()).read_text())['bundle']
 text=subprocess.check_output(['ps','-axo','pid=,ppid=,pgid=,command='],text=True)
 return [line.strip() for line in text.splitlines() if str(Path(bundle)/'extension-worker.js') in line]
def run(name,cmd,expected=None):
 expected=expected or json.loads((OUT/'expected.json').read_text())
 assert not (OUT/(name+'.json')).exists()
 before=freeze();ext=external();before_workers=workers()
 write(OUT/(name+'-before.json'),before);write(OUT/(name+'-external-before.json'),ext)
 assert before==expected, f'Pre-gate freeze mismatch {name}: {[p for p in set(before)|set(expected) if before.get(p)!=expected.get(p)]}'
 assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()==PIN
 env=os.environ.copy();p=Path('/tmp/tezbar-followup-prepared-20261107.txt').read_text().strip();data=json.loads(Path(p).read_text());bundle=Path(data['bundle'])
 env.update(CARGO_TARGET_DIR='/tmp/tezbar-m9-c01-validation-Rffw07/target',CARGO_INCREMENTAL='0',CARGO_BUILD_JOBS='2',RUST_TEST_THREADS='2',TZ=os.environ.get('GATE_TZ','Asia/Almaty'),TEZBAR_M09_BUN='/opt/homebrew/bin/bun',TEZBAR_TEST_BUN='/opt/homebrew/bin/bun',TEZBAR_M09_PREPARED=p,TEZBAR_TEST_RESOURCES=str(bundle.parent),TEZBAR_TEST_WORKER_BUNDLE=str(bundle/'extension-worker.js'))
 if os.environ.get('GATE_TARGET'):env['CARGO_TARGET_DIR']=os.environ['GATE_TARGET']
 t=time.monotonic();timed_out=False;err=None;code=None;child=None
 try:
  with (OUT/(name+'.log')).open('xb') as f:
   child=subprocess.Popen(cmd,cwd=ROOT,env=env,stdout=f,stderr=subprocess.STDOUT,start_new_session=True)
   try:code=child.wait(timeout=1800)
   except subprocess.TimeoutExpired:
    timed_out=True;os.killpg(child.pid,signal.SIGTERM)
    try:child.wait(timeout=10)
    except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
    code=124
 except BaseException as e:
  err=repr(e)
  if child and child.poll() is None:
   os.killpg(child.pid,signal.SIGTERM);child.wait(timeout=10)
  raise
 finally:
  after=freeze();ext_after=external();after_workers=workers();stable=before==after and ext==ext_after
  log=OUT/(name+'.log');text=log.read_text(errors='replace') if log.exists() else ''
  counts=[{'passed':int(p),'failed':int(f),'ignored':int(i)} for p,f,i in re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored;',text)]
  record={'name':name,'command':cmd,'cwd':str(ROOT),'head':PIN,'dirty':True,'source_label':'PIN plus frozen current WIP; NOT clean PIN','environment':{k:env[k] for k in ['CARGO_TARGET_DIR','CARGO_INCREMENTAL','CARGO_BUILD_JOBS','RUST_TEST_THREADS','TZ','TEZBAR_M09_BUN','TEZBAR_TEST_BUN','TEZBAR_M09_PREPARED','TEZBAR_TEST_RESOURCES','TEZBAR_TEST_WORKER_BUNDLE']},'exit':code,'exception':err,'timed_out':timed_out,'seconds':round(time.monotonic()-t,3),'stable':stable,'files':len(before),'before_aggregate':aggregate(before),'after_aggregate':aggregate(after),'changed_files':[p for p in set(before)|set(after) if before.get(p)!=after.get(p)],'counts':counts,'log_sha256':sha(log) if log.exists() else None,'fixture_workers_before':before_workers,'fixture_workers_after':after_workers,'new_fixture_survivors':[p for p in after_workers if p not in before_workers]}
  write(OUT/(name+'-after.json'),after);write(OUT/(name+'-external-after.json'),ext_after);write(OUT/(name+'.json'),record)
  print(json.dumps(record,indent=2));print('\n'.join(text.splitlines()[-45:]),flush=True)
 if not stable or record['new_fixture_survivors']:return 99
 return code
if __name__=='__main__':sys.exit(run(sys.argv[1],sys.argv[2:]))
