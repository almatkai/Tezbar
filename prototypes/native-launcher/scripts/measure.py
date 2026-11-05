#!/usr/bin/env python3
"""A/B/C local harness. Uses preserved desktop.py accounting (footprint, Mach CPU,
recursive + responsibility-owned XPC). --smoke is functional evidence, not a verdict.
Requires a fresh byte-for-byte fixture copy for EVERY invocation. Never hashes stores.
"""
import argparse,hashlib,json,os,pathlib,secrets,sqlite3,statistics,subprocess,sys,time,threading,tempfile
from control import Control
from build_identity import baseline_version,native_version
REPO=pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0,str(REPO/'docs/performance'))
from desktop import Desktop,processes,summarize,scrub_fixture_clipboard,require_unlocked_display

def wait(predicate,seconds=120):
    until=time.monotonic()+seconds
    while time.monotonic()<until:
        value=predicate()
        if value:return value
        time.sleep(.01)
    raise TimeoutError('prototype milestone timed out')
def counts(root):
    # Counts only, never SELECT clipboard text or action_json.
    with sqlite3.connect(f'file:{root}/data/search/index.sqlite3?mode=ro',uri=True) as db:
        return dict(db.execute('SELECT category,count(*) FROM documents GROUP BY category'))
class Baseline(Desktop):
    def launch(self,binary):
        # Existing desktop-bench validates raw OS-temp prefixes. Pass the OS's
        # /var spelling to env, while retaining canonical process ownership paths.
        alias=pathlib.Path(tempfile.gettempdir())/self.root.relative_to(pathlib.Path(tempfile.gettempdir()).resolve())
        app=self.root/'Tezbar Performance.app'
        assert app.is_dir(),'stage-a.py --local-preview first'
        env={'TEZBAR_DATA_DIR':str(alias/'data'),'TEZBAR_CONFIG_DIR':str(alias/'config'),'TEZBAR_BENCH_HOME':str(alias/'home'),'TEZBAR_BENCH_SOCKET':str(alias/'control.sock')}
        self.started=time.monotonic()
        subprocess.run(['open','-n','-g','-o',str(self.root/'stdout.log'),'--stderr',str(self.root/'stderr.log'),str(app)]+sum((['--env',k+'='+v] for k,v in env.items()),[]),check=True)
        expected=str(app/'Contents/MacOS/tezbar')
        def find():
            for line in subprocess.check_output(['ps','-axo','pid=,comm='],text=True).splitlines():
                fields=line.strip().split(None,1)
                if len(fields)==2 and fields[1]==expected:self.pid=int(fields[0]);return (self.root/'control.sock').exists()
            return False
        wait(find,60)
        def ready():
            try:return self.command('status')['phase']=='ready'
            except RuntimeError as error:
                if str(error)=='main window missing':return False
                raise
        self.wait(ready,60)
        self.wait(lambda:self.command('status')['rendererLoaded'],60)
        self.wait(lambda:self.js("return !!window.tezbar && !!document.querySelector('#command-input')"),60)
        self.ready_ms=(time.monotonic()-self.started)*1000
    def close(self):
        if self.pid:
            try:self.command('quit')
            except (OSError,RuntimeError):pass
            until=time.monotonic()+15
            while time.monotonic()<until:
                if not any(p['pid']==self.pid for p in processes(self.pid)):break
                time.sleep(.1)
            else:
                expected=str(self.root/'Tezbar Performance.app/Contents/MacOS/tezbar')
                actual=subprocess.run(['ps','-p',str(self.pid),'-o','comm='],capture_output=True,text=True).stdout.strip()
                if actual and actual!=expected:raise RuntimeError('baseline process identity changed')
                if actual:
                    import signal
                    try:os.kill(self.pid,signal.SIGTERM)
                    except ProcessLookupError:pass
                wait(lambda:not any(p['pid']==self.pid for p in processes(self.pid)),10)
        scrub_fixture_clipboard(self.root)
