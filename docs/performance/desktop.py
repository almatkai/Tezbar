#!/usr/bin/env python3
"""Isolated macOS release desktop benchmark (requires --features desktop-bench,tauri/custom-protocol).
No process argument capture, clipboard writes, user profile mutation, downloads or installed-app replacement.
"""
import argparse, ctypes, hashlib, json, os, pathlib, plistlib, shutil, socket, statistics, subprocess, tempfile, time

LIB = ctypes.CDLL(None)
FIELDS = '''user_time system_time pkg_idle_wkups interrupt_wkups pageins wired_size resident_size phys_footprint proc_start_abstime proc_exit_abstime child_user_time child_system_time child_pkg_idle_wkups child_interrupt_wkups child_pageins child_elapsed_abstime diskio_bytesread diskio_byteswritten cpu_time_qos_default cpu_time_qos_maintenance cpu_time_qos_background cpu_time_qos_utility cpu_time_qos_legacy cpu_time_qos_user_initiated cpu_time_qos_user_interactive billed_system_time serviced_system_time logical_writes lifetime_max_phys_footprint instructions cycles billed_energy serviced_energy interval_max_phys_footprint runnable_time'''.split()
class Usage(ctypes.Structure):
    _fields_ = [('uuid', ctypes.c_ubyte * 16)] + [(f, ctypes.c_uint64) for f in FIELDS]
LIB.responsibility_get_pid_responsible_for_pid.argtypes = [ctypes.c_int]
LIB.responsibility_get_pid_responsible_for_pid.restype = ctypes.c_int
LIB.proc_pid_rusage.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
class Timebase(ctypes.Structure):
    _fields_ = [('numer',ctypes.c_uint32),('denom',ctypes.c_uint32)]
TIMEBASE=Timebase(); LIB.mach_timebase_info(ctypes.byref(TIMEBASE))
TICK_SECONDS=TIMEBASE.numer/TIMEBASE.denom/1e9

def require_unlocked_display():
    # Read-only preflight. Never wake/unlock the session or change power settings.
    import re
    state=subprocess.run(['ioreg','-n','Root','-d1'],capture_output=True,text=True,check=True).stdout
    if re.search(r'CGSSessionScreenIsLocked"?\s*=\s*Yes',state):
        raise RuntimeError('macOS display is locked; unlock before visible UI benchmarks')


def scrub_fixture_clipboard(root):
    # Keep the production observer enabled during measurements. Never inspect,
    # copy, hash or retain its incidental OS clipboard contents in evidence.
    # This removes only this marked OS-temp fixture's private stores after exit.
    root=pathlib.Path(root)
    if not (root/'.tezbar-desktop-benchmark').is_file():
        raise ValueError('refusing clipboard-store cleanup outside a marked fixture')
    if pathlib.Path(tempfile.gettempdir()).resolve() not in root.resolve().parents:
        raise ValueError('fixture cleanup requires an OS-temp root')
    if not (root/'data/search').resolve().is_relative_to(root.resolve()):
        raise ValueError('fixture search directory escapes its root')
    path=root/'data/search/clipboard.json'
    path.unlink(missing_ok=True)
    images=root/'data/search/clipboard-images'
    if images.is_symlink(): images.unlink()
    elif images.exists(): shutil.rmtree(images)


