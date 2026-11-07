"""Produce a NEW review-only successor. Never apply it to a product checkout."""
from pathlib import Path
import argparse,difflib,hashlib,json,subprocess,re,posixpath,tempfile,os

def lf_lines(text):
    # splitlines also splits U+0085/U+2028/U+2029 and corrupts diff boundaries.
    parts = text.split('\n')
    return [part+'\n' for part in parts[:-1]] + ([parts[-1]] if parts[-1] else [])

def make_patch(changes, root):
    patch = []
    for name, new in sorted(changes.items()):
        path = root/name
        old = path.read_bytes().decode('utf-8') if path.exists() else ''
        for line in difflib.unified_diff(lf_lines(old),lf_lines(new),fromfile='a/'+name if old else '/dev/null',tofile='b/'+name if new else '/dev/null'):
            patch.append(line if line.endswith('\n') else line+'\n\\ No newline at end of file\n')
    return ''.join(patch).encode('utf-8')

def verify_roundtrip(changes, patch, root):
    # No repository, Git index, new checkout or worktree. Only changed input bytes.
    with tempfile.TemporaryDirectory(prefix='tezbar-cutover-roundtrip-') as temp:
        scratch = Path(temp)
        for name in changes:
            source = root/name
            if source.exists():
                target = scratch/name
                target.parent.mkdir(parents=True,exist_ok=True)
                target.write_bytes(source.read_bytes())
        artifact = scratch/'review.patch'
        artifact.write_bytes(patch)
        for args in [['--check'], []]:
            subprocess.run(['git','apply',*args,str(artifact)],cwd=scratch,check=True,capture_output=True)
        for name, new in changes.items():
            target = scratch/name
            actual = target.read_bytes() if target.exists() else b''
            if actual != new.encode('utf-8'):
                raise RuntimeError('Cutover byte roundtrip failed: '+name)
    return dict(state='PASSED_PRIVATE_BYTE_ROUNDTRIP',files=len(changes),patch_sha256=hashlib.sha256(patch).hexdigest())

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir',type=Path,required=True,help='Fresh successor directory; never overwrite held proof')
    args=parser.parse_args()
    root=Path(__file__).resolve().parents[3]
    os.chdir(root)
    out=args.output_dir.resolve()
    out.mkdir(parents=True,exist_ok=False)
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
    patch = make_patch(changes, root)
    roundtrip = verify_roundtrip(changes, patch, root)
    (out/'final-cutover-review.patch').write_bytes(patch)
    (out/'final-cutover-inventory.json').write_text(json.dumps(dict(state='NOT_APPLIED_NOT_RELEASE_READY',gates=['implement and independently review the actual native signed release builder (macOS + Windows), receipts and updater archives, with no Tauri CLI or src-tauri dependency', 'implement and independently review the native signing/feed pipeline: pinned certificate and helpers, resource seal, Tauri updater signatures, dual-platform feed and receipt policy', 'post-apply pnpm typecheck and pnpm test against all relocated import and oracle authorities', 'post-apply native release-builder dry run from a clean private source snapshot proving complete resources, manifest schema 2, installer/archive/receipt/feed contracts without signing or publishing', 'resolve dev, tauri, tauri:dev and icons:generate scripts after src-tauri relocation', 'fresh Opus full integration review','independent Windows compile/tests and disposable installer/storage rehearsal','macOS disposable OS-profile/signing/updater acceptance','manual GUI/IME/AX/GPU and matched RAM','explicit final source cutover authorization'],relocations=rows,binary_copies=binary,consumer_edits=[n for n in changes if n not in move and n not in move.values()],oracle_note='Preserves all old renderer tests/source and prototype evidence outside first-party runtime. Exact remaining dynamic and root test-config references require the recorded acceptance gates. This patch is reviewable but deliberately not represented as release-ready.',release_builder='NOT_IMPLEMENTED: scripts/build-release.mjs invokes Tauri; the native local-preview assembler does not replace the public release pipeline',roundtrip=roundtrip,build_usage='PRIVATE PREVIEW ONLY; NOT A RELEASE COMMAND: pnpm build --resources <reviewed staged resources> --output <fresh OS-temp child>; requires explicit inputs and builds release + private assembly, never signs a public release',patch_sha256=hashlib.sha256((out/'final-cutover-review.patch').read_bytes()).hexdigest()),indent=2)+'\n')
    print(len(rows),'exact relocations;',len(binary),'binary copies pinned;',len(patch),'patch bytes')

if __name__ == '__main__':
    main()
