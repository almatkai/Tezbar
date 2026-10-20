import { request } from './host'
import type { InstalledApplication } from '../search/providers/appsProvider'

export type { InstalledApplication }
let applications: InstalledApplication[] = []

export async function refreshHostSnapshot(): Promise<void> {
  applications = await request<InstalledApplication[]>('core:apps:list')
}

export function listApplications(): InstalledApplication[] {
  return applications
}

export function readAppBundleIdentifier(path: string): string | undefined {
  return applications.find((application) => application.path === path)?.bundleId
}

export async function askExtensionAI(prompt: string): Promise<string> {
  return request<string>('core:ai:extension-ask', prompt)
}

export async function readSelectedText(): Promise<string> {
  return request<string>('core:platform:selected-text')
}