def processes(root):
    rows = subprocess.check_output(['ps','-axo','pid=,ppid=,comm='], text=True).splitlines()
    rows = [r.strip().split(None, 2) for r in rows]
    rows = {int(p): (int(pp), name) for p, pp, name in rows}
    owned = {root}
    for _ in range(8):
        added = {p for p,(pp,n) in rows.items() if pp in owned or LIB.responsibility_get_pid_responsible_for_pid(p) == root}
        if added <= owned: break
        owned |= added
    result=[]
    for pid in sorted(owned):
        usage=Usage()
        if LIB.proc_pid_rusage(pid, 4, ctypes.byref(usage)) != 0: continue
        pp,name=rows.get(pid,(0,''))
        result.append(dict(pid=pid, ppid=pp, name=pathlib.Path(name).name,
            role='host' if pid==root else ('native-supervisor' if pathlib.Path(name).name=='tezbar' else pathlib.Path(name).name),
            responsible=LIB.responsibility_get_pid_responsible_for_pid(pid),
            start=usage.proc_start_abstime, cpu_s=(usage.user_time+usage.system_time)*TICK_SECONDS,
            footprint=usage.phys_footprint, rss=usage.resident_size,
            wakeups=usage.interrupt_wkups, idle_wakeups=usage.pkg_idle_wkups))
    return result

class Desktop:
    def __init__(self, root): self.root=pathlib.Path(root); self.pid=None; self.samples=[]
    def command(self, op, **kwargs):
        with socket.socket(socket.AF_UNIX) as s:
            s.settimeout(190); s.connect(str(self.root/'control.sock'))
            s.sendall((json.dumps(dict(op=op,**kwargs))+'\n').encode())
            data=b''
            while not data.endswith(b'\n'):
                chunk=s.recv(1048576)
                if not chunk: raise RuntimeError('desktop disconnected')
                data+=chunk
        result=json.loads(data)
        if not result['ok']: raise RuntimeError(result['error'])
        return result['value']
    def js(self, script):
        value=self.command('js',script=script)
        if 'error' in value: raise RuntimeError(value['error'])
        return value.get('value')
    def request(self, channel, *args): return self.command('request',channel=channel,args=args)
    def launch(self, binary):
        app=self.root/'Tezbar Performance.app'; contents=app/'Contents'
        (contents/'MacOS').mkdir(parents=True,exist_ok=True); (contents/'Resources').mkdir(exist_ok=True)
        shutil.copy2(binary, contents/'MacOS'/'tezbar')
        repo=pathlib.Path(__file__).resolve().parents[2]
        for name,source in [('dist-backend',repo/'dist-backend'),('embedding-runtime',repo/'build/embedding-runtime/resources'),('trayIconTemplate@2x.png',repo/'resources/trayIconTemplate@2x.png')]:
            target=contents/'Resources'/name
            if not target.exists():
                if source.is_dir(): shutil.copytree(source,target)
                else: shutil.copy2(source,target)
        with (contents/'Info.plist').open('wb') as f:
            plistlib.dump(dict(CFBundleExecutable='tezbar',CFBundleIdentifier='com.tezbar.performance',CFBundleName='Tezbar Performance',CFBundlePackageType='APPL',LSUIElement=True,NSHighResolutionCapable=True),f)
        env={ 'TEZBAR_DATA_DIR':str(self.root/'data'), 'TEZBAR_CONFIG_DIR':str(self.root/'config'),
              'TEZBAR_BENCH_HOME':str(self.root/'home'), 'TEZBAR_BENCH_SOCKET':str(self.root/'control.sock') }
        self.started=time.monotonic()
        subprocess.run(['open','-n','-g','-o',str(self.root/'stdout.log'),'--stderr',str(self.root/'stderr.log'),str(app)]+sum((['--env',k+'='+v] for k,v in env.items()),[]),check=True)
        until=time.monotonic()+60
        while time.monotonic()<until:
            pids=subprocess.check_output(['ps','-axo','pid=,comm='],text=True).splitlines()
            exact=[int(r.strip().split(None,1)[0]) for r in pids if r.strip().endswith(str(contents/'MacOS'/'tezbar'))]
            if exact:
                self.pid=exact[0]  # retain ownership even if control setup fails
                if (self.root/'control.sock').exists(): break
            time.sleep(.1)
        if self.pid is None or not (self.root/'control.sock').exists(): raise RuntimeError('desktop did not start (check macOS crash/log reports)')
        def ready():
            try: return self.command('status')['phase']=='ready'
            except RuntimeError as error:
                if str(error)=='main window missing': return False
                raise
        try:
            self.wait(ready,60)
        except Exception:
            subprocess.run(['sample',str(self.pid),'2','-file',str(self.root/'startup-timeout-stack.txt')],capture_output=True)
            raise
        self.ready_ms=(time.monotonic()-self.started)*1000
        self.wait(lambda:self.command('status')['rendererLoaded'],60)
        self.wait(lambda:self.js("return !!window.tezbar && !!document.querySelector('input, textarea')"),60)
        self.first_ui_ms=(time.monotonic()-self.started)*1000
    def wait(self, predicate, timeout=120):
        deadline=time.monotonic()+timeout
        while time.monotonic()<deadline:
            if predicate(): return
            time.sleep(.2)
        raise RuntimeError('lifecycle milestone timed out')
    def sample(self,label):
        sample=dict(t=time.monotonic()-self.started, phase=label, processes=processes(self.pid))
        self.samples.append(sample); return sample
    def idle(self,label,seconds):
        start=self.sample(label); end=time.monotonic()+seconds
        while time.monotonic()<end:
            time.sleep(min(.5,max(0,end-time.monotonic()))); last=self.sample(label)
        return summarize([s for s in self.samples if s['phase']==label])
    def close(self):
        if self.pid:
            try:
                self.command('quit')
            except (OSError, RuntimeError):
                # A failed control socket must not leak its actual owned fixture.
                # Never target a process merely by name or a stale PID.
                expected = str(self.root/'Tezbar Performance.app/Contents/MacOS/tezbar')
                actual = subprocess.run(['ps','-p',str(self.pid),'-o','comm='],capture_output=True,text=True).stdout.strip()
                if actual == expected:
                    try:
                        subprocess.run(['osascript','-e','tell application '+json.dumps(str(self.root/'Tezbar Performance.app'))+' to quit'],capture_output=True,timeout=5)
                    except subprocess.TimeoutExpired:
                        pass
            finally:
                deadline=time.monotonic()+15
                while time.monotonic()<deadline:
                    if not any(p['pid']==self.pid for p in processes(self.pid)): break
                    time.sleep(.1)
                else:
                    expected = str(self.root/'Tezbar Performance.app/Contents/MacOS/tezbar')
                    actual = subprocess.run(['ps','-p',str(self.pid),'-o','comm='],capture_output=True,text=True).stdout.strip()
                    if actual == expected:
                        import signal
                        os.kill(self.pid, signal.SIGTERM)
                        for _ in range(50):
                            if not any(p['pid']==self.pid for p in processes(self.pid)): break
                            time.sleep(.1)
                        else: raise RuntimeError('owned desktop failed termination after graceful shutdown timeout')
                    else: raise RuntimeError('owned desktop shutdown identity changed; refusing signal')
                survivors=processes(self.pid)
                if survivors: print('owned survivors after host exit:',[p['name'] for p in survivors])
        scrub_fixture_clipboard(self.root)

