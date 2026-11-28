#!/usr/bin/env python3
"""Build instrumented A from a clean checkout and attest its exact compiler/payload."""
import json,pathlib,subprocess,os
from build_identity import REPO,compiler,resources,frontend,sha
status=lambda:subprocess.check_output(['git','status','--porcelain'],cwd=REPO,text=True).strip()
if status():raise RuntimeError('Commit source changes before building A')
source=subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip()
subprocess.run(['pnpm','build:frontend'],cwd=REPO,check=True)
before_frontend=frontend()
before=resources()
target=REPO/'prototypes/native-launcher/target/tauri-a'
env=dict(os.environ,CARGO_TARGET_DIR=str(target),CARGO_BUILD_JOBS='2')
subprocess.run(['cargo','+1.98.1','build','--manifest-path',str(REPO/'src-tauri/Cargo.toml'),'--locked','--release','--features','desktop-bench,tauri/custom-protocol'],cwd=REPO,env=env,check=True)
if status() or subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip()!=source or resources()!=before or frontend()!=before_frontend:raise RuntimeError('Source/resources/frontend changed during A build')
binary=target/'release/tezbar'
receipt=dict(source=source,dirty=False,rustc_commit=compiler(binary),binary_sha256=sha(binary),resource_files=before,frontend_files=before_frontend,main_lock_sha256=sha(REPO/'src-tauri/Cargo.lock'))
binary.with_name(binary.name+'.build-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(binary)
