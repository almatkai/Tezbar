// Standalone browser regression for the real CommandBar and generated Tailwind CSS.
// Requires Playwright with Chromium and WebKit installed; no native backend/server.
// Run: node tests/manual/pinned-trash-layout.cjs
// Or set PLAYWRIGHT_MODULE to an existing Playwright module path.
const assert = require('node:assert/strict')
const { mkdtempSync, writeFileSync, rmSync } = require('node:fs')
const { tmpdir } = require('node:os')
const { resolve, join } = require('node:path')
const { pathToFileURL } = require('node:url')
const { execFileSync } = require('node:child_process')
const { build } = require('esbuild')
const { chromium, webkit } = require(process.env.PLAYWRIGHT_MODULE || 'playwright')

const root = resolve(__dirname, '../..')
const dir = mkdtempSync(join(tmpdir(), 'tezbar-pinned-trash-'))

async function main() {
  await build({
    stdin: {
      contents: `
        import React from 'react'
        import { createRoot } from 'react-dom/client'
        import CommandBar from './src/renderer/CommandBar'
        const count = Number(new URLSearchParams(location.search).get('pins'))
        localStorage.clear()
        localStorage.setItem('tezbar:pinned-commands', JSON.stringify(
          Array.from({length: count}, (_, i) => ({
            id: 'fixture' + i, title: 'Command ' + i, subtitle: 'Fixture',
            category: 'commands', action: {type: 'native-command', commandId: 'clipboard-history'}, slot: i + 1,
          }))
        ))
        window.tezbar = new Proxy({}, {get: (_, key) => key.startsWith('on')
          ? () => () => {}
          : () => Promise.resolve(key === 'getUpdateStatus' ? {kind: 'idle'} : key === 'getLlmConfig' ? {} : [])})
        createRoot(document.getElementById('root')).render(
          <div className="glass-shell flex h-screen w-full p-2"><CommandBar hasUnreadReleaseNotes={false} /></div>
        )
      `,
      loader: 'tsx',
      resolveDir: root,
    },
    outfile: join(dir, 'app.js'),
    bundle: true,
    jsx: 'automatic',
    define: {
      'import.meta.env.VITE_APP_RELEASE_NOTES': '""',
      'import.meta.env.VITE_APP_VERSION': '"test"',
    },
    logLevel: 'warning',
  })
  execFileSync(process.execPath, [require.resolve('tailwindcss/lib/cli.js'), '-i', 'src/renderer/styles.css', '-o', join(dir, 'style.css')], {cwd: root, stdio: 'pipe'})
  writeFileSync(join(dir, 'index.html'), '<html><head><link rel="stylesheet" href="style.css"></head><body><div id="root"></div><script src="app.js"></script></body></html>')
  for (const engine of [chromium, webkit]) {
    const browser = await engine.launch({headless: true})
    try {
      for (const theme of ['white', 'dark']) for (const height of [420, 650]) for (const count of [1, 12]) {
        const page = await browser.newPage({viewport: {width: 900, height}})
        const errors = []
        page.on('pageerror', error => errors.push(error.message))
        await page.goto(pathToFileURL(join(dir, 'index.html')).href + '?pins=' + count)
        await page.evaluate(theme => document.documentElement.dataset.theme = theme, theme)
        const rail = page.locator('.tezbar-pin-rail')
        const first = page.locator('[data-pin-index="0"]')
        await first.waitFor()
        // Wait for the rail's entrance transform to settle before measuring.
        await page.waitForTimeout(250)
        const start = await first.boundingBox()
        await page.mouse.move(start.x + 18, start.y + 18)
        await page.mouse.down()
        const boxes = await rail.evaluate(r => ({
          rail: r.getBoundingClientRect().toJSON(),
          list: r.firstElementChild.getBoundingClientRect().toJSON(),
          trash: r.lastElementChild.getBoundingClientRect().toJSON(),
          footer: r.parentElement.nextElementSibling.getBoundingClientRect().toJSON(),
        }))
        const label = `${engine.name()} ${theme} ${height}px ${count} pins`
        assert(boxes.trash.bottom <= boxes.rail.bottom, label + ': trash outside rail')
        assert(boxes.trash.bottom <= boxes.footer.top, label + ': trash behind hotkeys')
        assert(boxes.list.bottom <= boxes.trash.top, label + ': pins overlap trash')
        await page.mouse.move(boxes.trash.x + 21, boxes.trash.y + 16, {steps: 5})
        await page.waitForFunction(() => document.querySelector('.tezbar-pin-rail').lastElementChild.className.includes('bg-rose-400/15'))
        await page.mouse.up()
        await page.waitForFunction(count => document.querySelectorAll('[data-pin-index]').length === count - 1, count)
        assert.equal(errors.length, 0, errors.join('\n'))
        console.log('PASS ' + label)
        await page.close()
      }
    } finally {
      await browser.close()
    }
  }
}
main().catch(error => { console.error(error); process.exitCode = 1 }).finally(() => rmSync(dir, {recursive: true, force: true}))
