"""Probe API preservation, bounded ownership and frame-callback ordering."""
import importlib.util
import json
from pathlib import Path
import subprocess
import unittest


def load(name):
    spec=importlib.util.spec_from_file_location(name,Path(__file__).with_name(name+'.py'))
    module=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


p=load('row-layout-probe')
f=load('row-layout-frames')


def node(code):
    result=subprocess.run(['node','--input-type=module','-e',code],text=True,capture_output=True,check=True,timeout=10)
    return json.loads(result.stdout)


class RowLayoutTests(unittest.TestCase):
    def test_css_probe_scopes_skipping_and_events_to_search_result_rows(self):
        script=p.INSTALL.replace('SELECTOR',json.dumps(p.SELECTOR)).replace('ENABLE','true')
        value=node('''globalThis.window=globalThis;let listener,css;
          globalThis.CSS={supports:()=>true};
          globalThis.document={addEventListener:(name,fn)=>listener=fn,
           createElement:()=>({}),head:{append:el=>css=el.textContent}};
          new Function('''+json.dumps(script)+''')();
          listener({target:{matches:()=>false},skipped:true});
          listener({target:{matches:()=>true},skipped:true});
          listener({target:{matches:()=>true},skipped:false});
          console.log(JSON.stringify({css,events:__rowLayoutProbe.events,
            skipped:__rowLayoutProbe.skipped,unskipped:__rowLayoutProbe.unskipped}));''')
        self.assertEqual(value,dict(css=p.SELECTOR+'{content-visibility:auto;contain-intrinsic-block-size:auto 36px}',events=2,skipped=1,unskipped=1))

    def test_disabled_probe_does_not_insert_css(self):
        script=p.INSTALL.replace('SELECTOR',json.dumps(p.SELECTOR)).replace('ENABLE','false')
        value=node('''globalThis.window=globalThis;globalThis.CSS={supports:()=>false};
          globalThis.document={addEventListener:()=>{},createElement:()=>{throw Error('unexpected style')}};
          const value=new Function('''+json.dumps(script)+''')();console.log(JSON.stringify(value));''')
        self.assertFalse(value['supportsAuto'])
        self.assertEqual(value['events'],0)

    def frame_boot(self):
        return '''globalThis.window=globalThis;let callback;
          globalThis.performance={now:()=>123};
          window.requestAnimationFrame=cb=>{callback=cb;return 71;};
          class HTMLElement {get offsetTop(){return 42;}}
          globalThis.HTMLElement=HTMLElement;
          new Function('''+json.dumps(f.INSTALL)+''')();
          const row=new HTMLElement();row.tagName='LI';'''

    def test_frame_probe_preserves_callback_id_offsets_and_timestamp_order(self):
        value=node(self.frame_boot()+'''
          const id=requestAnimationFrame(timestamp=>{row.offsetTop;return timestamp;});
          const returned=callback(88);
          window.__rowReadingProbe=true;const offset=row.offsetTop;window.__rowReadingProbe=false;
          console.log(JSON.stringify({id,offset,returned,current:__rowCurrentRAF,events:__rowFrameEvents}));''')
        self.assertEqual(value,dict(id=71,offset=42,returned=88,current=None,events=[dict(kind='productionGeometry',now=123,timestamp=88,top=42)]))

    def test_frame_probe_bounds_records_without_owning_nodes_and_resets_on_error(self):
        value=node(self.frame_boot()+'''
          for(let i=0;i<2500;i++)row.offsetTop;
          requestAnimationFrame(()=>{throw Error('fixture')});
          try{callback(89);}catch{}
          console.log(JSON.stringify({count:__rowFrameEvents.length,current:__rowCurrentRAF,
           fields:Object.keys(__rowFrameEvents[0]),outside:__rowFrameEvents[0].timestamp}));''')
        self.assertEqual(value,dict(count=2000,current=None,fields=['kind','now','timestamp','top'],outside=None))


if __name__=='__main__':unittest.main()