def summarize(samples):
    # CPU/wakeup deltas include every observed identity, even helpers that exited.
    identities={}
    for s in samples:
        for p in s['processes']: identities.setdefault((p['pid'],p['start']),[]).append(p)
    elapsed=samples[-1]['t']-samples[0]['t']
    initial={(p['pid'],p['start']) for p in samples[0]['processes']}
    cpu=sum(v[-1]['cpu_s']-(v[0]['cpu_s'] if key in initial else 0) for key,v in identities.items())
    footprints=[sum(p['footprint'] for p in s['processes']) for s in samples]
    return dict(wall_s=elapsed,cpu_s=cpu,cpu_percent_one_core=100*cpu/elapsed if elapsed else 0,
        interrupt_wakeups=sum(v[-1]['wakeups']-(v[0]['wakeups'] if key in initial else 0) for key,v in identities.items()),
        idle_wakeups=sum(v[-1]['idle_wakeups']-(v[0]['idle_wakeups'] if key in initial else 0) for key,v in identities.items()),
        footprint_median_bytes=statistics.median(footprints),footprint_peak_bytes=max(footprints),
        footprint_last_bytes=footprints[-1], rss_last_bytes=sum(p['rss'] for p in samples[-1]['processes']),
        process_names=sorted(set(p['name'] for s in samples for p in s['processes'])))

