#!/usr/bin/env python3
"""Actual WKWebView geometry transitions; synthetic replies, never acceptance timing.

Native requests still finish, but their replies are replaced at the bridge seam
with labelled fixture rows to force same-sized replacement/reorder/empty cases.
Bounds reads force layout; callback observations are not pixel-paint evidence.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import tempfile
import time

spec = importlib.util.spec_from_file_location('row_frames', Path(__file__).with_name('row-layout-frames.py'))
f = importlib.util.module_from_spec(spec)
spec.loader.exec_module(f)
r = f.r


def validate_steps(steps):
    expected = [['Geometry a', 'Geometry b'], ['Geometry fresh', 'Geometry b'],
                ['Geometry b', 'Geometry fresh'], ['Geometry b', 'Geometry fresh'],
                [], ['Geometry new', 'Geometry last']]
    assert len(steps) == len(expected)
    for index, ((name, step), titles) in enumerate(zip(steps.items(), expected)):
        assert step['identity']['titles'] == titles, name
        assert step['identity']['calls'] == index + 1, name
        for frame in step['frames']:
            if frame.get('empty'):
                continue
            assert frame['top'] == frame['highlightTop'], (name, frame)
            assert frame['height'] == frame['highlightHeight'], (name, frame)
            assert frame['focus'], name
        if name == 'empty':
            assert step['frames'][-1].get('empty'), name
        elif name == 'height-change':
            assert step['frames'][-1]['height'] == 71, name
        else:
            assert step['frames'][-1]['height'] == 36, name
        if name in ['same-size-replacement', 'reorder']:
            assert not step['identity']['same'] and not step['identity']['oldConnected'], name


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--app', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    r.require_unlocked_display()
    a.output.mkdir(parents=True, exist_ok=False)
    root = Path(tempfile.mkdtemp(prefix='tb-geometry-frames-'))
    r.prepare(root)
    d = r.packaged(a.app.resolve(), root)
    result = dict(app=str(a.app.resolve()), root=str(root), diagnosticOnly=True,
                  syntheticReplies=True, completed=False, steps={})
    try:
        d.launch(a.app.resolve() / 'Contents/MacOS/tezbar')
        d.sample('initial')
        d.wait(lambda: d.request('core:launcher:status')['status'] == 'ready')
        if time.monotonic() - d.started < 12:
            time.sleep(12 - (time.monotonic() - d.started))
        r.b.f.visible(d)
        d.js(r.LEAN_INSTALL)
        d.js(f.INSTALL)
        d.js('''const native=window.tezbar.searchAll;
        window.__geometryFixture={rows:[],calls:0};
        window.tezbar.searchAll=async function(query){
          const rows=await native(query);
          if(!query.startsWith('geometry-fixture-')&&query!=='zx')return rows;
          window.__geometryFixture.calls++;
          return window.__geometryFixture.rows.map((key,index)=>({
            id:'geometry:'+key,title:'Geometry '+key,subtitle:query+' Synthetic transition fixture',
            category:'files',score:100-index,action:{type:'open-file',path:'/synthetic/'+key+'.txt'}
          }));
        };return true;''')
        cases = [('initial', ['a', 'b'], ''),
                 ('same-size-replacement', ['fresh', 'b'], ''),
                 ('reorder', ['b', 'fresh'], ''),
                 ('height-change', ['b', 'fresh'], "const style=document.createElement('style');style.id='geometry-height';style.textContent='li:has(>button.list-row[data-tone=accent]){min-height:71px}';document.head.append(style);"),
                 ('empty', [], ''),
                 ('show-after-empty', ['new', 'last'], "document.querySelector('#geometry-height').remove();")]
        for index, (name, rows, before) in enumerate(cases):
            # Save only one identity between cases; no diagnostic row/trace arrays
            # enter acceptance apps or the ordinary lean CPU/RAM workload.
            action = before + 'window.__geometryFixture.rows=' + json.dumps(rows) + ';'
            query = 'zx' if name == 'empty' else 'geometry-fixture-' + str(index)
            action += 'window.__inputBench.input(' + json.dumps(query) + ',' + json.dumps(query) + ');'
            value = d.js(f.FRAMES.replace('ACTION', action))
            value['identity'] = d.js('''const row=document.querySelector('button.list-row[data-tone="accent"][data-selected="true"]')?.parentElement;
              const old=window.__geometryOldRow;
              const value={same:old===row,oldConnected:old?.isConnected??null,
                titles:Array.from(document.querySelectorAll('button.list-row[data-tone="accent"] .list-row__title')).map(e=>e.textContent),
                calls:window.__geometryFixture.calls};
              window.__geometryOldRow=row;return value;''')
            result['steps'][name] = value
            d.sample('after-' + name)
        validate_steps(result['steps'])
        result['trustedInputEvents'] = d.js('return window.__inputBench.trustedInputEvents;')
        assert result['trustedInputEvents'] == 0, 'physical input interference'
        result['completed'] = True
    finally:
        try:
            if d.pid:
                try:
                    d.js("delete window.__geometryOldRow;await window.__TAURI_INTERNALS__.invoke('set_suppress_blur_hide',{value:false});return true;")
                finally:
                    d.close()
        finally:
            result['survivors'] = r.processes(d.pid) if d.pid else []
            result['samples'] = d.samples
            (a.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
