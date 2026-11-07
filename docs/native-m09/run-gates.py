#!/usr/bin/env python3
"""Source-stamped local gates. Never launches the desktop app or installs anything."""
import datetime, hashlib, json, os, pathlib, re, subprocess, sys, time
ROOT=pathlib.Path(__file__).resolve().parents[2]
os.chdir(ROOT)
OUT=ROOT/'docs/native-m09/gates'
FREEZE=ROOT/'docs/native-m09/final-source-freeze-06.json'
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def inputs():
    found={}
    for top in ['apps/tezbar','crates','src/main','src/shared','tests/fixtures/extensions']:
        for directory,dirs,files in os.walk(ROOT/top):
            dirs[:]=[d for d in dirs if d not in ['target','node_modules','.git','.sc-build','__pycache__']]
            for name in files:
                path=pathlib.Path(directory)/name
                if not path.is_symlink(): found[str(path.relative_to(ROOT))]=sha(path)
    for name in ['Cargo.toml','Cargo.lock','rust-toolchain.toml','src-tauri/Cargo.lock','package.json','pnpm-lock.yaml','tsconfig.json','tsconfig.main.json','vitest.config.ts','scripts/test-extension-worker.ts','scripts/native/lock-drift.py']:
        path=ROOT/name
        if path.exists():found[name]=sha(path)
    return dict(sorted(found.items()))
def changed(expected):
    actual=inputs()
    return [p for p in sorted(set(actual)|set(expected)) if actual.get(p)!=expected.get(p)]
def short(command):return subprocess.check_output(command,text=True).strip()
if '--freeze' in sys.argv:
    files=inputs();aggregate=hashlib.sha256(json.dumps(files,sort_keys=True).encode()).hexdigest()
    data=dict(kind='M9 plus root M10 compiled-input freeze; unstaged and uncommitted',utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),head=short(['git','rev-parse','HEAD']),branch=short(['git','branch','--show-current']),rustc=short(['rustc','+1.98.1','-Vv']),files=files,aggregate_sha256=aggregate)
    FREEZE.write_text(json.dumps(data,indent=2)+'\n');print(aggregate);sys.exit(0)
frozen=json.loads(FREEZE.read_text());expected=frozen['files']
if '--verify' in sys.argv:
    diff=changed(expected);print(json.dumps(dict(changed_inputs=diff)));sys.exit(bool(diff))
prepared=pathlib.Path((OUT/'54-prepare.log').read_text().strip());fixture=json.loads(prepared.read_text());bundle=pathlib.Path(fixture['bundle'])
env=dict(os.environ,CARGO_BUILD_JOBS='2',RUST_TEST_THREADS='2',TEZBAR_M09_PREPARED=str(prepared),TEZBAR_M09_BUN='/opt/homebrew/bin/bun',TEZBAR_TEST_BUN='/opt/homebrew/bin/bun',TEZBAR_TEST_RESOURCES=str(bundle.parent),TEZBAR_TEST_WORKER_BUNDLE=str(bundle/'extension-worker.js'))
cargo=['cargo','+1.98.1'];flags=['--locked','--offline','-j2'];tests=['--','--test-threads=2']
gates=[]
def test(name,package,features=None,filter=None,extra=None):
    cmd=cargo+['test']+flags+['-p',package,'--lib']
    if features:cmd+=['--features',features]
    if filter:cmd+=[filter]
    if extra:cmd+=extra
    gates.append((name,cmd+tests,{}))
