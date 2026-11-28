import gate
from pathlib import Path
import subprocess,json,os,hashlib,traceback
OUT=gate.OUT;ROOT=gate.ROOT
names=['crates/tezbar-core/src/extension_worker.rs','crates/tezbar-core/src/extensions/process_host.rs']
expected=json.loads((OUT/'expected.json').read_text());assert gate.freeze()==expected
saved={p:(ROOT/p).read_bytes() for p in names};baseline={p:subprocess.check_output(['git','--no-optional-locks','show',gate.PIN+':'+p],cwd=ROOT) for p in names}
backup=OUT/'red-03-durable-backups';backup.mkdir(exist_ok=False)
for kind,data in [('implementation',saved),('pin',baseline)]:
 for p,b in data.items():
  target=backup/kind/p;target.parent.mkdir(parents=True,exist_ok=True)
  with target.open('xb') as f:f.write(b);f.flush();os.fsync(f.fileno())
  assert target.read_bytes()==b
manifest={p:{'implementation':hashlib.sha256(saved[p]).hexdigest(),'baseline_pin':hashlib.sha256(baseline[p]).hexdigest()} for p in names};gate.write(backup/'hashes.json',manifest)
result={'state':'STARTED','meaningful_regression_red':False};error=None
try:
 for p in names:
  with (ROOT/p).open('wb') as f:f.write(baseline[p]);f.flush();os.fsync(f.fileno())
 red_expected=dict(expected)
 for p in names:red_expected[p]={'kind':'file','sha256':hashlib.sha256(baseline[p]).hexdigest()}
 assert gate.freeze()==red_expected
 cmd=['cargo','+1.98.1','test','--locked','--offline','-j2','-p','tezbar-native','--lib','--features','runtime-shaders']
 wanted='asset_transport::status_icons::tests::status_icons_real_bun_declared_builtin_and_custom_pixels_cross_core_host'
 listed=gate.run('red-03-registered-name',cmd+[wanted,'--','--list'],red_expected)
 result['list_exit']=listed
 if listed==0:
  text=(OUT/'red-03-registered-name.log').read_text()
  registered=[line.removesuffix(': test') for line in text.splitlines() if line.endswith(': test')]
  assert registered==[wanted],repr(registered)
  result['actual_registered_test']=registered[0]
  code=gate.run('red-03-exact',cmd+[registered[0],'--','--exact','--test-threads=1','--nocapture'],red_expected)
  result['red_exit']=code
  rec=json.loads((OUT/'red-03-exact.json').read_text())
  result['meaningful_regression_red']=code!=0 and rec['counts']==[{'passed':0,'failed':1,'ignored':0}]
  result['state']='REGRESSION_RED' if result['meaningful_regression_red'] else 'NOT_MEANINGFUL_RED'
 else:result['state']='COMPILE_OR_LIST_FAILURE_NOT_REGRESSION_RED'
except BaseException as e:
 error=repr(e);result['exception']=error;result['traceback']=traceback.format_exc()
finally:
 for p in names:
  restored=(backup/'implementation'/p).read_bytes()
  assert hashlib.sha256(restored).hexdigest()==manifest[p]['implementation']
  with (ROOT/p).open('wb') as f:f.write(restored);f.flush();os.fsync(f.fileno())
 proof={p:{'expected_implementation_sha256':manifest[p]['implementation'],'restored_sha256':gate.sha(ROOT/p),'restored':gate.sha(ROOT/p)==manifest[p]['implementation']} for p in names}
 actual=gate.freeze();result['restoration']=proof;result['whole_tree_restored']=actual==expected;result['restored_aggregate']=gate.aggregate(actual)
 gate.write(OUT/'red-03-restoration-proof.json',result)
 print(json.dumps(result,indent=2),flush=True)
 assert all(p['restored'] for p in proof.values()) and actual==expected,'RESTORATION FAILURE: STOP ALL GATES'
if error:raise RuntimeError(error)
