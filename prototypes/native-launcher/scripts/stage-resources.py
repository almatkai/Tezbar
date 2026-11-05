#!/usr/bin/env python3
"""Stage only this checkout's built, read-only resources for local prototype smoke.
For matched measurements pass the actual A Contents/Resources with --from-resources.
"""
import argparse,hashlib,json,pathlib,shutil,subprocess,tempfile
p=argparse.ArgumentParser();p.add_argument('root');p.add_argument('--from-resources');a=p.parse_args()
root=pathlib.Path(a.root).resolve();repo=pathlib.Path(__file__).resolve().parents[3]
assert pathlib.Path(tempfile.gettempdir()).resolve() in root.parents
assert (root/'.tezbar-native-launcher-prototype').is_file()
resources=root/'resources';assert not resources.exists()
if a.from_resources: shutil.copytree(a.from_resources,resources)
else:
    resources.mkdir()
    for name,source in [('dist-backend',repo/'dist-backend'),('embedding-runtime',repo/'build/embedding-runtime/resources')]:shutil.copytree(source,resources/name)
    shutil.copy2(repo/'resources/trayIconTemplate@2x.png',resources/'trayIconTemplate@2x.png')
(resources/'.tezbar-prototype-source').write_text((root/'.tezbar-native-launcher-prototype').read_text())
# No profile stores are traversed or hashed here.
manifest={str(p.relative_to(resources)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(resources.rglob('*')) if p.is_file()}
(root/'resource-manifest.json').write_text(json.dumps(dict(matched_A=bool(a.from_resources),files=manifest),indent=2))
for path in resources.rglob('*'):path.chmod(0o555 if path.is_dir() or path.stat().st_mode&0o111 else 0o444)
resources.chmod(0o555)
print(resources)
