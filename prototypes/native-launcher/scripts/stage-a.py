#!/usr/bin/env python3
"""Local-preview-only baseline bundle. Never modifies an installed application."""
import argparse,pathlib,plistlib,shutil,subprocess,tempfile,hashlib,json
from build_identity import baseline_version
p=argparse.ArgumentParser();p.add_argument('--local-preview',action='store_true',required=True);p.add_argument('--root',type=pathlib.Path,required=True);p.add_argument('--binary',type=pathlib.Path,required=True);a=p.parse_args()
r=a.root.resolve();repo=pathlib.Path(__file__).resolve().parents[3]
assert pathlib.Path(tempfile.gettempdir()).resolve() in r.parents and (r/'.tezbar-desktop-benchmark').is_file()
receipt=baseline_version(a.binary,(r/'.tezbar-native-launcher-prototype').read_text())
app=r/'Tezbar Performance.app';assert not app.exists();contents=app/'Contents';(contents/'MacOS').mkdir(parents=True)
shutil.copy2(a.binary,contents/'MacOS/tezbar')
# Same resource generator as B/C, from the same selected checkout's built payloads.
subprocess.run(['/opt/homebrew/bin/python3',str(pathlib.Path(__file__).with_name('stage-resources.py')),str(r)],check=True,stdout=subprocess.DEVNULL)
(r/'resources').chmod(0o755)
(r/'resources').rename(contents/'Resources')
for entry in (contents/'Resources').rglob('*'):entry.chmod(entry.stat().st_mode|0o200)
(contents/'Resources').chmod(0o755)
with (contents/'Info.plist').open('wb') as f:plistlib.dump(dict(CFBundleExecutable='tezbar',CFBundleIdentifier='com.tezbar.performance',CFBundleName='Tezbar Performance',CFBundlePackageType='APPL',LSUIElement=True,NSHighResolutionCapable=True),f)
# Explicit local preview identity; no production bundle ID/requirement is re-signed.
subprocess.run(['codesign','--force','--deep','--sign','-','--identifier','com.tezbar.performance',str(app)],check=True)
subprocess.run(['codesign','--verify','--deep','--strict',str(app)],check=True)
resources=contents/'Resources'
manifest={str(p.relative_to(resources)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(resources.rglob('*')) if p.is_file()}
(r/'resource-manifest.json').write_text(json.dumps(dict(matched_A=True,files=manifest),indent=2))
for entry in resources.rglob('*'):entry.chmod(0o555 if entry.is_dir() or entry.stat().st_mode&0o111 else 0o444)
resources.chmod(0o555)
print(app)
