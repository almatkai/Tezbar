import { existsSync, readdirSync, readFileSync } from 'node:fs'
import { homedir } from 'node:os'
import path from 'node:path'

export interface PiExtensionItem {
  id: string
  name: string
  version: string
  description: string
  type: 'package' | 'script'
  location: string
  enabled: boolean
  features?: string[]
}

export function getInstalledPiExtensions(): PiExtensionItem[] {
  const agentDir = path.join(homedir(), '.pi', 'agent')
  const settingsPath = path.join(agentDir, 'settings.json')
  const nodeModulesDir = path.join(agentDir, 'npm', 'node_modules')
  const extensionsDir = path.join(agentDir, 'extensions')

  const results: PiExtensionItem[] = []
  const seenIds = new Set<string>()

  // 1. Packages declared in ~/.pi/agent/settings.json
  if (existsSync(settingsPath)) {
    try {
      const settings = JSON.parse(readFileSync(settingsPath, 'utf8')) as { packages?: string[] }
      const packages = Array.isArray(settings.packages) ? settings.packages : []
      for (const pkg of packages) {
        const pkgName = pkg.replace(/^npm:/, '')
        if (seenIds.has(pkgName)) continue
        seenIds.add(pkgName)

        const pkgJsonPath = path.join(nodeModulesDir, pkgName, 'package.json')
        if (existsSync(pkgJsonPath)) {
          try {
            const pkgJson = JSON.parse(readFileSync(pkgJsonPath, 'utf8')) as {
              name?: string
              version?: string
              description?: string
              keywords?: string[]
            }
            results.push({
              id: pkgName,
              name: pkgJson.name || pkgName,
              version: pkgJson.version ? `v${pkgJson.version}` : 'installed',
              description: pkgJson.description || 'Pi agent package',
              type: 'package',
              location: path.join(nodeModulesDir, pkgName),
              enabled: true,
              features: getPackageFeatures(pkgName, pkgJson.description),
            })
            continue
          } catch {
            /* fall through to default entry */
          }
        }

        results.push({
          id: pkgName,
          name: pkgName,
          version: 'installed',
          description: 'Pi agent package',
          type: 'package',
          location: pkg,
          enabled: true,
          features: getPackageFeatures(pkgName),
        })
      }
    } catch {
      /* ignore settings parse errors */
    }
  }

  // 2. Local scripts inside ~/.pi/agent/extensions/
  if (existsSync(extensionsDir)) {
    try {
      const files = readdirSync(extensionsDir)
      for (const file of files) {
        if (file.endsWith('.ts') || file.endsWith('.js')) {
          const id = path.basename(file, path.extname(file))
          if (seenIds.has(id)) continue
          seenIds.add(id)

          const scriptPath = path.join(extensionsDir, file)
          results.push({
            id,
            name: id.replace(/[-_]/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase()),
            version: 'local',
            description: `Custom extension script (${file})`,
            type: 'script',
            location: scriptPath,
            enabled: true,
            features: getScriptFeatures(id),
          })
        }
      }
    } catch {
      /* ignore read errors */
    }
  }

  return results
}

function getPackageFeatures(id: string, description?: string): string[] {
  switch (id) {
    case 'pi-antigravity':
      return ['Google Cloud Code Provider', 'Image generation (generate_image)', 'Claude & Gemini models']
    case 'pi-agents':
      return ['Multi-agent orchestration', 'Parallel & sequence workflows', 'Subagent spawning', 'Mermaid diagrams']
    case 'pi-multi-account':
      return ['Multi-account rotation', 'Automatic rate-limit failover', 'OAuth account cycling']
    case 'pi-notify':
      return ['OSC / native desktop notifications', 'Long-running turn alerts']
    case 'pi-reset-usage-limit':
      return ['Codex usage limit credit resets']
    default:
      if (description?.toLowerCase().includes('provider')) return ['Custom AI provider']
      return []
  }
}

function getScriptFeatures(id: string): string[] {
  if (id.includes('modal')) return ['Modal GLM-5.3-Flash provider', 'Serverless inference endpoint']
  return ['Custom extension hooks and tools']
}
