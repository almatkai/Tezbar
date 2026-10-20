import { execFileSync } from 'node:child_process'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { macClipboardProcessEnv } from './desktop-runtime'

// Opt-in native round trip. Never touches the general/user clipboard; preserve
// every type on the separate ruler pasteboard and restore it even on failure.
const enabled = process.platform === 'darwin' && process.env.TEZBAR_TEST_MAC_CLIPBOARD === '1'
const text = 'Привет Қазақша العربية 中文 日本語 한국어 हिन्दी 😀 👩🏽‍💻 e\u0301\nsecond line\n'
const pasteboard =
  'ObjC.import("AppKit"); ObjC.import("Foundation"); const pb=$.NSPasteboard.pasteboardWithName($.NSPasteboardNameRuler); '
function runJavaScript(script: string): string {
  return execFileSync('/usr/bin/osascript', ['-l', 'JavaScript', '-e', pasteboard + script], {
    encoding: 'utf8',
    timeout: 5_000,
  })
}

describe.skipIf(!enabled)('native macOS clipboard Unicode', () => {
  afterEach(() => vi.unstubAllEnvs())

  it.each(['C', undefined])('round trips all alphabets with inherited locale %s', (locale) => {
    const saved = JSON.parse(
      runJavaScript(
        [
          'const saved=[]; const items=pb.pasteboardItems;',
          'if(items){for(let i=0;i<items.count;i++){const item=items.objectAtIndex(i);const data={};',
          'for(let j=0;j<item.types.count;j++){const type=item.types.objectAtIndex(j);',
          'data[ObjC.unwrap(type)]=ObjC.unwrap(item.dataForType(type).base64EncodedStringWithOptions(0));}',
          'saved.push(data);}} JSON.stringify(saved);',
        ].join(' ')
      )
    )
    try {
      for (const key of ['LANG', 'LC_CTYPE', 'LC_ALL']) vi.stubEnv(key, locale)
      const env = macClipboardProcessEnv()
      execFileSync('/usr/bin/pbcopy', ['-pboard', 'ruler'], {
        input: Buffer.from(text, 'utf8'),
        env,
        timeout: 5_000,
      })
      const actual = execFileSync('/usr/bin/pbpaste', ['-pboard', 'ruler'], {
        encoding: 'utf8',
        env,
        timeout: 5_000,
      })
      expect(actual).toBe(text)
    } finally {
      runJavaScript(
        [
          `const saved=${JSON.stringify(saved)}; pb.clearContents;`,
          'const items=$.NSMutableArray.array;',
          'for(const obj of saved){const item=$.NSPasteboardItem.alloc.init;',
          'for(const type in obj){item.setDataForType($.NSData.alloc.initWithBase64EncodedStringOptions($(obj[type]),0),$(type));}',
          'items.addObject(item);} if(items.count)pb.writeObjects(items);',
        ].join(' ')
      )
    }
  })
})
