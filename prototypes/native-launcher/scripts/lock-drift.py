#!/usr/bin/env python3
"""Compare every shared package; never modifies either lockfile. Python 3.11+."""
import hashlib, json, pathlib, tomllib
root = pathlib.Path(__file__).resolve().parents[3]
a_path, b_path = root/'src-tauri/Cargo.lock', root/'prototypes/native-launcher/Cargo.lock'
def packages(path):
    result = {}
    for p in tomllib.loads(path.read_text())['package']:
        result.setdefault(p['name'], set()).add((p['version'], p.get('source', 'path')))
    return {k: sorted(v) for k,v in result.items()}
a, b = packages(a_path), packages(b_path)
core = {'tokio','rusqlite','serde_json','notify','walkdir','arboard','regex','unicode-normalization','image','reqwest','tauri'}
shared = [dict(name=n, app=a[n], prototype=b[n], changed=a[n]!=b[n], core_watchlist=n in core) for n in sorted(a.keys() & b.keys())]
print(json.dumps(dict(app_lock_sha256=hashlib.sha256(a_path.read_bytes()).hexdigest(), prototype_lock_sha256=hashlib.sha256(b_path.read_bytes()).hexdigest(), shared=shared, changed=[p for p in shared if p['changed']]), indent=2))
