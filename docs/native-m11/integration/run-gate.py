import pathlib,hashlib,json,subprocess,sys,time,os
root=pathlib.Path.cwd();out=root/'docs/native-m11/integration';out.mkdir(exist_ok=True)
def inputs():
 result={}
 for top in ['apps/tezbar','crates','src/main','src/shared','tests/fixtures/extensions','scripts','packaging','native/input','resources','docs/releases','docs/native-m11/upstream']:
  for d,ds,fs in os.walk(top):
   ds[:]=[x for x in ds if x not in ['target','node_modules','.git','__pycache__','.sc-build']]
   for n in fs:
    p=pathlib.Path(d)/n
    if not p.is_symlink():result[str(p)]=hashlib.sha256(p.read_bytes()).hexdigest()
 for n in ['Cargo.toml','Cargo.lock','rust-toolchain.toml','package.json','pnpm-lock.yaml','tsconfig.json','tsconfig.main.json','vitest.config.ts','src-tauri/tauri.conf.json','src-tauri/macos-signing.json','src-tauri/runtime/embedding-macos-arm64.json','src-tauri/Cargo.lock','src-tauri/icons/icon.icns','src-tauri/icons/icon.ico']:
  p=pathlib.Path(n)
  if p.exists():result[n]=hashlib.sha256(p.read_bytes()).hexdigest()
 return dict(sorted(result.items()))
name=sys.argv[1];cmd=sys.argv[2:];before=inputs();(out/f'{name}-inputs.json').write_text(json.dumps(before,indent=2)+'\n');start=time.monotonic()
env=dict(os.environ,CARGO_BUILD_JOBS='2',RUST_TEST_THREADS='2',TEZBAR_M09_BUN='/opt/homebrew/bin/bun',TEZBAR_TEST_BUN='/opt/homebrew/bin/bun')
env.pop('TEZBAR_SIGNING_TEST_IDENTITY',None) # Never access a real certificate in this lane.
prepared=pathlib.Path('docs/native-m09/gates/54-prepare.log').read_text().strip();f=json.loads(pathlib.Path(prepared).read_text());bundle=pathlib.Path(f['bundle']);env.update(TEZBAR_M09_PREPARED=prepared,TEZBAR_TEST_RESOURCES=str(bundle.parent),TEZBAR_TEST_WORKER_BUNDLE=str(bundle/'extension-worker.js'))
with (out/f'{name}.log').open('wb')as log:r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT,env=env)
after=inputs();changed=[p for p in set(before)|set(after) if before.get(p)!=after.get(p)]
receipt=dict(command=cmd,environment={k:v for k,v in env.items() if k.startswith(('TEZBAR_M09','TEZBAR_TEST','CARGO_BUILD','RUST_TEST'))},exit_code=r.returncode,seconds=round(time.monotonic()-start,2),inputs_sha256=hashlib.sha256(json.dumps(before,sort_keys=True).encode()).hexdigest(),changed_inputs=sorted(changed),log_sha256=hashlib.sha256((out/f'{name}.log').read_bytes()).hexdigest())
(out/f'{name}.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt),flush=True);sys.exit(r.returncode or bool(changed))
