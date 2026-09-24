import { mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { listApplications, readAppBundleIdentifier } from './appsProvider'

describe('appsProvider and bundle identifiers', () => {
  let testDir: string

  beforeEach(() => {
    testDir = join(tmpdir(), `apps-provider-test-${Date.now()}-${Math.random().toString(36).slice(2)}`)
    mkdirSync(testDir, { recursive: true })
  })

  afterEach(() => {
    rmSync(testDir, { recursive: true, force: true })
  })

  it('reads CFBundleIdentifier from XML Info.plist', () => {
    if (process.platform !== 'darwin') return
    const appDir = join(testDir, 'Sample.app')
    const contentsDir = join(appDir, 'Contents')
    mkdirSync(contentsDir, { recursive: true })

    const xmlPlist = `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>com.example.SampleApp</string>
  <key>CFBundleName</key>
  <string>Sample</string>
</dict>
</plist>`
    writeFileSync(join(contentsDir, 'Info.plist'), xmlPlist, 'utf8')

    const bundleId = readAppBundleIdentifier(appDir)
    expect(bundleId).toBe('com.example.SampleApp')
  })

  it('returns undefined when Info.plist does not exist', () => {
    if (process.platform !== 'darwin') return
    const appDir = join(testDir, 'NonExistent.app')
    expect(readAppBundleIdentifier(appDir)).toBeUndefined()
  })

  it('reads bundleId for Amphetamine if installed', () => {
    if (process.platform !== 'darwin') return
    const amphetamineBundleId = readAppBundleIdentifier('/Applications/Amphetamine.app')
    if (amphetamineBundleId) {
      expect(amphetamineBundleId).toBe('com.if.Amphetamine')
    }
  })

  it('listApplications returns application entries with bundleId on macOS', () => {
    const apps = listApplications()
    expect(Array.isArray(apps)).toBe(true)
    if (process.platform === 'darwin' && apps.length > 0) {
      const amp = apps.find((a) => a.name === 'Amphetamine' || a.bundleId === 'com.if.Amphetamine')
      if (amp) {
        expect(amp.bundleId).toBe('com.if.Amphetamine')
      }
      // At least some apps on macOS should have bundleId resolved
      const withBundleId = apps.filter((a) => Boolean(a.bundleId))
      expect(withBundleId.length).toBeGreaterThan(0)
    }
  })
})
