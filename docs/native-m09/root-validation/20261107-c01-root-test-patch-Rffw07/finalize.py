import pathlib,json,hashlib,shlex,re,subprocess,sys,shutil,datetime
ROOT=pathlib.Path('/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent');OUT=pathlib.Path(__file__).parent;sys.path.insert(0,str(OUT));from gate import validate,write
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest();validate();cut=(OUT/'cache-clone.json').stat().st_mtime_ns
records={};inputs={};directories=set();missing=set();generated={}
for target in [OUT/'target',OUT/'target-msrv']:
 for dep in list((target/'debug/deps').glob('tezbar*.d'))+list((target/'debug/deps').glob('app_lib*.d')):
  if dep.stat().st_mtime_ns < cut:continue
  line=dep.read_text(errors='replace').splitlines()[0]
  if ': ' not in line:continue
  files=shlex.split(line.split(': ',1)[1]);keys=[]
  for name in files:
   p=pathlib.Path(name)
   if not p.is_absolute():
    p=ROOT/p
    if not p.exists() and (ROOT/'src-tauri'/name).exists():p=ROOT/'src-tauri'/name
   p=p.resolve()
   if p.is_file():
    if p.is_relative_to(ROOT):
     key=str(p.relative_to(ROOT));inputs[key]=sha(p);keys.append(key)
    elif p.is_relative_to(OUT):generated[str(p)]=sha(p)
   elif p.is_dir():directories.add(str(p))
   else:missing.add(str(p))
  records[str(dep.relative_to(OUT))]={'sha256':sha(dep),'firstparty_inputs':keys,'compiler':'1.85.0' if target.name=='target-msrv' else '1.98.1'}
write('compiled-firstparty-source-manifest.json',{'scope':'All first-party input files in dep-info of workspace artifacts freshly compiled in this run, including targeted Tauri check. Cached registry/git dependency objects are not claimed rebuilt. All repository inputs, manifests and locks are separately hashed.','artifacts':records,'files':dict(sorted(inputs.items())),'aggregate_sha256':hashlib.sha256(json.dumps(inputs,sort_keys=True).encode()).hexdigest(),'generated_inputs':generated,'directory_inputs':sorted(directories),'missing_dep_entries':sorted(missing)})
receipts={}
for p in OUT.glob('*.json'):
 try:d=json.loads(p.read_text())
 except Exception:continue
 if isinstance(d,dict) and 'command' in d and 'exit_code' in d:
  assert d['log_sha256']==sha(OUT/(p.stem+'.log')),p
  receipts[p.stem]=d
write('gate-index.json',receipts)
summary={'source':'c01fa313fb04b5b53a97780c03ce2cb1838bb352 + exact root test-only patch; dirty','patch_sha256':'d3b390164b70563cfc9cab0a789eedfef4b51522480168f4c05d0a5987ebfff0','original_c01_native':'RED: 405 passed, 1 failed; preserved','final_native_normal':406,'final_native_bench':408,'review_m9_in_each':15,'sdk_in_each':9,'native_failed_ignored':0,'ui':57,'models_198':381,'models_185':381,'host':45,'core_worker_units':3,'core_host':12,'core_status':1,'core_explicit_launch':1,'core_actual_worker':1,'typescript_pass':746,'typescript_existing_skips':7,'typecheck':True,'worker_smoke':True,'native_check_build':True,'msrv_check':True,'tauri_compatibility_check':True,'owned_rustfmt_files':26,'owned_rustfmt_failures':0,'scoped_diff_check':True,'historical_global_format_failures':49,'historical_baseline_overlap_with_owned':['apps/tezbar/src/app/m8_tests.rs'],'global_format_not_rerun':True,'node_packaging_reused':7,'portable_updater_reused':{'pass':40,'existing_live_network_ignored':1},'original_review_and_m11_evidence_preserved':True,'no_root_target_changes':True,'no_cargo_rustc_owned_workers_remaining':True,'no_new_ignores_or_skips':True,'source_after_patch_stable':True,'compiled_firstparty_files':len(inputs),'compiled_workspace_depinfo':len(records),'safe_boundary':True,'acceptance':False,'windows':False,'GUI_IME_AX_GPU_RAM':False,'M810_root_drafts_included':False,'fresh_M11_Opus_review':False,'tauri_default':True}
write('summary.json',summary)
api=json.loads((OUT/'api-source-map.json').read_text());api['root_boundary_test_correction']=subprocess.run(['rg','-n','quiescence|session-104|dispose_extensions|drain_extensions',str(ROOT/'apps/tezbar/src/surfaces/command_bar/extension_integration_tests.rs')],capture_output=True,text=True).stdout.splitlines();write('api-source-map.json',api)
# Large private artifacts stay external; every one is referenced by exact path/hash.
external={}
for n in ['tezbar-debug-preview','root-cache-before.json','root-cache-after.json']:
 p=OUT/n;external[n]={'path':str(p),'bytes':p.stat().st_size,'sha256':sha(p)}
