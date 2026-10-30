"""Geometry fixture verdicts and bounded diagnostic callback ownership."""
import importlib.util
import json
from pathlib import Path
import subprocess
import unittest


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


f = load('geometry-frames')
t = load('geometry-trajectory')


def node(code):
    result = subprocess.run(['node', '--input-type=module', '-e', code],
                            text=True, capture_output=True, check=True, timeout=10)
    return json.loads(result.stdout)


class GeometryTests(unittest.TestCase):
    def fixture(self):
        names = ['initial', 'same-size-replacement', 'reorder', 'height-change', 'empty', 'show-after-empty']
        titles = [['Geometry a', 'Geometry b'], ['Geometry fresh', 'Geometry b'],
                  ['Geometry b', 'Geometry fresh'], ['Geometry b', 'Geometry fresh'],
                  [], ['Geometry new', 'Geometry last']]
        return {name: dict(identity=dict(titles=rows, calls=i + 1, same=False, oldConnected=False),
                           frames=[dict(empty=True)] if not rows else [dict(
                               top=6, highlightTop=6, height=71 if i == 3 else 36,
                               highlightHeight=71 if i == 3 else 36, focus=True)])
                for i, (name, rows) in enumerate(zip(names, titles))}

    def test_fixture_accepts_complete_transition_contract(self):
        f.validate_steps(self.fixture())

    def test_fixture_rejects_wrong_order_missing_native_call_and_late_geometry(self):
        for field, value in [('titles', ['Wrong']), ('calls', 1)]:
            fixture = self.fixture()
            fixture['reorder']['identity'][field] = value
            with self.assertRaises(AssertionError):
                f.validate_steps(fixture)
        fixture = self.fixture()
        fixture['same-size-replacement']['frames'][0]['highlightTop'] = 0
        with self.assertRaises(AssertionError):
            f.validate_steps(fixture)

    def test_counters_preserve_microtasks_errors_and_drain_without_storing_callbacks(self):
        value = node('''globalThis.window=globalThis;
          const tasks=[];window.queueMicrotask=callback=>tasks.push(callback);
          new Function(''' + json.dumps(t.COUNTERS) + ''')();
          let calls=0;queueMicrotask(()=>{calls++;});
          queueMicrotask(()=>{calls++;throw Error('fixture')});
          let errors=0;while(tasks.length){try{tasks.shift()();}catch{errors++;}}
          console.log(JSON.stringify({calls,errors,state:__geometryQueue}));''')
        self.assertEqual(value, dict(calls=2, errors=1, state=dict(
            scheduled=2, finished=2, pending=0, maxPending=2)))

    def test_quiet_read_is_safe_before_install_and_returns_counts_not_nodes(self):
        value = node('''globalThis.window=globalThis;
          const read=new Function(''' + json.dumps(t.READ) + ''');
          const before=read();window.__geometryQueue={pending:0};
          window.__inputBench={active:0};
          globalThis.document={querySelectorAll:()=>({length:7})};
          console.log(JSON.stringify({before,after:read()}));''')
        self.assertEqual(value, dict(before=dict(notInstalled=True), after=dict(
            queue=dict(pending=0), domNodes=7, rows=7, highlights=7,
            bridgeActive=0, ownsFinalReply=False)))


if __name__ == '__main__':
    unittest.main()