test('model-extension-regressions','tezbar-ui-model','launcher-models,agent-models','extensions')
test('root-bench-panel','tezbar-native','runtime-shaders,bench-control','windows::terminal_sessions')
test('normal-native','tezbar-native','runtime-shaders')
test('bench-native','tezbar-native','runtime-shaders,bench-control')
test('ui','tezbar-ui')
test('model-all','tezbar-ui-model','launcher-models,agent-models')
for name,filter in [('assets','assets::'),('registry-scopes','extensions::registry::asset_scope'),('desktop','desktop_effect_tests'),('worker-unit','extension_worker::tests'),('root-stream','terminal::pty::stream'),('status-shapes','extensions::process_host::shape_tests')]:test('core-'+name,'tezbar-core',filter=filter)
test('rust-bun-smoke','tezbar-core',filter='extension_worker::integration_tests::real_rust_host_bun_roundtrip_and_termination',extra=['--','--ignored','--exact'])
# Correct the split harness arguments for the pre-existing ignored smoke.
gates[-1]=(gates[-1][0],cargo+['test']+flags+['-p','tezbar-core','--lib','extension_worker::integration_tests::real_rust_host_bun_roundtrip_and_termination','--','--ignored','--exact','--test-threads=2'],{})
gates += [('channel-audit',cargo+['test']+flags+['-p','tezbar-native','--features','runtime-shaders','--lib','m9_channel_audit','--','--test-threads=2','--nocapture'],{}),('typecheck',['pnpm','typecheck'],{}),('typescript',['pnpm','test','--maxWorkers=2'],{}),('worker-smoke',['pnpm','exec','tsx','scripts/test-extension-worker.ts'],{}),('normal-check',cargo+['check']+flags+['-p','tezbar-native','--features','runtime-shaders'],{}),('debug-build',cargo+['build']+flags+['-p','tezbar-native','--features','runtime-shaders'],{}),('tree',cargo+['tree','--locked','--offline','-p','tezbar-native','--features','runtime-shaders','--edges','normal'],{}),('metadata',cargo+['metadata','--locked','--offline','--format-version','1','--filter-platform','aarch64-apple-darwin','--features','runtime-shaders'],{}),('lock-drift',['/opt/homebrew/bin/python3','scripts/native/lock-drift.py'],{}),('diff',['git','diff','--check'],{})]
msrv={'CARGO_TARGET_DIR':str(ROOT/'target/msrv-m09')}
gates += [('msrv-check',['cargo','+1.85.0','check']+flags+['-p','tezbar-core','-p','tezbar-ui-model','--all-features'],msrv),('msrv-model',['cargo','+1.85.0','test']+flags+['-p','tezbar-ui-model','--all-features','--lib']+tests,msrv),('msrv-core-stream',['cargo','+1.85.0','test']+flags+['-p','tezbar-core','--lib','terminal::pty::stream']+tests,msrv)]
suffix=sys.argv[sys.argv.index('--suffix')+1] if '--suffix' in sys.argv else ''
if '--select' in sys.argv:
    selected=set(sys.argv[sys.argv.index('--select')+1].split(','))
    gates=[gate for gate in gates if gate[0] in selected]
gates += [('core-host-port',cargo+['test']+flags+['-p','tezbar-core','--lib','platform::tests::host_']+tests,{}),('core-status-port',cargo+['test']+flags+['-p','tezbar-core','--lib','extensions::process_host::status_port_tests']+tests,{}),('core-color-formats',cargo+['test']+flags+['-p','tezbar-core','--features','native-ui-instant','--lib','instant::color::']+tests,{})]
if '--select' in sys.argv:
    selected=set(sys.argv[sys.argv.index('--select')+1].split(','))
    gates=[gate for gate in gates if gate[0] in selected]
report=OUT/f'frozen-gates-06{suffix}.json'
results=[]
for name,cmd,overrides in gates:
    before=changed(expected)
    if before:raise RuntimeError(f'Source freeze changed before {name}: {before}')
    log=OUT/f'frozen-06{suffix}-{name}.log';start=time.monotonic()
    print(f'START {name}',flush=True)
    with log.open('w')as f:
        result=subprocess.run(cmd,env=dict(env,**overrides),stdout=f,stderr=subprocess.STDOUT)
    after=changed(expected)
    summaries=[line for line in log.read_text(errors='replace').splitlines() if re.search(r'test result:|Test Files |Tests  |Finished |passed|failed|error\[',line)]
    results.append(dict(gate=name,command=cmd,environment=overrides,log=str(log.relative_to(ROOT)),exit_code=result.returncode,seconds=round(time.monotonic()-start,2),changed_inputs_before=before,changed_inputs_after=after,summaries=summaries[-12:]))
    report.write_text(json.dumps(dict(freeze=str(FREEZE.relative_to(ROOT)),aggregate_sha256=frozen['aggregate_sha256'],gates=results),indent=2)+'\n')
    print(f'END {name}: {result.returncode}',flush=True)
    if result.returncode or after:sys.exit(1)