class Native:
    def __init__(self,root,binary,resources,headless=False,theme='system'):
        self.root=root;self.binary=binary;self.resources=resources;self.headless=headless;self.theme=theme;self.process=None;self.control=Control(root)
    def launch(self):
        token=self.root/'control-token';token.write_text(secrets.token_hex(32));token.chmod(0o600)
        self.started=time.monotonic()
        app=self.binary.parent.parent.parent
        if app.suffix!='.app':raise RuntimeError('macOS measurements require a staged bundle for independent XPC responsibility')
        subprocess.run(['open','-n','-g','-o',str(self.root/'stdout.log'),'--stderr',str(self.root/'stderr.log'),str(app),'--args','--profile',str(self.root),'--resources',str(self.resources),'--control-token-file',str(token),'--theme',self.theme]+(['--headless'] if self.headless else []),check=True)
        def find():
            for line in subprocess.check_output(['ps','-axo','pid=,comm='],text=True).splitlines():
                fields=line.strip().split(None,1)
                if len(fields)==2 and fields[1]==str(self.binary):self.pid=int(fields[0]);return True
            return False
        wait(find,60)
        wait(lambda:(self.root/'control.sock').exists() or (self.root/'control-pipe.txt').exists(),60)
        wait(lambda:self.command('status').get('launcher',{}).get('status')=='ready')
        self.ready_ms=(time.monotonic()-self.started)*1000
    def command(self,op,**fields):return self.control.command(op,**fields)
    def close(self):
        if getattr(self,'pid',None):
            try:
                self.command('quit')
            except (OSError,RuntimeError):pass
            finally:
                try:wait(lambda:not any(p['pid']==self.pid for p in processes(self.pid)),15)
                except TimeoutError:
                    expected=str(self.binary)
                    actual=subprocess.run(['ps','-p',str(self.pid),'-o','comm='],capture_output=True,text=True).stdout.strip()
                    if actual and actual!=expected:raise RuntimeError('native process identity changed')
                    if actual:
                        import signal
                        try:os.kill(self.pid,signal.SIGTERM)
                        except ProcessLookupError:pass
                    wait(lambda:not any(p['pid']==self.pid for p in processes(self.pid)),10)
        scrub_fixture_clipboard(self.root)

def settled(d,minimum=0):
    def check():
        status=d.command('status')
        if not status['visible']:raise RuntimeError('Launcher lost focus/was hidden during synthetic workload; discard this run')
        return status if status['generation']>=minimum and not status['pending'] and status['painted_generation']==status['generation'] else None
    try:return wait(check)
    except TimeoutError as error:raise TimeoutError(f'{error}; last status={d.command("status")}') from error
def open_timings(d):
    from window_visibility import main_on_screen
    d.command('hide');wait(lambda:not main_on_screen(d.pid),5)
    previous=d.command('status')['frames'] if isinstance(d,Native) else None;observed=[];stop=threading.Event()
    def poll():
        while not stop.is_set():
            if main_on_screen(d.pid):observed.append(time.monotonic());return
            stop.wait(.002)
    thread=threading.Thread(target=poll);thread.start();started=time.monotonic()
    try:
        d.command('show')
        if isinstance(d,Native):wait(lambda:d.command('status')['frames']>previous,5)
        else:d.js('await new Promise(requestAnimationFrame);await new Promise(requestAnimationFrame);return true')
        receipt=time.monotonic();thread.join(5)
        return dict(open_control_receipt_ms=(receipt-started)*1000,open_os_poll_ms=(observed[0]-started)*1000 if observed else None,app_boundary='write show -> first post-show frame receipt (includes socket/poll overhead)',os_boundary='write show -> CoreGraphics on-screen 760x640 window observed; <=2ms requested polling')
    finally:stop.set();thread.join()
def latency(d,repetitions=10):
    queries=['do','doc','Calculator','document-001','Orchard project meeting','zz-no-match-65001'];out=[]
    for rep in range(repetitions):
        for query in queries:
            for style in ['burst','characters']:
                ack=d.command('query',text='');settled(d,ack['expected_generation'])
                if style=='characters':
                    for c in query:ack=d.command('type',text=c);time.sleep(.07)
                else:ack=d.command('query',text=query)
                s=settled(d,ack['expected_generation'])
                out.append(dict(rep=rep,warmup=rep==0,query=query,style=style,**{k:s[k] for k in ['generation','input_echo_ms','input_results_ms','core_search_ms','stale_dropped']}))
    return out