def prepare(root,model=None):
    root=pathlib.Path(root)
    resolved=root.resolve()
    if 'build' in resolved.parts:
        raise ValueError("launcher excludes 'build' path components; use the default OS temporary directory for desktop profiles")
    if pathlib.Path(tempfile.gettempdir()).resolve() not in resolved.parents:
        raise ValueError('benchmark profiles must be beneath the OS temporary directory')
    marker=root/'.tezbar-desktop-benchmark'
    if root.exists() and any(root.iterdir()) and not marker.exists():
        raise ValueError('refusing to populate an unmarked nonempty directory')
    root.mkdir(parents=True,exist_ok=True)
    marker.write_text('Isolated Tezbar desktop benchmark profile\n')
    for name in ['data','config','home/Desktop','home/Documents','home/Downloads','home/Applications','knowledge-docs']:
        (root/name).mkdir(parents=True,exist_ok=True)
    # No credentials or actual clipboard contents are copied. The live clipboard
    # observer runs normally; benchmark never writes to the system clipboard.
    (root/'config/config.json').write_text(json.dumps(dict(hasCompletedOnboarding=True,raymesHotkey='Control+Alt+Shift+F19')))
    for i in range(5000):
        p=root/f'home/Documents/group{i%20}/runtime-fixture-{i:05}.txt';p.parent.mkdir(exist_ok=True)
        p.write_text(f'Runtime fixture {i}. Searchable document for deterministic file enumeration.\n')
    for i in range(96):
        (root/f'knowledge-docs/knowledge-{i:03}.txt').write_text((f'Runtime fixture document {i}. Orchards contain apples and pears. Search indexing preserves background progress and foreground response.\n')*12)
    if model:
        dest=root/'data/knowledge/models'/pathlib.Path(model).name;dest.parent.mkdir(parents=True,exist_ok=True)
        if not dest.exists(): subprocess.run(['cp','-cR',str(model),str(dest)],check=True)

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--root');p.add_argument('--model');p.add_argument('--output',required=True);p.add_argument('--smoke',action='store_true')
    args=p.parse_args();root=args.root or tempfile.mkdtemp(prefix='tezbar-desktop-');prepare(root,args.model)
    d=Desktop(root)
    try:
        d.launch(pathlib.Path(args.binary).resolve())
        print(json.dumps(dict(root=root,pid=d.pid,ready_ms=d.ready_ms,first_ui_ms=d.first_ui_ms,processes=processes(d.pid))),flush=True)
        result=dict(root=root,binary_sha256=hashlib.sha256(pathlib.Path(args.binary).read_bytes()).hexdigest(),ready_ms=d.ready_ms,first_ui_ms=d.first_ui_ms)
        result['smoke']=d.js("return {title:document.title,inputs:document.querySelectorAll('input,textarea').length,bridge:!!window.tezbar}")
        d.wait(lambda:d.request('core:launcher:status')['status']=='ready',120)
        d.request('knowledge:snapshot')
        d.command('hide');result['hidden']=d.idle('hidden',10 if args.smoke else 30)
        d.command('show');result['visible']=d.idle('visible',10 if args.smoke else 30)
        result['samples']=d.samples
        pathlib.Path(args.output).write_text(json.dumps(result,indent=2)+'\n')
        print(json.dumps({k:v for k,v in result.items() if k!='samples'}),flush=True)
    finally: d.close()
