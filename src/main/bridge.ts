import { spawn } from 'node:child_process'
import { existsSync } from 'node:fs'
import { createRequire } from 'node:module'
import path from 'node:path'

export type MouseButton = 'left' | 'right';
export type KeyModifier = 'cmd' | 'command' | 'ctrl' | 'control' | 'shift' | 'opt' | 'option' | 'alt';

type InputAddon = {
  moveMouse: (x: number, y: number) => void;
  click: (x: number, y: number, button: MouseButton) => void;
  doubleClick: (x: number, y: number) => void;
  typeText: (text: string) => void;
  pressKey: (key: string, mods: string[]) => void;
  scroll: (x: number, y: number, dx: number, dy: number) => void;
  screenshot: () => Buffer;
  /** Present after rebuilding `native/input` with CGEventSourceKeyState. */
  isPhysicalKeyDown?: (key: string) => boolean;
};

export type AXFrame = { x: number; y: number; w: number; h: number };
export type AXElement = {
  role?: string;
  label?: string;
  value?: string;
  frame?: AXFrame;
  children: AXElement[];
};

const addonPath = path.resolve(__dirname, '../../native/input/index.node')
const axHelperPath = process.env.AXHELPER_PATH || path.resolve(__dirname, '../../native/axhelper/axhelper')

const requireNative = createRequire(__filename)

/** `undefined` = not loaded yet, `null` = missing or failed load. */
let inputAddon: InputAddon | null | undefined

function getInputAddon(): InputAddon | null {
  if (inputAddon !== undefined) return inputAddon
  if (!existsSync(addonPath)) {
    console.warn(
      `[tezbar] Native input addon not found (${addonPath}). Screen/control helpers and Alt+Space hold-to-speak key polling are disabled. Build with: pnpm build:native:input`,
    )
    inputAddon = null
    return null
  }
  try {
    inputAddon = requireNative(addonPath) as InputAddon
    return inputAddon
  } catch (err) {
    console.warn('[tezbar] Failed to load native input addon:', err)
    inputAddon = null
    return null
  }
}

function requireInputAddon(): InputAddon {
  const mod = getInputAddon()
  if (!mod) {
    throw new Error(
      `Tezbar native input is not available (missing or failed to load: ${addonPath}). Run \`pnpm build:native:input\` from the repository root.`,
    )
  }
  return mod
}

function runAxHelper(payload: { action: 'snapshot' | 'find'; query?: string }): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const child = spawn(axHelperPath, [], {
      stdio: ['pipe', 'pipe', 'pipe'],
    });

    let stdout = '';
    let stderr = '';

    child.stdout.on('data', (chunk: Buffer) => {
      stdout += chunk.toString('utf8');
    });

    child.stderr.on('data', (chunk: Buffer) => {
      stderr += chunk.toString('utf8');
    });

    child.on('error', reject);
    child.on('close', (code) => {
      if (code !== 0) {
        reject(new Error(`axhelper failed (code ${code}): ${stderr || stdout}`));
        return;
      }

      try {
        resolve(JSON.parse(stdout));
      } catch (err) {
        reject(new Error(`invalid axhelper json: ${(err as Error).message}; output=${stdout}`));
      }
    });

    child.stdin.end(JSON.stringify(payload));
  });
}

export function moveMouse(x: number, y: number): void {
  requireInputAddon().moveMouse(x, y)
}

export function click(x: number, y: number, button: MouseButton): void {
  requireInputAddon().click(x, y, button)
}

export function doubleClick(x: number, y: number): void {
  requireInputAddon().doubleClick(x, y)
}

export function typeText(text: string): void {
  requireInputAddon().typeText(text)
}

export function pressKey(key: string, mods: KeyModifier[] = []): void {
  requireInputAddon().pressKey(key, mods)
}

export function scroll(x: number, y: number, dx: number, dy: number): void {
  requireInputAddon().scroll(x, y, dx, dy)
}

export function screenshot(): Buffer {
  return requireInputAddon().screenshot()
}

/** macOS only: reads HID key state (not available on older `index.node` builds). */
export function isPhysicalKeyDown(key: string): boolean {
  if (process.platform !== 'darwin') return false
  const mod = getInputAddon()
  if (!mod) return false
  const fn = mod.isPhysicalKeyDown
  if (typeof fn !== 'function') return false
  try {
    return fn(key) === true
  } catch {
    return false
  }
}

export async function getScreenSnapshot(): Promise<{ elements: AXElement[] }> {
  const out = (await runAxHelper({ action: 'snapshot' })) as { elements: AXElement[] };
  return out;
}

export async function findElement(query: string): Promise<{ frame: AXFrame | null }> {
  const out = (await runAxHelper({ action: 'find', query })) as { frame: AXFrame | null };
  return out;
}
