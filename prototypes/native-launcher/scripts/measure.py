#!/usr/bin/env python3
"""A/B/C local harness. Uses preserved desktop.py accounting (footprint, Mach CPU,
recursive + responsibility-owned XPC). --smoke is functional evidence, not a verdict.
Requires a fresh byte-for-byte fixture copy for EVERY invocation. Never hashes stores.
"""
import argparse,hashlib,json,os,pathlib,secrets,sqlite3,statistics,subprocess,sys,time
from control import Control
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
class Native:
    def __init__(self,root,binary,resources,headless=False,theme='system'):
        self.root=root;self.binary=binary;self.resources=resources;self.headless=headless;self.theme=theme;self.process=None;self.control=Control(root)
    def launch(self):
        token=self.root/'control-token';token.write_text(secrets.token_hex(32));token.chmod(0o600)
        self.stdout=(self.root/'stdout.log').open('w');self.stderr=(self.root/'stderr.log').open('w')
        self.started=time.monotonic()
        self.process=subprocess.Popen([str(self.binary),'--profile',str(self.root),'--resources',str(self.resources),'--control-token-file',str(token),'--theme',self.theme]+(['--headless'] if self.headless else ['--show']),stdout=self.stdout,stderr=self.stderr)
        self.pid=self.process.pid
        wait(lambda:(self.root/'control.sock').exists() or (self.root/'control-pipe.txt').exists(),60)
        wait(lambda:self.command('status').get('launcher',{}).get('status')=='ready')
        self.ready_ms=(time.monotonic()-self.started)*1000
    def command(self,op,**fields):return self.control.command(op,**fields)
    def close(self):
        if self.process is not None:
            try:
                if self.process.poll() is None:self.command('quit')
            finally:
                try:self.process.wait(15)
                except subprocess.TimeoutExpired:self.process.terminate();self.process.wait(10)
                self.stdout.close();self.stderr.close()
        scrub_fixture_clipboard(self.root)

def settled(d,minimum=0):
    return wait(lambda:(s if s['generation']>=minimum and not s['pending'] and s['painted_generation']==s['generation'] else None) if (s:=d.command('status')) else None)
def latency(d,repetitions=10):
    queries=['d','doc','Calculator','document-001','Orchard project meeting','zz-no-match-65001'];out=[]
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
    d=Desktop(root) if a.mode=='A' else Native(root,binary,a.resources.resolve(),a.mode=='C',a.theme)
    raw=[];result=dict(mode=a.mode,condition=a.condition,smoke=a.smoke,source_sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip(),binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),resources_manifest=json.loads((root/'resource-manifest.json').read_text()) if (root/'resource-manifest.json').exists() else None,latency_boundary='synthetic committed input -> content render plus following frame callback; not photon time',runtime_shaders=True if a.mode=='B' else None)
    installed=pathlib.Path('/Applications/Tezbar.app/Contents/MacOS/tezbar');before=hashlib.sha256(installed.read_bytes()).hexdigest()
    try:
        if a.mode=='A':d.launch(binary)
        else:d.launch()
        result.update(pid=d.pid,ready_ms=d.ready_ms,indexed_counts=counts(root))
        if a.mode=='B':
            d.command('show');ack=d.command('query',text='doc');s=settled(d,ack['expected_generation']);result['smoke_status']=s
            if a.condition=='retained' or a.smoke:result['latencies']=latency(d,1 if a.smoke else 10)
            # Exercise exactly one safe synthetic note write. No copy/open during smoke.
            if a.smoke:
                ack=d.command('query',text='qzvxtestonly');s=settled(d,ack['expected_generation'])
                index=next(i for i,r in enumerate(s['rows']) if r['id'].startswith('note-add:'))
                with sqlite3.connect(root/'data/search/index.sqlite3') as db:before_clicks=db.execute('SELECT count(*) FROM click_events').fetchone()[0]
                d.command('select',index=index);d.command('activation');wait(lambda:not d.command('status')['visible'])
                with sqlite3.connect(root/'data/search/index.sqlite3') as db:after_clicks=db.execute('SELECT count(*) FROM click_events').fetchone()[0]
                assert after_clicks-before_clicks==1;result['activation_click_records']=after_clicks-before_clicks
            if a.condition=='visible':d.command('show');ack=d.command('query',text='doc');settled(d,ack['expected_generation'])
            else:d.command('hide')
        elif a.mode=='A':
            if a.condition=='visible':d.command('show')
            else:d.command('show');d.command('hide')
            # Existing, preserved harness provides the app-internal Tauri latency
            # instrumentation. Run responsiveness-desktop.py separately for A;
            # this file does not mislabel core-only timing as matched UI latency.
        started=time.monotonic();warmup=2 if a.smoke else 120
        def sample(phase):
            value=dict(t=time.monotonic()-started,phase=phase,processes=processes(d.pid));raw.append(value);return value
        until=time.monotonic()+warmup
        while time.monotonic()<until:sample('warmup');time.sleep(1)
        if not a.smoke:
            # Require 30 stable one-second intervals. Bounded at 15 min; no silent pass.
            deadline=time.monotonic()+900
            while True:
                s=sample('stabilize');last=[v for v in raw if v['phase']=='stabilize'][-31:]
                cpu=[summarize(last[i:i+2])['cpu_percent_one_core'] for i in range(len(last)-1)]
                if len(cpu)==30 and statistics.mean(cpu)<2 and sorted(cpu)[28]<5:break
                if time.monotonic()>deadline:raise TimeoutError('CPU stabilization did not meet spec')
                time.sleep(1)
        until=time.monotonic()+(3 if a.smoke else 60)
        while time.monotonic()<until:sample('measure');time.sleep(1)
        result['summary']=summarize([s for s in raw if s['phase']=='measure']);result['samples']=raw
        result['host']={cmd:subprocess.run(command,capture_output=True,text=True).stdout for cmd,command in [('swap',['sysctl','vm.swapusage']),('power',['pmset','-g','batt'])]}
        for proc in raw[-1]['processes']:
            for name,command in [('footprint',['footprint','-p',str(proc['pid'])]),('vmmap',['vmmap','-summary',str(proc['pid'])])]:
                (root/f'{name}-{proc["pid"]}.log').write_text(subprocess.run(command,capture_output=True,text=True).stdout)
    finally:
        d.close();result['owned_survivors']=processes(d.pid) if getattr(d,'pid',None) else []
        result['installed_app_unchanged']=hashlib.sha256(installed.read_bytes()).hexdigest()==before
        a.output.write_text(json.dumps(result,indent=2)+'\n')
    assert not result['owned_survivors'];assert result['installed_app_unchanged']
    print(json.dumps({k:v for k,v in result.items() if k in ['mode','smoke','pid','ready_ms','indexed_counts','summary','owned_survivors','installed_app_unchanged']}))
if __name__=='__main__':main()
