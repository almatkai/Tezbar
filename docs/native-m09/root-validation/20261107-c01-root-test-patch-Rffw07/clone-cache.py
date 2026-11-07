import pathlib,subprocess,shutil,os,json,time
out=pathlib.Path(__file__).parent;root=pathlib.Path('/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent');start=time.time();copies=[];removed=[]
for source,target in [(root/'target/debug',out/'target/debug'),(root/'target/msrv-m09/debug',out/'target-msrv/debug')]:
 target.mkdir(parents=True)
 for name in ['deps','build','.fingerprint']:
  p=source/name; assert p.is_dir()
  assert not any(x.is_symlink() for x in p.rglob('*')),'cache symlink requires review'
  subprocess.run(['/bin/cp','-cR',str(p),str(target/name)],check=True);copies.append(str(p))
 # Invalidate only workspace outputs in PRIVATE copy, forcing exact-PIN workspace recompilation.
 for kind in ['deps','build','.fingerprint']:
  for p in (target/kind).iterdir():
   n=p.name
   if n.startswith(('tezbar','libtezbar')):
    removed.append(str(p.relative_to(out)))
    if p.is_dir():shutil.rmtree(p)
    else:p.unlink()
(out/'cache-clone.json').write_text(json.dumps({'method':'APFS cp -cR; no incremental; no symlinks; no writeback','copies':copies,'removed_workspace_outputs':removed,'seconds':time.time()-start},indent=2));print('CLONED',len(removed),'workspace outputs invalidated',flush=True)