def main():
    p=argparse.ArgumentParser();p.add_argument('--mode',choices=['A','B','C'],required=True);p.add_argument('--root',type=pathlib.Path,required=True);p.add_argument('--binary',type=pathlib.Path,required=True);p.add_argument('--resources',type=pathlib.Path);p.add_argument('--theme',default='system');p.add_argument('--condition',choices=['hidden','visible','retained'],default='hidden');p.add_argument('--smoke',action='store_true');p.add_argument('--output',type=pathlib.Path,required=True);a=p.parse_args()
    if sys.platform!='darwin':raise RuntimeError('macOS accounting harness only; use Windows collector for PrivateUsage')
    root=a.root.resolve();binary=a.binary.resolve();assert (root/'.tezbar-desktop-benchmark').is_file()
    if a.mode!='C':require_unlocked_display()
    d=Baseline(root) if a.mode=='A' else Native(root,binary,a.resources.resolve(),a.mode=='C',a.theme)
    source=(root/'.tezbar-native-launcher-prototype').read_text()
    version=baseline_version(binary,source) if a.mode=='A' else native_version(binary,source)
    raw=[];result=dict(mode=a.mode,condition=a.condition,smoke=a.smoke,binary_version=version,source_sha=version['source'],harness_sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip(),input_binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),resources_manifest=json.loads((root/'resource-manifest.json').read_text()) if (root/'resource-manifest.json').exists() else None,latency_boundary='synthetic committed input -> content render plus following frame callback; not photon time',runtime_shaders=True if a.mode=='B' else None)
    installed=pathlib.Path('/Applications/Tezbar.app/Contents/MacOS/tezbar');before=hashlib.sha256(installed.read_bytes()).hexdigest()
    # Retain every collected sample even when stabilization or visibility fails.
    result.update(root=str(root),samples=raw)
    startup=[];stopping=threading.Event()
    def startup_sampler():
        while not stopping.is_set():
            pid=getattr(d,'pid',None)
            if pid:startup.append(dict(t=time.monotonic(),processes=processes(pid)))
            stopping.wait(.25)
    thread=threading.Thread(target=startup_sampler,daemon=True);thread.start()
    try:
        if a.mode=='A':d.launch(binary)
        else:d.launch()
        stopping.set();thread.join();result['startup_samples']=startup
        if a.mode=='A':d.wait(lambda:d.request('core:launcher:status')['status']=='ready',120)
        measured=(root/'Tezbar Performance.app/Contents/MacOS/tezbar') if a.mode=='A' else binary
        result.update(pid=d.pid,ready_ms=d.ready_ms,indexed_counts=counts(root),measured_binary_sha256=hashlib.sha256(measured.read_bytes()).hexdigest())
        host=next(p for p in processes(d.pid) if p['pid']==d.pid)
        if host['responsible']!=d.pid:raise RuntimeError('Host must own its responsibility domain; XPC attribution would be incomplete')
        if a.mode in ['A','B']:
            result['pre_sample_steps']=['show']
            d.command('show')
            if a.condition=='retained':
                result['open_timings']=open_timings(d)
                result['pre_sample_steps']+=['hide','show','opening frame receipt','fixed query workload']
            elif a.condition=='visible':result['pre_sample_steps']+=['query doc','results committed' if a.mode=='B' else 'two animation frames; search settles during warmup']
            else:result['pre_sample_steps']+=['hide']
        if a.mode=='B':
            if a.condition=='retained' or a.smoke:result['latencies']=latency(d,1 if a.smoke else 10)
            # Exercise exactly one safe synthetic note write. No copy/open during smoke.
            if a.smoke:
                ack=d.command('query',text='qzvxtestonly');s=settled(d,ack['expected_generation'])
                index=next(i for i,r in enumerate(s['rows']) if r['id'].startswith('note-add:'))
                with sqlite3.connect(root/'data/search/index.sqlite3') as db:before_clicks=db.execute('SELECT count(*) FROM click_events').fetchone()[0]
                d.command('select',index=index);d.command('activation');wait(lambda:not d.command('status')['visible'])
                with sqlite3.connect(root/'data/search/index.sqlite3') as db:after_clicks=db.execute('SELECT count(*) FROM click_events').fetchone()[0]
                assert after_clicks-before_clicks==1;result['activation_click_records']=after_clicks-before_clicks;result['smoke_extra_work']='synthetic query workload and exactly-once add-note activation; not comparable idle evidence'
            if a.condition=='visible':
                if a.smoke:d.command('show')
                ack=d.command('query',text='doc');settled(d,ack['expected_generation'])
            else:d.command('hide')
        elif a.mode=='A':
            if a.condition=='visible':
                d.js("const e=document.querySelector('#command-input');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'doc');e.dispatchEvent(new Event('input',{bubbles:true}));await new Promise(requestAnimationFrame);await new Promise(requestAnimationFrame);return true")
            elif a.condition=='hidden':d.command('hide')
            if a.condition=='retained':
                import importlib.util
                spec=importlib.util.spec_from_file_location('native_responsiveness',REPO/'docs/performance/responsiveness-desktop.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
                d.js(module.INSTALL);observations=[]
                for rep in range(10):
                    for query in ['do','doc','Calculator','document-001','Orchard project meeting','zz-no-match-65001']:
                        for style in ['burst','characters']:
                            d.js("window.__inputBench.input('','');await new Promise(r=>setTimeout(r,30));while(window.__inputBench.active)await new Promise(r=>setTimeout(r,1));return true")
                            value=module.burst(d,[query] if style=='burst' else [query[:i] for i in range(1,len(query)+1)],70)
                            # Do not retain action envelopes, titles or incidental user input.
                            observations.append(dict(rep=rep,warmup=rep==0,query=query,style=style,**{k:value[k] for k in ['inputToTwoFramesMs','inputToCorrectDomObservedMs','ipcMs','obsoleteCompleted','trustedInputEvents']}))
                result['latencies_tauri']=observations;d.command('hide')
            # Existing, preserved harness provides the app-internal Tauri latency
            # instrumentation. Run responsiveness-desktop.py separately for A;
            # this file does not mislabel core-only timing as matched UI latency.
        if a.mode!='C':
            from window_visibility import main_on_screen
            wait(lambda:bool(main_on_screen(d.pid))==(a.condition=='visible'),5)
        result['latency_query_note']='Fresh-native-reply timing starts at two characters: A deliberately filters one-character input locally. One-character results/IPC timings are not compared. In character typing B still searches the first character while A filters locally; that extra work is included in B and can affect latency.'
        started=time.monotonic();warmup=2 if a.smoke else 120
        def sample(phase):
            if a.mode!='C':
                from window_visibility import main_on_screen
                if bool(main_on_screen(d.pid)) != (a.condition=='visible'):raise RuntimeError('Window visibility changed during measurement; discard this run')
            elapsed=time.monotonic()-started;wall_ns=time.time_ns()
            if raw:
                drift_s=(wall_ns-raw[-1]['wall_time_ns'])/1e9-(elapsed-raw[-1]['t'])
                if abs(drift_s)>2:
                    result['clock_discontinuity']=dict(drift_s=drift_s,wall_time_ns=wall_ns,t=elapsed)
                    raise RuntimeError('Wall/monotonic clock discontinuity (sleep or clock adjustment); discard this run')
            value=dict(t=elapsed,wall_time_ns=wall_ns,phase=phase,processes=processes(d.pid));raw.append(value);return value
        until=time.monotonic()+warmup
        while time.monotonic()<until:sample('warmup');time.sleep(1)
        if not a.smoke:
            # Require 30 stable one-second intervals. Bounded at 15 min; no silent pass.
            deadline=time.monotonic()+900
            while True:
                s=sample('stabilize');last=[v for v in raw if v['phase']=='stabilize'][-31:]
                cpu=[summarize(last[i:i+2])['cpu_percent_one_core'] for i in range(len(last)-1)]
                result['stabilization']=dict(intervals=len(cpu),mean_cpu=statistics.mean(cpu) if cpu else None,p95_cpu=sorted(cpu)[28] if len(cpu)==30 else None,last_intervals_cpu=cpu)
                if len(cpu)==30 and statistics.mean(cpu)<2 and sorted(cpu)[28]<5:break
                if time.monotonic()>deadline:raise TimeoutError('CPU stabilization did not meet spec')
                time.sleep(1)
        until=time.monotonic()+(3 if a.smoke else 60)
        while time.monotonic()<until:sample('measure');time.sleep(1)
        result['summary']=summarize([s for s in raw if s['phase']=='measure']);result['samples']=raw
        result['host']={cmd:subprocess.run(command,capture_output=True,text=True).stdout for cmd,command in [('swap',['sysctl','vm.swapusage']),('power',['pmset','-g','batt'])]}
        result['map_commands']=[]
        for proc in raw[-1]['processes']:
            for name,command in [('footprint',['footprint','-p',str(proc['pid'])]),('vmmap',['vmmap','-summary',str(proc['pid'])])]:
                captured=subprocess.run(command,capture_output=True,text=True,timeout=60)
                (root/f'{name}-{proc["pid"]}.log').write_text(captured.stdout)
                (root/f'{name}-{proc["pid"]}-stderr.log').write_text(captured.stderr)
                result['map_commands'].append(dict(pid=proc['pid'],command=name,returncode=captured.returncode))
            mapped=subprocess.run(['vmmap',str(proc['pid'])],capture_output=True,text=True,timeout=60)
            (root/f'webkit-mappings-{proc["pid"]}.log').write_text('\n'.join(mapped.stdout.splitlines()[:15]+[line for line in mapped.stdout.splitlines() if 'WebKit' in line])+ '\n'+mapped.stderr)
            result['map_commands'].append(dict(pid=proc['pid'],command='vmmap-WebKit-mappings',returncode=mapped.returncode))
    except BaseException as error:
        result['failure']=str(error);raise
    finally:
        stopping.set();thread.join();d.close();result['owned_survivors']=processes(d.pid) if getattr(d,'pid',None) else []
        result['installed_app_unchanged']=hashlib.sha256(installed.read_bytes()).hexdigest()==before
        a.output.write_text(json.dumps(result,indent=2)+'\n')
    assert not result['owned_survivors'];assert result['installed_app_unchanged']
    print(json.dumps({k:v for k,v in result.items() if k in ['mode','smoke','pid','ready_ms','indexed_counts','summary','owned_survivors','installed_app_unchanged']}))
if __name__=='__main__':main()
