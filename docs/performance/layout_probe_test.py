"""Diagnostic probe API preservation and lazy layout-read timing, using real JS."""
import importlib.util
import json
from pathlib import Path
import subprocess
import unittest

spec = importlib.util.spec_from_file_location('layout_probe', Path(__file__).with_name('layout-probe.py'))
p = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)

BOOT = r'''
globalThis.window=globalThis;let clock=0;
globalThis.performance={now:()=>clock};
class Element {scrollIntoView(options){clock+=3;return options.block;}}
class HTMLElement extends Element {
  get offsetTop(){clock+=7;return 42;}
  get offsetHeight(){clock+=2;return 76;}
}
class CanvasRenderingContext2D {measureText(text){clock+=5;return {width:text.length*8};}}
Object.assign(globalThis,{Element,HTMLElement,CanvasRenderingContext2D});
const style={get fontWeight(){clock+=11;return '500';},fontSize:'15px',fontFamily:'fixture'};
window.getComputedStyle=()=>{clock+=1;return style;};
new Function(PROBE)();
'''


def node(script):
    source = 'const PROBE=' + json.dumps(p.INSTALL) + ';' + BOOT + script
    result = subprocess.run(['node', '--input-type=module', '-e', source], text=True,
                            capture_output=True, check=True, timeout=10)
    return json.loads(result.stdout)


class LayoutProbeTests(unittest.TestCase):
    def test_times_lazy_font_read_and_preserves_computed_values(self):
        value = node("const s=getComputedStyle({id:'command-input'});const weight=s.fontWeight;console.log(JSON.stringify({weight,stats:__layoutProbe}));")
        self.assertEqual(value['weight'], '500')
        self.assertEqual(value['stats']['input.getComputedStyle']['ms'], 1)
        self.assertEqual(value['stats']['input.style.fontWeight']['ms'], 11)

    def test_preserves_offsets_text_metrics_and_scroll_arguments(self):
        value = node("const row=new HTMLElement();row.tagName='LI';const top=row.offsetTop,height=row.offsetHeight;const scroll=row.scrollIntoView({block:'nearest'});const width=new CanvasRenderingContext2D().measureText('abc').width;console.log(JSON.stringify({top,height,scroll,width,stats:__layoutProbe}));")
        self.assertEqual({k:v for k,v in value.items() if k!='stats'},
                         dict(top=42,height=76,scroll='nearest',width=24))
        for name,ms in [('row.offsetTop',7),('row.offsetHeight',2),('scrollIntoView',3),('measureText',5)]:
            self.assertEqual(value['stats'][name]['count'], 1)
            self.assertEqual(value['stats'][name]['ms'], ms)


if __name__=='__main__': unittest.main()