write('external-artifacts.json',external)
report=f'''# M9 root validation: safe boundary

Validated worktree: `{ROOT}`, branch `feat/native-gpui-app`. HEAD remains `c01fa313fb04b5b53a97780c03ce2cb1838bb352`; no index/history/branch mutation. **Final results apply to that commit plus the exact root-authored test patch, dirty. They are not clean-PIN or clean-build results.**

The original full c01 library run is preserved as RED: 405 passed, one failed, zero ignored. `retained_store_runtime_ack_source_fence_and_settings_route` still assumed `drain_extensions` destroyed its active session. At a verified boundary with no Cargo/compiler, only root's authorized one-file patch was applied. Patch SHA256: `d3b390164b70563cfc9cab0a789eedfef4b51522480168f4c05d0a5987ebfff0`. It asserts two prior disposals and retained session-104 after quiescence, invokes actual final `dispose_extensions`, drains it and retains the original three-session disposal assertion. Source before/after hashes and exact patch are included. No other implementation, expectation, skip, manifest, lock or format change was made.

| Gate | Actual result | Evidence |
|---|---|---|
| Patched full native normal, runtime-shaders | 406 passed, 0 failed/ignored | patched-native-normal.json/.log |
| Patched full native bench | 408 passed, 0 failed/ignored | patched-native-bench.json/.log |
| Root regressions / original real SDK scenarios | 15 / 9 pass in **each** full suite | test-counts.json; full raw logs |
| UI | 57 passed | ui.json/.log |
| Models, all features, Rust 1.98.1 / 1.85.0 | 381 / 381 passed | models-198.json; models-185.json |
| Core worker framing/startup units / typed Host routes | 3 / 12 passed | core-worker.json; core-host.json |
| Core status boundary / explicit launch validation | 1 / 1 passed | core-status.json; core-launch.json |
| Explicit real Rust↔Bun worker lifecycle | 1 passed, run explicitly despite its pre-existing ignored annotation | core-worker-integration.json/.log |
| Host | 45 passed | host.json/.log |
| Typecheck / full TypeScript | passed / 746 passed, 7 existing skips | typecheck.json; typescript.json |
| Actual production worker smoke | passed with **new private** prepared bundle and mocked desktop/AI boundary | worker-smoke.json/.log |
| Date cross-realm/local calendar | Asia/Almaty covered by full suites; America/Los_Angeles targeted test passed | patched-date-los-angeles.json/.log |
| Native normal check / debug build | passed, bench-control=false | patched-native-check.json; patched-native-build.json |
| Rust 1.85.0 core/host/update/model all-features check | passed | msrv-check.json/.log |
| Tauri compatibility check | passed, locked/offline; sandbox denied all checkout writes and network | tauri-compatibility.json/.log; tauri-readonly.sb |
| Scoped Rustfmt / diff | 26 owned Rust files pass / source diff passes | patched-format-results.json; patched-owned-diff-check.json |
| Unchanged M11 packaging / updater | reused 7 packaging passes / 40 updater passes, one existing live-network ignore | m11-preservation-and-reuse.json; unchanged-components.json |

The UI/model/core/Host/TS/worker executions completed on clean c01 before applying root's test-only correction. Their complete component sources are unchanged by that one test-file patch; `patched-component-reuse.json` records this explicitly. They are not relabelled as post-patch reruns. Native normal/bench/check/build and MSRV/compatibility receipts are from the patched dirty freeze. Core was never run unfiltered: no login shell, credential or actual PTY/ConPTY exercise. Unchanged asset/history/PTY/protocol component evidence is scoped in the reuse manifests; the worker/core boundaries changed by root were actually tested again.

## Source and artifact provenance

All 608 root frozen input hashes and all 39 root-owned hashes matched initially. The original review matched SHA256 `f7206c9e0e270758c40589481eab1bc0ed13d1e30575bcda3fc13e01c04296f4`; all 43 root repair logs and 105 existing M11 integration evidence files were hash verified and preserved. Original root aggregate `c44dfaa97ae5e846dd20142d5ae15e4a5d06267905c20548488b142c1c53dadd` is recorded as supplied; individual hashes were verified. A distinct sorted-JSON aggregation was used for this run's manifests.

The patched freeze covers 608 inputs, SHA256 `4aaab379936783b63057dd566881b10a8518c098f6007e985c4fc0ba4925896d`. The broader before/after inventory hashes 2691 tracked and source/resource/test files, including detection of untracked source additions; final aggregate is `7489620ceef8fcf88e34c1e4e7143a27178f6158ac5b5e2de0f3db829230f84a`. Every command receipt records verified before/after source identity. `compiled-firstparty-source-manifest.json` records {len(inputs)} actual first-party dep-info inputs across {len(records)} freshly compiled workspace artifacts, including the Tauri check. Warm third-party dependency objects are not claimed rebuilt. The complete input manifests and worker's entire bundled input graph are retained.

Cargo used only owned APFS-cloned private targets, locked/offline, jobs=2 and test threads=2, with explicit Rust 1.98.1 or 1.85.0 and Bun `/opt/homebrew/bin/bun`. No incremental directory or writeback symlink was copied. Workspace fingerprints/build outputs were invalidated before first Cargo; the App's were invalidated again after the exact patch. Redundant stale copied workspace objects were never executed and cannot satisfy the removed fingerprints. Root cache size/mtime/inode inventory of 312893 nonincremental entries is unchanged; large raw inventories remain in the owned temporary directory with exact hashes in `external-artifacts.json`. Git index bytes, main/secondary Cargo locks, pnpm lock, toolchain and manifests are unchanged. Dependency/license deltas remain those already audited at 9230bcc; `lock-license.json` pins the inherited receipt and locks.

Preserved debug executable: `{OUT/'tezbar-debug-preview'}`. SHA256 `5e57d2c5432674b2731d5e798e2367d6a31b6c09eb54ccf928a4332cd9bafd5e`. Its sole execution was pre-App `--version`. Metadata reports source c01, dirty=true, rustc commit `48a229ceaefd4985c50990b14116b6d856af0985`, GPUI `c84af5b99cbc366d675064e52b23664a015b6e35`, runtime shaders=true, bench-control=false. `otool -L` and the normal Cargo dependency tree contain no WebKit/JavaScriptCore/Tauri/Wry/Tao. The successful **separate** compatibility check does compile legacy Tauri dependencies and does not change the normal executable's graph. Binary size is not RAM evidence.

Fresh worker bundle: `{json.loads((OUT/'worker-bundle.json').read_text())['prepared']['owned']}`. All 23 bundled source inputs, 22 first-party graph inputs and all 40 generated/resource file hashes were captured; resources remain unchanged. No shared dist-backend was generated or edited. The rejecting-host no-view test and actual positive in-flight Preferences event passed, as did effect sequencing/no replay. Workers were launched only by explicit private fixtures, never App startup. Lifecycle and final process receipts verify owned workers are gone.

## Preserved failures and limits

Original c01 RED log, historical root failures, and raw whitespace are retained. The broad diff check reports whitespace only in preserved raw logs; scoped owned source diff passes. An initial aggregate-serialization assertion and a private format harness `FileExistsError` are identified as harness failures; the former preceded Cargo, and the latter was corrected only in the private harness and rerun. No source change was used to hide either. All gate/log SHA256 values are indexed. The historical global Rustfmt baseline is 49 files; one owned file (`app/m8_tests.rs`) appears in that historical list, and all 26 current scoped Rust checks pass. Global formatting was not mass-fixed or rerun.

M11 migration/state admission, updater actor, quiescence/resume, eight-second quit budget and root extension admission/teardown remain integrated. Default legacy Host controls remain no-op and SDK tray capability false; the legacy app compiles with those defaults. MenuBarExtra's intentional background owner remains; manual source-zero/background-control acceptance is open.

**This is a headless verification safe boundary, not M9/M11/full-migration acceptance.** Windows remains blocked before these gates; no provider/account/CLI/native or remote fallback was attempted. Physical GUI/IME/AX/GPU/RAM, actual old-profile discovery, native cutover, signed updater/installer/relaunch and signing acceptance remain open. New M810 R1–R7 root drafts are outside this tree and were not included; fresh M11 Opus review was rate limited and is not claimed. Tauri remains the default product. No subagent, push/PR/merge/release/Actions/install/signing, user profile/data, live provider, credential, clipboard or Keychain action occurred. Only fake test effects were exercised.

All builds/gates finished before these documents were copied into the repository. No Cargo/compiler or owned worker/App process remains. Root owns subsequent implementation and any later M810 integration.
'''
(OUT/'report.md').write_text(report)
print('FINAL REPORT READY',len(inputs),'compiled first-party files',len(receipts),'gate receipts')
