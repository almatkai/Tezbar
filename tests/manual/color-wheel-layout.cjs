// Browser regression using the real form/detail components and generated app CSS.
// Run: node tests/manual/color-wheel-layout.cjs
// Set PLAYWRIGHT_MODULE when Playwright is installed outside this checkout.
// Set PLAYWRIGHT_BROWSER=webkit to check the macOS renderer engine.
const assert = require('node:assert/strict')
const { mkdtempSync, writeFileSync, rmSync } = require('node:fs')
const { tmpdir } = require('node:os')
const { resolve, join } = require('node:path')
const { pathToFileURL } = require('node:url')
const { execFileSync } = require('node:child_process')
const { build } = require('esbuild')
const playwright = require(process.env.PLAYWRIGHT_MODULE || 'playwright')
const browserType = playwright[process.env.PLAYWRIGHT_BROWSER || 'chromium']

const root = resolve(__dirname, '../..')
const dir = mkdtempSync(join(tmpdir(), 'tezbar-color-wheel-'))

async function main() {
  await build({
    stdin: {
      contents: `
        import React from 'react'
        import { createRoot } from 'react-dom/client'
        import { FormRuntime } from './src/renderer/src/raycast-api/form-runtime'
        import { DetailRuntime } from './src/renderer/src/raycast-api/detail-runtime'
        import { buildColorFormatRows } from './src/renderer/colorConverter'
        const formats = buildColorFormatRows({r:255, g:0, b:0, a:1, source:'rgb'})
        const noop = () => {}
        window.tezbar = {clipboardWriteText: async value => {window.copied = value}}
        const props = {title:'Color Wheel', onBack:noop, onSubmitForm:noop,
          onChangeField:noop, onInvokeAction:(id, value) => {window.lastAction = {id, value}},
          onOpenActions:noop, onRunPrimaryAction:noop}
        const nativeRoot = {type:'Form', props:{nativeColorWheel:{hue:0, saturation:100,
          value:100, alpha:100, preview:'#FF0000', changeActionId:'change', saveActionId:'save',
          formats:formats.map(row => ({...row, actionId:row.key}))}}, children:[
          {type:'Form.TextField', props:{id:'hue', title:'Hue (degrees)', value:'0'}},
          {type:'Form.TextField', props:{id:'saturation', title:'Saturation (%)', value:'100'}}]}
        const detailRoot = {type:'Detail', props:{markdown:'![RGB Color Wheel](rgb-color-wheel.webp?&raycast-height=350)'}}
        const detail = new URLSearchParams(location.search).get('view') === 'detail'
        createRoot(document.getElementById('root')).render(
          <div className="glass-shell h-screen w-full">
            {detail ? <DetailRuntime {...props} root={detailRoot}/> : <FormRuntime {...props} root={nativeRoot}/>}
          </div>
        )
      `,
      loader: 'tsx',
      resolveDir: root,
    },
    outfile: join(dir, 'app.js'), bundle: true, jsx: 'automatic', logLevel: 'warning',
  })
  execFileSync(process.execPath, [require.resolve('tailwindcss/lib/cli.js'), '-i', 'src/renderer/styles.css', '-o', join(dir, 'style.css')], {cwd: root, stdio: 'pipe'})
  writeFileSync(join(dir, 'index.html'), '<html><head><link rel="stylesheet" href="style.css"></head><body><div id="root"></div><script src="app.js"></script></body></html>')
  const browser = await browserType.launch({headless: true})
  const failures = []
  try {
    for (const view of ['native', 'detail']) for (const width of [960, 640, 520]) for (const height of [580, 540]) {
      const page = await browser.newPage({viewport:{width, height}})
      page.setDefaultTimeout(5000)
      try {
        await page.goto(pathToFileURL(join(dir, 'index.html')).href + '?view=' + view)
        const wheel = page.getByRole('button', {name: view === 'native' ? 'Color wheel' : 'RGB Color Wheel', exact:true})
        await wheel.waitFor()
        const boxes = await wheel.evaluate(button => {
          const panel = button.parentElement.parentElement
          const grid = panel.parentElement
          const formats = grid.querySelector('aside').lastElementChild
          return {
            wheel:button.getBoundingClientRect().toJSON(), panel:panel.getBoundingClientRect().toJSON(),
            grid:grid.getBoundingClientRect().toJSON(), formats:formats.getBoundingClientRect().toJSON(),
            formatScrollHeight:formats.scrollHeight, documentWidth:document.documentElement.scrollWidth,
            panelBorder:getComputedStyle(panel).borderTopColor,
            footer:document.querySelector(".command-surface-footer").getBoundingClientRect().toJSON(),
            scrollTop:button.closest(".command-surface-body").scrollTop,
          }
        })
        if (process.env.DEBUG_LAYOUT) console.log(view + ' ' + width + '×' + height + ' ' + JSON.stringify(boxes))
        assert(Math.abs(boxes.wheel.width - boxes.wheel.height) < 1, `wheel must be square (${boxes.wheel.width}×${boxes.wheel.height})`)
        assert.equal(boxes.scrollTop, 0, 'wheel must be usable without first scrolling the form')
        assert.equal(boxes.panelBorder, 'rgba(255, 255, 255, 0.08)', 'wheel panel border must use the intended translucent color')
        assert(boxes.wheel.bottom <= boxes.footer.top, 'wheel must stay above the footer')
        assert(boxes.wheel.width >= 120, 'wheel must remain usable at launcher widths')
        assert(boxes.wheel.top >= boxes.panel.top && boxes.wheel.bottom <= boxes.panel.bottom, 'wheel must fit its panel')
        assert(Math.abs((boxes.wheel.top + boxes.wheel.bottom) - (boxes.panel.top + boxes.panel.bottom)) < 1, 'wheel must be vertically centered')
        assert(boxes.panel.bottom <= boxes.grid.bottom + 1, 'format content must not expand the grid track')
        assert(boxes.formats.bottom <= boxes.grid.bottom + 1, 'formats must stay inside the grid')
        assert(boxes.formatScrollHeight > boxes.formats.height, 'long formats must scroll in their own pane')
        assert(boxes.documentWidth <= width, 'layout must not overflow horizontally')
        // Pointer coordinates must match the visible circle after sizing.
        await page.mouse.click(boxes.wheel.x + boxes.wheel.width / 2, boxes.wheel.y + boxes.wheel.height / 2)
        if (view === 'native') {
          await page.waitForFunction(() => window.lastAction?.id === 'change')
          const action = await page.evaluate(() => window.lastAction)
          // WebKit rounds injected pointer coordinates to device pixels.
          const tolerance = 2 * Math.SQRT2 / (boxes.wheel.width / 2) * 100
          assert(action.value.saturation < tolerance, `center of the circle must select near-zero saturation (${action.value.saturation})`)
        } else {
          const hexButton = page.getByRole('button', {name:/^HEX #[0-9a-f]{6}/i})
          const selectedHex = (await hexButton.textContent()).match(/#[0-9a-f]{6}/i)[0]
          assert(selectedHex.slice(1).match(/../g).every(channel => parseInt(channel, 16) >= 247), `center must select near-white (${selectedHex})`)
          await hexButton.click()
          assert.equal(await page.evaluate(() => window.copied), selectedHex)
        }
        console.log('PASS ' + view + ' ' + width + '×' + height)
      } catch (error) {
        console.error('FAIL ' + view + ' ' + width + '×' + height + ': ' + error.message)
        failures.push(view + ' ' + width + '×' + height + ': ' + error.message)
      } finally { await page.close() }
    }
  } finally { await browser.close() }
  assert.equal(failures.length, 0, failures.join('\n'))
}
main().catch(error => {console.error(error); process.exitCode = 1}).finally(() => rmSync(dir, {recursive:true, force:true}))
