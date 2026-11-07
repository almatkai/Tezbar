"""Produce a review-only patch. Never applies it or edits a product input."""
from pathlib import Path
import difflib,hashlib,json,subprocess,re,posixpath
root=Path.cwd();out=root/'docs/native-m11/integration'
tracked=subprocess.check_output(['git','ls-files','-z']).decode().split('\0')
# Keep every TS oracle and prototype artifact. Only the original first-party runtime paths move.
move={}
for n in tracked:
 if n.startswith('src/renderer/'):move[n]='tests/oracles/legacy-renderer/'+n[len('src/renderer/'):]
 if n.startswith('src-tauri/'):
  rel=n[len('src-tauri/'):]
  if rel.startswith('icons/'):move[n]='packaging/'+rel
  elif rel=='macos-signing.json':move[n]='packaging/macos-signing.json'
  elif rel=='tauri.conf.json':move[n]='packaging/legacy-app-policy.json'
  elif rel.startswith('runtime/'):move[n]='packaging/'+rel
  else:move[n]='docs/native-m11/legacy-tauri-archive/'+rel
 if n.startswith('prototypes/native-launcher/'):
  move[n]='docs/native-m11/legacy-prototype-archive/'+n[len('prototypes/native-launcher/'):]
changes={};binary=[];rows=[]
for old,new in move.items():
 p=Path(old)
 if not p.is_file():continue
 b=p.read_bytes();rows.append(dict(path=old,action='relocate',destination=new,sha256=hashlib.sha256(b).hexdigest()))
 try:s=b.decode()
 except UnicodeDecodeError:binary.append(dict(source=old,destination=new,sha256=hashlib.sha256(b).hexdigest()));continue
 changes[old]='';changes[new]=s
# Exact relative string imports in moved files, preserving worker/shared/test authorities.
for old,new in move.items():
 if new not in changes or not old.startswith('src/renderer/'):continue
 s=changes[new]
 def rewrite(m):
  q,ref=m.group(1),m.group(2)
  target=posixpath.normpath(posixpath.join(posixpath.dirname(old),ref))
  mapped=move.get(target)
  if not mapped:
   for suffix in ['.ts','.tsx','.json','.css','/index.ts','/index.tsx']:
    if target+suffix in move:
     mapped=move[target+suffix]
     if not ref.endswith(suffix) and suffix.startswith('.'):mapped=mapped[:-len(suffix)]
     break
  target=mapped or target
  relative=posixpath.relpath(target,posixpath.dirname(new))
  if not relative.startswith('.'):relative='./'+relative
  return q+relative+q
 s=re.sub(r'''(['"])(\.[^'"\n]+)\1''',rewrite,s);changes[new]=s
# Consumers outside old products must follow relocated policy/icon/catalog bytes.
replacements={
 'src-tauri/macos-signing.json':'packaging/macos-signing.json',
 'src-tauri/tauri.conf.json':'packaging/legacy-app-policy.json',
 'src-tauri/runtime/':'packaging/runtime/',
 'src-tauri/icons/':'packaging/icons/',
}
for top in ['apps','crates','scripts']:
 for p in Path(top).rglob('*'):
  if not p.is_file() or p.suffix not in ['.rs','.mjs','.py','.ts','.json'] or 'target' in p.parts:continue
  old=p.read_bytes().decode('utf-8');new=old
  for a,b in replacements.items():new=new.replace(a,b)
  if new!=old:changes[str(p)]=new
# Exact external oracle references, including root test/typecheck configuration.
for n in tracked:
 p=Path(n)
 if n in move or not p.is_file() or p.suffix not in ['.ts','.tsx','.json','.mjs','.py'] or n.startswith(('docs/','prototypes/')):continue
 old=p.read_bytes().decode('utf-8');new=changes.get(n,old).replace('src/renderer/','tests/oracles/legacy-renderer/')
 if new!=old:changes[n]=new
# Archive frontend-only root inputs, retaining their bytes for historical reconstruction.
for n in ['index.html','vite.config.ts','postcss.config.js','tailwind.config.js']:
 p=Path(n)
 if p.exists():
  new='docs/native-m11/legacy-frontend-archive/'+n
  changes[new]=p.read_bytes().decode('utf-8');changes[n]='';rows.append(dict(path=n,action='relocate',destination=new,sha256=hashlib.sha256(p.read_bytes()).hexdigest()))
# Default change is explicit, held behind all acceptance gates; no public version or feed edit.
p=Path('package.json');s=p.read_bytes().decode('utf-8').replace('"build": "pnpm build:native && tauri build"','"build": "node scripts/native/build.mjs --release --local-preview"')
changes[str(p)]=s
patch=[]
for n,new in sorted(changes.items()):
 old=Path(n).read_bytes().decode('utf-8') if Path(n).exists() else ''
 for line in difflib.unified_diff(old.splitlines(True),new.splitlines(True),fromfile='a/'+n if old else '/dev/null',tofile='b/'+n if new else '/dev/null'):
  patch.append(line if line.endswith('\n') else line+'\n\\ No newline at end of file\n')
(out/'final-cutover-review.patch').write_text(''.join(patch))
(out/'final-cutover-inventory.json').write_text(json.dumps(dict(state='REVIEW_ONLY_NOT_APPLIED',gates=['fresh Opus full integration review','independent Windows compile/tests and disposable installer/storage rehearsal','macOS disposable OS-profile/signing/updater acceptance','manual GUI/IME/AX/GPU and matched RAM','explicit final source cutover authorization'],relocations=rows,binary_copies=binary,consumer_edits=[n for n in changes if n not in move and n not in move.values()],oracle_note='Preserves all old renderer tests/source and prototype evidence outside first-party runtime. Exact remaining dynamic and root test-config references require the recorded acceptance gates. This patch is reviewable but deliberately not represented as release-ready.',build_usage='pnpm build --resources <reviewed staged resources> --output <fresh OS-temp child>; requires explicit inputs and builds release + private assembly, never signs a public release',patch_sha256=hashlib.sha256((out/'final-cutover-review.patch').read_bytes()).hexdigest()),indent=2)+'\n')
print(len(rows),'exact relocations;',len(binary),'binary copies pinned;',len(patch),'patch lines')
