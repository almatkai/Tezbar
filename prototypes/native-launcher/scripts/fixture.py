#!/usr/bin/env python3
"""Create deterministic, marked OS-temp fixture. Prints its canonical path."""
import argparse,hashlib,json,pathlib,random,subprocess,tempfile
p=argparse.ArgumentParser();p.add_argument('--files',type=int,default=20000);p.add_argument('--seed',type=int,default=65001);p.add_argument('--source');p.add_argument('--root');a=p.parse_args()
repo=pathlib.Path(__file__).resolve().parents[3]
source=a.source or subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()
root=pathlib.Path(a.root or tempfile.mkdtemp(prefix='tezbar-native-')).resolve()
assert pathlib.Path(tempfile.gettempdir()).resolve() in root.parents
assert not any(root.iterdir()),'refusing nonempty fixture'
(root/'.tezbar-native-launcher-prototype').write_text(source)
(root/'.tezbar-desktop-benchmark').write_text('Isolated desktop benchmark\n')
for name in ['data/search','config','home/Applications','home/Desktop','home/Documents','home/Downloads','home/Pictures','temp']:(root/name).mkdir(parents=True,exist_ok=True)
(root/'config/config.json').write_text(json.dumps(dict(hasCompletedOnboarding=True,raymesHotkey='Control+Alt+Shift+F19')))
r=random.Random(a.seed);extensions=['txt','md','csv','json'];folders=['Desktop','Documents','Downloads','Pictures']
for i in range(a.files):
    path=root/f'home/{folders[i%4]}/group{i%20}/document-{i:05}.{r.choice(extensions)}';path.parent.mkdir(exist_ok=True)
    path.write_text(f'Prototype document {i}. Orchard project meeting notes. Seed {a.seed}.\n')
for name,records in [('notes',dict(notes=[dict(text=f'Orchard project meeting note {i}',createdAt=1700000000000+i,updatedAt=1700000000000+i) for i in range(50)])),('snippets',dict(snippets=[dict(id=f'snippet:user:fixture-{i}',label=f'Orchard snippet {i}',trigger=f'orch{i}',body=f'Synthetic orchard snippet {i}',createdAt=1700000000000+i) for i in range(50)])),('quick-links',dict(links=[dict(id=f'ql:fixture-{i}',name=f'Orchard link {i}',template=f'https://example.com/{i}?q={{query}}',createdAt=1700000000000+i) for i in range(20)]))]:
    (root/f'data/search/{name}.json').write_text(json.dumps(records,ensure_ascii=False))
# Manifest is made before any app runs: no clipboard store can be included.
files={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(root.rglob('*')) if p.is_file()}
(root/'fixture-manifest.json').write_text(json.dumps(dict(source=source,seed=a.seed,file_count=a.files,files=files),indent=2))
print(root)
