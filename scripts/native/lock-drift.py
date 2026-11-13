#!/usr/bin/env python3
"""Audit the separate lock domains against the accepted Tauri seed. Python 3.11+."""
import hashlib, json, pathlib, subprocess, tomllib
ROOT = pathlib.Path(__file__).resolve().parents[2]
BASE = '55971131bf2bcb2cd4e44129fc0c1a130d085500'
WATCH = {'tokio','rusqlite','serde_json','notify','walkdir','arboard','regex','unicode-normalization','image','reqwest'}
def packages(data):
    return { (p['name'],p['version'],p.get('source','path')): p for p in tomllib.loads(data)['package'] }
def versions(packages):
    result={}
    for name,version,source in packages:
        if source.startswith('registry+'):
            result.setdefault(name,[]).append(version)
    return {name:sorted(v) for name,v in result.items()}
seed=packages(subprocess.check_output(['git','show',f'{BASE}:src-tauri/Cargo.lock'],cwd=ROOT).decode())
a=versions(seed)
report={'baseline':BASE,'locks':{}}
failed=False
for path, current_path in [('Cargo.lock','Cargo.lock'), ('src-tauri/Cargo.lock','docs/native-m11/legacy-tauri-archive/Cargo.lock'), ('prototypes/native-launcher/Cargo.lock','prototypes/native-launcher/Cargo.lock')]:
    data=(ROOT/current_path).read_bytes(); current=packages(data.decode()); b=versions(current)
    changes=[dict(name=name,before=a[name],after=b[name]) for name in sorted(a.keys() & b.keys()) if a[name]!=b[name]]
    watch=[c for c in changes if c['name'] in WATCH]
    registry_changes=sorted(set(k for k in seed if k[2].startswith('registry+')) ^ set(k for k in current if k[2].startswith('registry+')))
    registry_added=sorted(k for k in current.keys()-seed.keys() if k[2].startswith('registry+'))
    registry_removed=sorted(k for k in seed.keys()-current.keys() if k[2].startswith('registry+'))
    domain_seed=subprocess.run(['git','show',f'{BASE}:{path}'],cwd=ROOT,capture_output=True,text=True)
    domain_registry_changes=None
    if domain_seed.returncode==0:
        domain=packages(domain_seed.stdout)
        domain_registry_changes=sorted(set(k for k in domain if k[2].startswith('registry+')) ^ set(k for k in current if k[2].startswith('registry+')))
    report['locks'][path]={'current_path':current_path,'sha256':hashlib.sha256(data).hexdigest(),'watchlist':{name:b.get(name) for name in sorted(WATCH)},'watchlist_changes':watch,'shared_registry_changes':changes,'registry_set_changes':registry_changes,'registry_added':registry_added,'registry_removed':registry_removed,'domain_baseline_registry_set_changes':domain_registry_changes,'path_packages':sorted(k[:2] for k in current if k[2]=='path')}
    failed |= bool(watch) or (path=='src-tauri/Cargo.lock' and bool(registry_changes)) or (path=='prototypes/native-launcher/Cargo.lock' and bool(domain_registry_changes))
print(json.dumps(report,indent=2))
raise SystemExit(1 if failed else 0)
