"""Behavioral tests of the actual injected wrapper/burst, not a copy of it.
Run: python3 -m unittest discover -s docs/performance -p 'renderer_loop_test.py'
"""
import importlib.util
import json
from pathlib import Path
import subprocess
import unittest

spec=importlib.util.spec_from_file_location('renderer_loop',Path(__file__).with_name('renderer-loop.py'))
r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)

BOOT=r'''
globalThis.window=globalThis;
let rows=[{id:'fixture',title:'Fixture row',score:1,action:{type:'open-file',path:'/fixture'}}];
let dom=[];let dispatches=0;
class Input {dispatchEvent(){dispatches++;window.tezbar.searchAll(this.value).then(value=>{dom=value.map(r=>({textContent:r.title}));});}}
Object.defineProperty(Input.prototype,'value',{get(){return this.text||'';},set(v){this.text=v;}});
globalThis.HTMLInputElement=Input;const input=new Input();
globalThis.document={addEventListener(){},querySelector(){return input;},querySelectorAll(){return dom;}};
globalThis.tezbar={searchAll:async()=>rows};
globalThis.requestAnimationFrame=callback=>setTimeout(callback,0);
const execute=source=>new Function('return (async()=>{'+source+'})()')();
'''


def node(script):
    run=subprocess.run(['node','--input-type=module','-e',BOOT+script],capture_output=True,text=True,check=True,timeout=10)
    return json.loads(run.stdout)


class Capture:
    def js(self,script):return script


def burst_script():
    return r.b.burst(r.LeanDesktop(Capture()),['fixture'],0)


class RendererLoopTests(unittest.TestCase):
    def test_old_protocol_is_red_for_outside_burst_row_retention(self):
        value=node('await execute('+json.dumps(r.b.INSTALL)+');await tezbar.searchAll("home");console.log(JSON.stringify(!!__inputBench.calls[0].rows));')
        self.assertTrue(value)

    def test_lean_never_retains_rows_outside_burst(self):
        value=node('await execute('+json.dumps(r.LEAN_INSTALL)+');__inputBench.latest="home";await tezbar.searchAll("home");console.log(JSON.stringify({rows:!!__inputBench.calls[0].rows,final:!!__inputBench.final}));')
        self.assertEqual(value,dict(rows=False,final=False))

    def test_lean_temporary_owner_only_captures_latest_reply(self):
        value=node('await execute('+json.dumps(r.LEAN_INSTALL)+');__inputBench.captureFinal=true;__inputBench.latest="final";await tezbar.searchAll("obsolete");const old=!!__inputBench.final;await tezbar.searchAll("final");console.log(JSON.stringify({old,current:__inputBench.final.rows===rows,metadataRows:__inputBench.calls.some(c=>!!c.rows)}));')
        self.assertEqual(value,dict(old=False,current=True,metadataRows=False))

    def test_real_burst_requires_native_reply_checks_dom_and_releases_payload(self):
        value=node('await execute('+json.dumps(r.LEAN_INSTALL)+');const result=await execute('+json.dumps(burst_script())+');console.log(JSON.stringify({dispatches,completed:result.completed,count:result.domRowCount,returnedRows:"finalRows" in result,returnedTitles:"domTitles" in result,retained:!!__inputBench.final,capture:__inputBench.captureFinal}));')
        self.assertEqual(value,dict(dispatches=1,completed=1,count=1,returnedRows=False,returnedTitles=False,retained=False,capture=False))

    def test_cached_dom_cannot_replace_fresh_final_reply(self):
        value=node('await execute('+json.dumps(r.LEAN_INSTALL)+');dom=[{textContent:"Fixture row"}];tezbar.searchAll=()=>new Promise(()=>{});let clock=0;globalThis.performance={now:()=>clock+=1000};try{await execute('+json.dumps(burst_script())+');console.log("false");}catch(e){console.log(JSON.stringify(String(e).includes("final input did not complete")));}')
        self.assertTrue(value)

    def test_error_path_releases_temporary_owner(self):
        class Broken:
            def __init__(self):self.calls=[]
            def js(self,script):
                self.calls.append(script)
                if len(self.calls)==1:raise RuntimeError('guard failed')
        d=Broken()
        with self.assertRaisesRegex(RuntimeError,'guard failed'):
            r.LeanDesktop(d).js('return result;')
        self.assertEqual(len(d.calls),2)
        self.assertIn('captureFinal=false',d.calls[1])
        self.assertIn('delete window.__inputBench.final',d.calls[1])

    def test_cleanup_disconnect_does_not_hide_original_guard_error(self):
        class Broken:
            def __init__(self):self.calls=0
            def js(self,script):
                self.calls+=1
                raise RuntimeError('guard failed' if self.calls==1 else 'cleanup disconnected')
        with self.assertRaisesRegex(RuntimeError,'guard failed'):
            r.LeanDesktop(Broken()).js('return result;')

    def test_lean_keeps_physical_input_and_two_frame_guards(self):
        script=burst_script()
        self.assertEqual(script.count('await new Promise(requestAnimationFrame); await new Promise(requestAnimationFrame);'),1)
        self.assertIn('Physical input interfered',script)
        self.assertIn('state.calls.slice(offset).find(c=>c.query===target && c.end && !c.error)',script)
        self.assertIn('while(!domMatches())',script)
        self.assertIn('while(state.active)',script)


if __name__=='__main__':unittest.main()
