/* eslint-disable @typescript-eslint/no-explicit-any, @typescript-eslint/no-require-imports, @typescript-eslint/no-unused-vars */
/**
 * Extension Registry
 *
 * Fetches, caches, installs, and uninstalls community extensions.
 *
 * Strategy: API-based (supercmd-backend) + prebuilt bundles first.
 *   - Git is only a last-resort sparse source fallback after API failure.
 *   - Fast search/discovery via backend API.
 *   - Pre-built bundles downloaded from S3.
 *   - Source-based installs use Bun for dependency installation.
 *   - npm and git fallbacks have been removed to keep installs reproducible
 *     and avoid shipping a package manager inside a launcher.
 */

import { app } from '@tezbar/desktop-runtime';
import { execFileSync } from 'child_process';
import { EventEmitter } from 'events';
import * as fs from 'fs';
import * as path from 'path';
import * as zlib from 'zlib';
import { getCurrentRaycastPlatform, getManifestPlatforms, isManifestPlatformCompatible } from './extension-platform';
import { discoverInstalledExtensionCommands, type ExtensionCommandInfo } from './extension-builder';
import {
  fetchCatalogFromAPI,
  getExtensionBundleUrl,
  getExtensionScreenshotsFromAPI,
  reportInstall,
  reportUninstall,
} from './extension-api';
import { installDepsWithBun, installSpecificPackagesWithBun } from './bun-manager';
import type { ExtensionManifest } from '../shared/extensions';
import { searchLovedExtensions } from '../shared/lovedExtensions';
import type { ExtensionRegistryCommand, InstalledRegistryExtension } from '../shared/extensionRuntime';
import { parseGitHubRepositoryUrl, type GitHubRepositoryReference } from '../shared/extensionRepository';

export const extensionRegistryEvents = new EventEmitter();

const extensionInstallJobs = new Map<string, Promise<InstalledRegistryExtension>>();

function emitInstallProgress(name: string, progress: number): void {
  extensionRegistryEvents.emit('progress', {
    id: normalizeRaymesExtensionId(name),
    progress: Math.max(0, Math.min(100, progress)),
  });
}

async function installDependenciesWithProgress(name: string, installPath: string): Promise<boolean> {
  let progress = 60;
  emitInstallProgress(name, progress);
  const heartbeat = setInterval(() => {
    progress = Math.min(72, progress + 1);
    emitInstallProgress(name, progress);
  }, 1_500);
  heartbeat.unref?.();

  try {
    return await installDepsWithBun(installPath);
  } finally {
    clearInterval(heartbeat);
  }
}

function hasNodeModules(extPath: string): boolean {
  try {
    return fs.existsSync(path.join(extPath, 'node_modules'));
  } catch {
    return false;
  }
}

const GITHUB_RAW = 'https://raw.githubusercontent.com/raycast/extensions/main';
const GITHUB_API = 'https://api.github.com/repos/raycast/extensions/contents';
const GITHUB_TREE_API = 'https://api.github.com/repos/raycast/extensions/git/trees/main?recursive=1';
const RAYMES_EXTENSIONS_GIT = 'https://github.com/almatkai/raymes-extensions.git';

type RepoTreeEntry = {
  path: string;
  type: 'blob' | 'tree' | string;
  size?: number;
};
type RepoTreeCache = {
  fetchedAt: number;
  entries: RepoTreeEntry[];
};
const REPO_TREE_TTL_MS = 10 * 60 * 1000;
let repoTreeCache: RepoTreeCache | null = null;
const MAX_EXTENSION_ARCHIVE_BYTES = 150 * 1024 * 1024;

function shouldUseNetworkFallback(error: any): boolean {
  const text = `${String(error?.message || '')}\n${String(error?.stderr || '')}`.toLowerCase();
  return (
    text.includes('homebrew was not found') ||
    text.includes('git executable was not found') ||
    text.includes('xcode-select') ||
    text.includes('no developer tools were found') ||
    text.includes('unable to find utility "git"') ||
    /enoent|not found/.test(text)
  );
}

function githubApiHeaders(): Record<string, string> {
  return {
    'User-Agent': 'Tezbar',
    Accept: 'application/vnd.github+json',
  };
}

async function fetchWithTimeout(url: string, options: any = {}, timeoutMs = 45_000): Promise<Response> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetch(url, {
      ...options,
      signal: controller.signal,
    });
  } finally {
    clearTimeout(timeout);
  }
}

async function fetchRepoTreeEntries(forceRefresh = false): Promise<RepoTreeEntry[]> {
  if (!forceRefresh && repoTreeCache && Date.now() - repoTreeCache.fetchedAt < REPO_TREE_TTL_MS) {
    return repoTreeCache.entries;
  }

  const response = await fetchWithTimeout(GITHUB_TREE_API, { headers: githubApiHeaders() }, 90_000);
  if (!response.ok) {
    throw new Error(`GitHub tree fetch failed with ${response.status} ${response.statusText}`);
  }

  const data = await response.json();
  const rawEntries = Array.isArray(data?.tree) ? data.tree : [];
  const entries: RepoTreeEntry[] = rawEntries
    .map((entry: any) => ({
      path: String(entry?.path || ''),
      type: String(entry?.type || ''),
      size: typeof entry?.size === 'number' ? entry.size : undefined,
    }))
    .filter((entry: RepoTreeEntry) => Boolean(entry.path));

  repoTreeCache = {
    fetchedAt: Date.now(),
    entries,
  };
  return entries;
}

function readCatalogEntriesFromExtensionsDir(extensionsDir: string): CatalogEntry[] {
  const dirs = fs.readdirSync(extensionsDir);
  const entries: CatalogEntry[] = [];

  for (const dir of dirs) {
    const pkgPath = path.join(extensionsDir, dir, 'package.json');
    if (!fs.existsSync(pkgPath)) continue;

    try {
      const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf-8'));

      const toAssetUrl = (value: string): string => {
        if (!value) return '';
        if (/^https?:\/\//i.test(value)) return value;
        const normalized = value.replace(/^\.?\//, '');
        if (normalized.startsWith('extensions/')) {
          return `${GITHUB_RAW}/${normalized}`;
        }
        return `${GITHUB_RAW}/extensions/${dir}/${normalized}`;
      };

      const iconFile = pkg.icon || 'assets/icon.png';
      const iconUrl = toAssetUrl(iconFile.includes('/') ? iconFile : `assets/${iconFile}`);

      const commands = (pkg.commands || []).map((c: any) => ({
        name: c.name || '',
        title: c.title || '',
        description: c.description || '',
      }));
      const platforms = getManifestPlatforms(pkg);
      if (!isManifestPlatformCompatible(pkg)) {
        continue;
      }

      const normalizePerson = (p: any): string | null => {
        if (!p) return null;
        if (typeof p === 'string') {
          const cleaned = p.split('<')[0].split('(')[0].trim();
          return cleaned || null;
        }
        if (typeof p === 'object') {
          const name = typeof p.name === 'string' ? p.name.trim() : '';
          return name || null;
        }
        return null;
      };

      const contributors: string[] = [];
      const addContributor = (name: string | null) => {
        if (!name) return;
        if (!contributors.includes(name)) contributors.push(name);
      };

      addContributor(normalizePerson(pkg.author));
      if (Array.isArray(pkg.contributors)) {
        for (const person of pkg.contributors) {
          addContributor(normalizePerson(person));
        }
      }

      const authorName = normalizePerson(pkg.author) || '';
      const screenshotUrlsFromPackage: string[] = Array.isArray(pkg.screenshots)
        ? pkg.screenshots
            .map((entry: any) => {
              if (typeof entry === 'string') return toAssetUrl(entry);
              if (entry && typeof entry === 'object') {
                if (typeof entry.path === 'string') return toAssetUrl(entry.path);
                if (typeof entry.src === 'string') return toAssetUrl(entry.src);
                if (typeof entry.url === 'string') return toAssetUrl(entry.url);
              }
              return '';
            })
            .filter(Boolean)
        : [];

      const screenshotUrls = screenshotUrlsFromPackage;

      entries.push({
        name: dir,
        title: pkg.title || dir,
        description: pkg.description || '',
        author: authorName,
        contributors,
        icon: iconFile,
        iconUrl,
        screenshotUrls,
        categories: pkg.categories || [],
        platforms,
        commands,
      });
    } catch {
      // Skip malformed package.json
    }
  }

  entries.sort((a, b) => a.title.localeCompare(b.title));
  return entries;
}

function buildLightweightCatalogFromTree(
  treeEntries: RepoTreeEntry[],
  previousEntries: CatalogEntry[] = []
): CatalogEntry[] {
  const previousByName = new Map<string, CatalogEntry>();
  for (const entry of previousEntries) {
    previousByName.set(entry.name, entry);
  }

  const extensionNames = new Set<string>();
  for (const entry of treeEntries) {
    const match = /^extensions\/([^/]+)\/package\.json$/.exec(entry.path);
    if (match) extensionNames.add(match[1]);
  }

  const result: CatalogEntry[] = Array.from(extensionNames).map((name) => {
    const previous = previousByName.get(name);
    const fallbackTitle = name.replace(/[-_]+/g, ' ');
    return {
      name,
      title: previous?.title || fallbackTitle || name,
      description: previous?.description || '',
      author: previous?.author || '',
      contributors: previous?.contributors || [],
      icon: previous?.icon || 'assets/icon.png',
      iconUrl: previous?.iconUrl || `${GITHUB_RAW}/extensions/${name}/assets/icon.png`,
      screenshotUrls: previous?.screenshotUrls || [],
      categories: previous?.categories || [],
      platforms: previous?.platforms || [],
      commands: previous?.commands || [],
    };
  });

  result.sort((a, b) => a.title.localeCompare(b.title));
  return result;
}

async function downloadExtensionFromTree(name: string, tmpDir: string): Promise<string | null> {
  const treeEntries = await fetchRepoTreeEntries();
  const prefix = `extensions/${name}/`;
  const fileEntries = treeEntries.filter((entry) => entry.type === 'blob' && entry.path.startsWith(prefix));
  if (fileEntries.length === 0) return null;

  const srcDir = path.join(tmpDir, 'extensions', name);
  fs.mkdirSync(srcDir, { recursive: true });

  // Create all directories upfront
  for (const entry of fileEntries) {
    const relativePath = entry.path.slice(prefix.length);
    if (!relativePath) continue;
    const destination = path.join(srcDir, relativePath);
    fs.mkdirSync(path.dirname(destination), { recursive: true });
  }

  // Download files in parallel (up to 15 concurrent)
  const CONCURRENCY = 30;
  let index = 0;

  const downloadOne = async () => {
    while (index < fileEntries.length) {
      const i = index++;
      const entry = fileEntries[i];
      const relativePath = entry.path.slice(prefix.length);
      if (!relativePath) continue;

      const destination = path.join(srcDir, relativePath);
      const fileUrl = `${GITHUB_RAW}/${entry.path}`;
      const response = await fetchWithTimeout(
        fileUrl,
        {
          headers: {
            'User-Agent': 'Tezbar',
            Accept: 'application/octet-stream',
          },
        },
        90_000
      );
      if (!response.ok) {
        throw new Error(`Failed to download ${entry.path} (${response.status} ${response.statusText})`);
      }
      const data = await response.arrayBuffer();
      fs.writeFileSync(destination, Buffer.from(data));
    }
  };

  const workers = Array.from({ length: Math.min(CONCURRENCY, fileEntries.length) }, () => downloadOne());
  await Promise.all(workers);

  console.log(`Downloaded ${fileEntries.length} files for "${name}"`);
  return srcDir;
}

function downloadExtensionViaSparseGit(name: string, tmpDir: string): string | null {
  const checkoutDir = path.join(tmpDir, 'raymes-extensions-source');
  try {
    execFileSync(
      'git',
      ['clone', '--depth', '1', '--filter=blob:none', '--sparse', RAYMES_EXTENSIONS_GIT, checkoutDir],
      { stdio: 'ignore', timeout: 180_000 }
    );
    execFileSync('git', ['-C', checkoutDir, 'sparse-checkout', 'set', `extensions/${name}`], {
      stdio: 'ignore',
      timeout: 120_000,
    });
    const extensionDir = path.join(checkoutDir, 'extensions', name);
    return fs.existsSync(path.join(extensionDir, 'package.json')) ? extensionDir : null;
  } catch (error) {
    console.warn(`Sparse git fallback failed for "${name}":`, error);
    return null;
  }
}

// ─── Types ──────────────────────────────────────────────────────────

export interface CatalogEntry {
  name: string; // directory name in repo
  title: string;
  description: string;
  author: string;
  contributors: string[];
  icon: string; // icon filename
  iconUrl: string; // full GitHub raw URL to icon
  screenshotUrls: string[];
  categories: string[];
  platforms: string[];
  commands: { name: string; title: string; description: string }[];
  installCount?: number; // from backend API
}

interface CatalogCache {
  entries: CatalogEntry[];
  fetchedAt: number;
  version: number;
}

const CATALOG_VERSION = 6;
const CATALOG_TTL = 24 * 60 * 60 * 1000; // 24 hours

let catalogCache: CatalogCache | null = null;
let catalogFetchPromise: Promise<CatalogEntry[]> | null = null;

function coerceCatalogEntry(raw: any): CatalogEntry | null {
  if (!raw || typeof raw !== 'object') return null;
  const name = typeof raw.name === 'string' ? raw.name : '';
  if (!name) return null;

  const commands = Array.isArray(raw.commands)
    ? raw.commands
        .filter((cmd: any) => cmd && typeof cmd === 'object' && cmd.name)
        .map((cmd: any) => ({
          name: String(cmd.name || ''),
          title: String(cmd.title || cmd.name || ''),
          description: String(cmd.description || ''),
        }))
    : [];

  return {
    name,
    title: typeof raw.title === 'string' ? raw.title : name,
    description: typeof raw.description === 'string' ? raw.description : '',
    author: typeof raw.author === 'string' ? raw.author : '',
    contributors: Array.isArray(raw.contributors) ? raw.contributors.filter((v: any) => typeof v === 'string') : [],
    icon: typeof raw.icon === 'string' ? raw.icon : '',
    iconUrl: typeof raw.iconUrl === 'string' ? raw.iconUrl : '',
    screenshotUrls: Array.isArray(raw.screenshotUrls)
      ? raw.screenshotUrls.filter((v: any) => typeof v === 'string')
      : [],
    categories: Array.isArray(raw.categories) ? raw.categories.filter((v: any) => typeof v === 'string') : [],
    platforms: Array.isArray(raw.platforms) ? raw.platforms.filter((v: any) => typeof v === 'string') : [],
    commands,
  };
}

// ─── Paths ──────────────────────────────────────────────────────────

function getCatalogPath(): string {
  return path.join(app.getPath('userData'), 'extension-catalog.json');
}

function getExtensionsDir(): string {
  const dir = path.join(app.getPath('userData'), 'extensions');
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  return dir;
}

function getInstalledPath(name: string): string {
  return path.join(getExtensionsDir(), name);
}

function getLegacyInstalledPath(name: string): string {
  const slug = slugFromRaymesExtensionId(name);
  return path.join(getExtensionsDir(), 'packages', normalizeRaymesExtensionId(slug));
}

function getLegacyRegistryInstalledPath(name: string): string {
  const slug = slugFromRaymesExtensionId(name);
  return path.join(app.getPath('userData'), 'extension-registry', 'packages', normalizeRaymesExtensionId(slug));
}

function resolveInstalledExtensionPathForRaymes(name: string): string | null {
  const slug = slugFromRaymesExtensionId(name);
  if (!slug) return null;
  const candidates = [
    getInstalledPath(slug),
    getInstalledPath(normalizeRaymesExtensionId(slug)),
    getLegacyInstalledPath(slug),
    getLegacyRegistryInstalledPath(slug),
  ];
  for (const candidate of candidates) {
    if (fs.existsSync(path.join(candidate, 'package.json'))) return candidate;
  }
  return null;
}

// ─── Catalog: Disk Cache ────────────────────────────────────────────

function loadCatalogFromDisk(): CatalogCache | null {
  try {
    const data = fs.readFileSync(getCatalogPath(), 'utf-8');
    const parsed = JSON.parse(data) as Partial<CatalogCache>;
    const entries = Array.isArray(parsed.entries)
      ? (parsed.entries.map((entry: any) => coerceCatalogEntry(entry)).filter(Boolean) as CatalogEntry[])
      : [];
    if (entries.length === 0) return null;
    return {
      entries,
      fetchedAt: typeof parsed.fetchedAt === 'number' ? parsed.fetchedAt : Date.now(),
      version: typeof parsed.version === 'number' ? parsed.version : CATALOG_VERSION,
    };
  } catch {}
  return null;
}

function saveCatalogToDisk(catalog: CatalogCache): void {
  try {
    fs.writeFileSync(getCatalogPath(), JSON.stringify(catalog));
  } catch (e) {
    console.error('Failed to save catalog:', e);
  }
}

// ─── Catalog: Public API ────────────────────────────────────────────

async function getCatalogWithoutSharedRequest(forceRefresh = false): Promise<CatalogEntry[]> {
  // In-memory cache
  if (!forceRefresh && catalogCache && Date.now() - catalogCache.fetchedAt < CATALOG_TTL) {
    return catalogCache.entries;
  }

  // Disk cache
  if (!forceRefresh) {
    const diskCache = loadCatalogFromDisk();
    if (diskCache && Date.now() - diskCache.fetchedAt < CATALOG_TTL) {
      catalogCache = diskCache;
      return diskCache.entries;
    }
  }

  // PRIMARY: Fetch from supercmd-backend API
  try {
    console.log('Fetching extension catalog from API…');
    const entries = await fetchCatalogFromAPI();

    const cache: CatalogCache = {
      entries,
      fetchedAt: Date.now(),
      version: CATALOG_VERSION,
    };
    catalogCache = cache;
    saveCatalogToDisk(cache);

    console.log(`Extension catalog (API): ${entries.length} extensions cached.`);
    return entries;
  } catch (apiError: any) {
    console.warn('API catalog fetch failed:', apiError?.message || apiError);
  }

  // FALLBACK: disk cache (even if expired)
  const diskCache = loadCatalogFromDisk();
  if (diskCache) {
    catalogCache = diskCache;
    console.log(`Extension catalog (disk cache): ${diskCache.entries.length} extensions from cache.`);
    return diskCache.entries;
  }

  return [];
}

export async function getCatalog(forceRefresh = false): Promise<CatalogEntry[]> {
  // The store searches as the user types. Share the pending catalog load so a
  // cold cache never produces one long-running backend request per character.
  if (catalogFetchPromise) return catalogFetchPromise;

  const request = getCatalogWithoutSharedRequest(forceRefresh);
  catalogFetchPromise = request;
  try {
    return await request;
  } finally {
    if (catalogFetchPromise === request) catalogFetchPromise = null;
  }
}

/**
 * Lazily fetch screenshot URLs for one extension.
 * Tries the backend API first, falls back to GitHub API.
 */
export async function getExtensionScreenshotUrls(name: string): Promise<string[]> {
  if (!name) return [];

  // PRIMARY: Try backend API
  try {
    const urls = await getExtensionScreenshotsFromAPI(name);
    if (urls.length > 0) return urls;
  } catch (apiError: any) {
    console.warn(`API screenshots fetch failed for ${name}:`, apiError?.message || apiError);
  }

  // FALLBACK: GitHub API
  try {
    const url = `${GITHUB_API}/extensions/${encodeURIComponent(name)}/metadata`;
    const response = await fetch(url, {
      headers: {
        'User-Agent': 'Tezbar',
        Accept: 'application/vnd.github+json',
      },
    });
    if (!response.ok) return [];
    const data = await response.json();
    if (!Array.isArray(data)) return [];
    const imagePattern = /\.(png|jpe?g|webp|gif)$/i;
    return data
      .filter((entry: any) => entry?.type === 'file' && imagePattern.test(entry?.name || ''))
      .sort((a: any, b: any) =>
        String(a?.name || '').localeCompare(String(b?.name || ''), undefined, {
          numeric: true,
        })
      )
      .map((entry: any) => String(entry?.download_url || ''))
      .filter(Boolean);
  } catch (e) {
    console.warn(`Failed to load screenshots for ${name}:`, e);
    return [];
  }
}

// ─── Dependency Installation ────────────────────────────────────────

/**
 * Install an extension's npm dependencies.
 *
 * Strategy:
 *   1. Read the extension's package.json
 *   2. Collect non-Raycast, non-dev dependencies
 *   3. Install them with Bun (no npm fallback).
 */
/**
 * Install a specific set of packages into an extension's node_modules without
 * modifying its package.json. Used to repair extensions that import modules not
 * declared in their dependencies (a pattern Raycast tolerates via `ray build`
 * but esbuild does not).
 */
export async function installSpecificPackages(extPath: string, packageNames: string[]): Promise<void> {
  const unique = Array.from(new Set(packageNames.map((name) => String(name || '').trim()).filter(Boolean)));
  if (unique.length === 0) return;

  console.log(`Installing missing packages for ${path.basename(extPath)}: ${unique.join(', ')}`);

  const ok = await installSpecificPackagesWithBun(extPath, unique);
  if (!ok) {
    throw new Error(`Bun failed to install packages for ${path.basename(extPath)}`);
  }
}

export async function installExtensionDeps(extPath: string): Promise<void> {
  const pkgPath = path.join(extPath, 'package.json');
  if (!fs.existsSync(pkgPath)) return;

  let pkg: any;
  try {
    pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf-8'));
  } catch {
    return;
  }

  const deps = {
    ...(pkg.dependencies || {}),
    ...(pkg.optionalDependencies || {}),
  };
  const thirdPartyDeps = Object.entries(deps)
    .filter(([name]) => !name.startsWith('@raycast/'))
    .map(([name, version]) => `${name}@${version}`)
    .filter(Boolean);

  if (thirdPartyDeps.length === 0) {
    console.log(`No third-party dependencies for ${path.basename(extPath)}`);
    return;
  }

  console.log(
    `Installing ${thirdPartyDeps.length} dependencies for ${path.basename(extPath)}: ${thirdPartyDeps.join(', ')}`
  );

  const ok = await installDepsWithBun(extPath);
  if (!ok) {
    throw new Error(`Bun dependency installation failed for ${path.basename(extPath)}`);
  }
  if (!hasNodeModules(extPath)) {
    throw new Error('Bun completed but node_modules is still missing');
  }
  console.log(`Dependencies installed for ${path.basename(extPath)}`);
}

// ─── Install / Uninstall ────────────────────────────────────────────

export function isExtensionInstalled(name: string): boolean {
  return resolveInstalledExtensionPathForRaymes(name) !== null;
}

export function getInstalledExtensionNames(): string[] {
  const names = new Set<string>();
  const scanRoot = (root: string, stripRaycastPrefix: boolean) => {
    if (!fs.existsSync(root)) return;
    try {
      for (const d of fs.readdirSync(root)) {
        const p = path.join(root, d);
        if (fs.statSync(p).isDirectory() && fs.existsSync(path.join(p, 'package.json'))) {
          names.add(stripRaycastPrefix ? slugFromRaymesExtensionId(d) : d);
        }
      }
    } catch {}
  };

  scanRoot(getExtensionsDir(), false);
  scanRoot(path.join(getExtensionsDir(), 'packages'), true);
  scanRoot(path.join(app.getPath('userData'), 'extension-registry', 'packages'), true);
  return [...names].filter(Boolean).sort((a, b) => a.localeCompare(b));
}

function getDirectInstalledExtensionNames(): string[] {
  try {
    return fs.readdirSync(getExtensionsDir()).filter((d) => {
      const p = getInstalledPath(d);
      return fs.statSync(p).isDirectory() && fs.existsSync(path.join(p, 'package.json'));
    });
  } catch {
    return [];
  }
}

/**
 * Install a community extension by name.
 *
 * Strategy:
 *   1. PRIMARY: Download pre-built bundle from API (no package manager needed).
 *   2. FALLBACK: Download source from GitHub raw + Bun + esbuild.
 *   No npm or git is used anywhere in the install path.
 */
export async function installExtension(name: string): Promise<boolean> {
  if (!/^[A-Za-z0-9._-]+$/.test(String(name || ''))) {
    console.error(`Invalid extension name: "${name}"`);
    return false;
  }

  emitInstallProgress(name, 5);

  // 1. FASTEST: Pre-built bundle from S3 (~2-3s, no npm/bun/esbuild needed)
  try {
    const success = await installExtensionFromBundle(name);
    if (success) return true;
  } catch (bundleError: any) {
    console.warn(`Bundle install failed for "${name}":`, bundleError?.message || bundleError);
  }

  // 2. FALLBACK: Download source + Bun + esbuild
  emitInstallProgress(name, 45);
  try {
    const success = await installExtensionViaAPI(name);
    if (success) return true;
  } catch (apiError: any) {
    console.warn(`API install failed for "${name}":`, apiError?.message || apiError);
  }

  return false;
}

// ─── Pre-built Bundle Install (Fastest) ─────────────────────────────

/**
 * Download a pre-built bundle from S3 via the backend API.
 * The bundle contains package.json + assets/ + .sc-build/ (esbuild output).
 * No npm, no bun, no esbuild needed. ~2-3s total.
 */
async function installExtensionFromBundle(name: string): Promise<boolean> {
  const installPath = getInstalledPath(name);
  const hadExistingInstall = fs.existsSync(installPath);
  const backupPath = hadExistingInstall ? path.join(getExtensionsDir(), `${name}.backup-${Date.now()}`) : '';
  const tmpDir = path.join(app.getPath('temp'), `supercmd-bundle-${Date.now()}`);

  try {
    const t0 = Date.now();

    // Get pre-signed S3 URL from backend
    const { url } = await getExtensionBundleUrl(name);
    emitInstallProgress(name, 10);
    console.log(`Downloading pre-built bundle for "${name}"…`);

    fs.mkdirSync(tmpDir, { recursive: true });
    await downloadAndExtractTarball(url, tmpDir);
    emitInstallProgress(name, 35);

    // Find the extension in the extracted directory
    const nestedPath = path.join(tmpDir, name);
    let srcDir = tmpDir;
    if (fs.existsSync(path.join(nestedPath, 'package.json'))) {
      srcDir = nestedPath;
    } else if (!fs.existsSync(path.join(srcDir, 'package.json'))) {
      // Search subdirs
      const subdirs = fs.readdirSync(tmpDir, { withFileTypes: true }).filter((d) => d.isDirectory());
      for (const sub of subdirs) {
        if (fs.existsSync(path.join(tmpDir, sub.name, 'package.json'))) {
          srcDir = path.join(tmpDir, sub.name);
          break;
        }
      }
    }

    if (!fs.existsSync(path.join(srcDir, 'package.json'))) {
      throw new Error('Bundle has no package.json');
    }

    // Must have .sc-build/ — otherwise it's not a valid pre-built bundle
    if (!fs.existsSync(path.join(srcDir, '.sc-build'))) {
      throw new Error('Bundle has no .sc-build/ directory — not a pre-built bundle');
    }

    const bundleManifest = JSON.parse(fs.readFileSync(path.join(srcDir, 'package.json'), 'utf8'));
    const missingCommandBundles = (Array.isArray(bundleManifest.commands) ? bundleManifest.commands : [])
      .filter((command: any) => command?.name)
      .filter((command: any) => !fs.existsSync(path.join(srcDir, '.sc-build', `${command.name}.js`)))
      .map((command: any) => command.name);
    if (missingCommandBundles.length > 0) {
      throw new Error(`Bundle is incomplete; missing commands: ${missingCommandBundles.join(', ')}`);
    }

    // Backup existing
    if (hadExistingInstall) {
      fs.renameSync(installPath, backupPath);
    }

    // Copy to extensions directory
    fs.cpSync(srcDir, installPath, { recursive: true });
    emitInstallProgress(name, 95);

    // Cleanup backup
    if (backupPath && fs.existsSync(backupPath)) {
      fs.rmSync(backupPath, { recursive: true, force: true });
    }

    // Report install (fire-and-forget)
    reportInstall(name, getMachineId()).catch(() => {});

    console.log(`Extension "${name}" installed from pre-built bundle in ${Date.now() - t0}ms`);
    return true;
  } catch (error) {
    // Rollback
    try {
      fs.rmSync(installPath, { recursive: true, force: true });
    } catch {}
    if (backupPath && fs.existsSync(backupPath)) {
      try {
        fs.renameSync(backupPath, installPath);
      } catch {}
    }
    throw error;
  } finally {
    try {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    } catch {}
    if (backupPath && fs.existsSync(backupPath)) {
      try {
        fs.rmSync(backupPath, { recursive: true, force: true });
      } catch {}
    }
  }
}

// ─── Source-based Install ───────────────────────────────────────────

/**
 * Download source from GitHub raw, install deps with bun/npm, esbuild.
 * Fallback when no pre-built bundle exists.
 */
async function installExtensionViaAPI(name: string): Promise<boolean> {
  const installPath = getInstalledPath(name);
  const hadExistingInstall = fs.existsSync(installPath);
  const backupPath = hadExistingInstall ? path.join(getExtensionsDir(), `${name}.backup-${Date.now()}`) : '';
  const tmpDir = path.join(app.getPath('temp'), `supercmd-api-install-${Date.now()}`);

  try {
    const t0 = Date.now();
    console.log(`Installing extension: ${name}…`);
    fs.mkdirSync(tmpDir, { recursive: true });

    // Download extension source from GitHub raw (no git needed)
    let srcDir: string | null = null;
    try {
      srcDir = await downloadExtensionFromTree(name, tmpDir);
    } catch (error) {
      console.warn(`GitHub API source download failed for "${name}"; trying sparse git.`, error);
    }
    if (!srcDir) srcDir = downloadExtensionViaSparseGit(name, tmpDir);
    console.log(`  Download: ${Date.now() - t0}ms`);
    emitInstallProgress(name, 55);

    if (!srcDir || !fs.existsSync(path.join(srcDir, 'package.json'))) {
      throw new Error(`Extension "${name}" not found or has no package.json`);
    }

    // Platform compatibility check
    const srcPkg = JSON.parse(fs.readFileSync(path.join(srcDir, 'package.json'), 'utf-8'));
    if (!isManifestPlatformCompatible(srcPkg)) {
      const supported = getManifestPlatforms(srcPkg);
      console.error(
        `Extension "${name}" is not compatible with ${getCurrentRaycastPlatform()} (supports: ${supported.join(', ')})`
      );
      return false;
    }

    // Backup existing installation
    if (hadExistingInstall) {
      fs.renameSync(installPath, backupPath);
    }

    // Copy to local extensions directory
    fs.cpSync(srcDir, installPath, { recursive: true });

    // Install dependencies and build
    {
      const extPkg = JSON.parse(fs.readFileSync(path.join(installPath, 'package.json'), 'utf-8'));
      const allDeps = { ...(extPkg.dependencies || {}), ...(extPkg.optionalDependencies || {}) };
      const thirdPartyDeps = Object.keys(allDeps).filter((d) => !d.startsWith('@raycast/'));

      if (thirdPartyDeps.length === 0) {
        console.log(`No third-party dependencies for "${name}" — skipping install`);
      } else {
        // Bun only — npm fallback removed.
        const depsInstalled = await installDependenciesWithProgress(name, installPath);
        if (!depsInstalled) {
          throw new Error(`Could not install dependencies for "${name}"`);
        }
      }

      emitInstallProgress(name, 75);
      const t1 = Date.now();
      console.log(`  Deps: ${t1 - t0}ms. Pre-building commands for "${name}"…`);
      const { buildAllCommands } = require('./extension-builder');
      const builtCount = await buildAllCommands(name);
      const expectedCount = (Array.isArray(extPkg.commands) ? extPkg.commands : []).filter(
        (command: any) => command?.name
      ).length;
      if (builtCount < expectedCount) {
        throw new Error(`Built ${builtCount}/${expectedCount} commands for "${name}"`);
      }
      emitInstallProgress(name, 95);
      console.log(
        `  Build: ${Date.now() - t1}ms. Extension "${name}" installed (${builtCount} commands) in ${Date.now() - t0}ms total`
      );
    }

    // Cleanup backup
    if (backupPath && fs.existsSync(backupPath)) {
      fs.rmSync(backupPath, { recursive: true, force: true });
    }

    // Report install to backend (fire-and-forget)
    reportInstall(name, getMachineId()).catch(() => {});

    return true;
  } catch (error) {
    console.error(`API install failed for "${name}":`, error);
    // Rollback
    try {
      fs.rmSync(installPath, { recursive: true, force: true });
    } catch {}
    if (backupPath && fs.existsSync(backupPath)) {
      try {
        fs.renameSync(backupPath, installPath);
      } catch {}
    }
    throw error; // Re-throw so the caller knows to try git fallback
  } finally {
    try {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    } catch {}
    if (backupPath && fs.existsSync(backupPath)) {
      try {
        fs.rmSync(backupPath, { recursive: true, force: true });
      } catch {}
    }
  }
}

// ─── Download + Extract Helpers ─────────────────────────────────────

/**
 * Download a .tar.gz from a URL and extract to destDir.
 * Uses Node.js built-in https + zlib + tar-stream parsing — no npm deps.
 */
function findRepositoryExtensionRoot(extractedDir: string): string | null {
  if (fs.existsSync(path.join(extractedDir, 'package.json'))) return extractedDir;

  const directories = fs.readdirSync(extractedDir, { withFileTypes: true }).filter((entry) => entry.isDirectory());
  for (const directory of directories) {
    const candidate = path.join(extractedDir, directory.name);
    if (fs.existsSync(path.join(candidate, 'package.json'))) return candidate;
  }
  return null;
}

function extensionSlugFromRepositoryManifest(pkg: any, fallback: string): string {
  const packageName = String(pkg?.name || '').trim();
  if (/^[A-Za-z0-9._-]+$/.test(packageName)) return packageName;

  const sanitized = String(fallback || '')
    .trim()
    .replace(/[^A-Za-z0-9._-]+/g, '-')
    .replace(/^-+|-+$/g, '');
  if (!sanitized) throw new Error('The repository does not provide a valid extension name');
  return sanitized;
}

function missingPrebuiltCommands(extensionPath: string, pkg: any): string[] {
  const commands = Array.isArray(pkg?.commands) ? pkg.commands : [];
  return commands
    .map((command: any) => String(command?.name || '').trim())
    .filter(Boolean)
    .filter((name: string) => !fs.existsSync(path.join(extensionPath, '.sc-build', `${name}.js`)));
}

async function installRegistryExtensionFromGitHubRepository(
  reference: GitHubRepositoryReference
): Promise<InstalledRegistryExtension> {
  const metadataUrl = `https://api.github.com/repos/${encodeURIComponent(reference.owner)}/${encodeURIComponent(reference.repository)}`;
  const metadataResponse = await fetchWithTimeout(metadataUrl, { headers: githubApiHeaders() }, 30_000);
  if (!metadataResponse.ok) {
    if (metadataResponse.status === 404) {
      throw new Error(`Repository not found or private: ${reference.url}`);
    }
    throw new Error(`GitHub repository lookup failed (${metadataResponse.status} ${metadataResponse.statusText})`);
  }

  const metadata = await metadataResponse.json();
  const defaultBranch = String(metadata?.default_branch || '').trim();
  if (!defaultBranch) throw new Error('GitHub did not return a default branch for this repository');

  const tmpDir = path.join(app.getPath('temp'), `tezbar-repository-install-${Date.now()}-${randomHex(8)}`);
  let installPath = '';
  let backupPath = '';
  let installedSlug = '';

  try {
    fs.mkdirSync(tmpDir, { recursive: true });
    const archiveUrl =
      `https://codeload.github.com/${encodeURIComponent(reference.owner)}/` +
      `${encodeURIComponent(reference.repository)}/tar.gz/${encodeURIComponent(defaultBranch)}`;
    await downloadAndExtractTarball(archiveUrl, tmpDir);

    const sourcePath = findRepositoryExtensionRoot(tmpDir);
    if (!sourcePath) {
      throw new Error('This repository has no package.json at its root');
    }

    const pkgPath = path.join(sourcePath, 'package.json');
    const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf-8'));
    const commands = Array.isArray(pkg?.commands)
      ? pkg.commands.filter((command: any) => String(command?.name || '').trim())
      : [];
    if (commands.length === 0) {
      throw new Error('This repository is not a Tezbar extension: package.json has no commands');
    }
    if (!isManifestPlatformCompatible(pkg)) {
      const supported = getManifestPlatforms(pkg);
      throw new Error(
        `This extension does not support ${getCurrentRaycastPlatform()}` +
          (supported.length > 0 ? ` (supports: ${supported.join(', ')})` : '')
      );
    }

    installedSlug = extensionSlugFromRepositoryManifest(pkg, reference.repository);
    installPath = getInstalledPath(installedSlug);
    const hadExistingInstall = fs.existsSync(installPath);
    backupPath = hadExistingInstall ? path.join(getExtensionsDir(), `${installedSlug}.backup-${Date.now()}`) : '';

    emitInstallProgress(installedSlug, 20);
    if (hadExistingInstall) fs.renameSync(installPath, backupPath);
    fs.cpSync(sourcePath, installPath, { recursive: true });
    emitInstallProgress(installedSlug, 45);

    const installedPkgPath = path.join(installPath, 'package.json');
    const installedPkg = JSON.parse(fs.readFileSync(installedPkgPath, 'utf-8'));
    const missingBundles = missingPrebuiltCommands(installPath, installedPkg);
    if (missingBundles.length > 0) {
      const dependenciesInstalled = await installDepsWithBun(installPath);
      if (!dependenciesInstalled) {
        throw new Error(`Could not install dependencies for ${installedSlug}`);
      }

      emitInstallProgress(installedSlug, 70);
      const { buildAllCommands } = require('./extension-builder');
      const builtCount = await buildAllCommands(installedSlug);
      const expectedCount = (Array.isArray(installedPkg.commands) ? installedPkg.commands : []).filter((command: any) =>
        String(command?.name || '').trim()
      ).length;
      if (builtCount < expectedCount) {
        throw new Error(`Built ${builtCount}/${expectedCount} commands for ${installedSlug}`);
      }
    }

    const stillMissing = missingPrebuiltCommands(installPath, installedPkg);
    if (stillMissing.length > 0) {
      throw new Error(`Extension bundle is missing commands: ${stillMissing.join(', ')}`);
    }

    if (backupPath && fs.existsSync(backupPath)) {
      fs.rmSync(backupPath, { recursive: true, force: true });
    }
    emitInstallProgress(installedSlug, 100);

    const installed = listInstalledRegistryExtensions().find((entry) => entry.slug === installedSlug);
    if (!installed) {
      throw new Error(`Extension installed but could not be loaded: ${installedSlug}`);
    }
    return installed;
  } catch (error) {
    if (installPath) {
      try {
        fs.rmSync(installPath, { recursive: true, force: true });
      } catch {}
    }
    if (backupPath && fs.existsSync(backupPath)) {
      try {
        fs.renameSync(backupPath, installPath);
      } catch {}
    }
    if (installedSlug) emitInstallProgress(installedSlug, 0);
    throw error;
  } finally {
    try {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    } catch {}
    if (backupPath && fs.existsSync(backupPath)) {
      try {
        fs.rmSync(backupPath, { recursive: true, force: true });
      } catch {}
    }
  }
}

async function downloadAndExtractTarball(url: string, destDir: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const makeRequest = (requestUrl: string, redirectCount = 0) => {
      if (redirectCount > 5) {
        reject(new Error('Too many redirects'));
        return;
      }

      const parsedUrl = new URL(requestUrl);
      const isHttps = parsedUrl.protocol === 'https:';
      const transport = isHttps ? require('https') : require('http');

      transport
        .get(
          requestUrl,
          {
            timeout: 120_000,
            headers: {
              'User-Agent': 'Tezbar',
              Accept: 'application/octet-stream',
            },
          },
          (res: any) => {
            // Follow redirects (S3 pre-signed URLs may redirect)
            if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
              res.resume();
              makeRequest(new URL(res.headers.location, requestUrl).toString(), redirectCount + 1);
              return;
            }

            if (res.statusCode !== 200) {
              res.resume();
              reject(new Error(`Download failed: ${res.statusCode} ${res.statusMessage}`));
              return;
            }

            const declaredSize = Number(res.headers['content-length'] || 0);
            if (declaredSize > MAX_EXTENSION_ARCHIVE_BYTES) {
              res.resume();
              reject(new Error('Extension archive is larger than 150 MB'));
              return;
            }

            const chunks: Buffer[] = [];
            let receivedSize = 0;
            res.on('data', (chunk: Buffer) => {
              receivedSize += chunk.length;
              if (receivedSize > MAX_EXTENSION_ARCHIVE_BYTES) {
                res.destroy(new Error('Extension archive is larger than 150 MB'));
                return;
              }
              chunks.push(chunk);
            });
            res.on('error', reject);
            res.on('end', () => {
              try {
                const buffer = Buffer.concat(chunks);
                extractTarGz(buffer, destDir);
                resolve();
              } catch (err) {
                reject(err);
              }
            });
          }
        )
        .on('error', reject);
    };

    makeRequest(url);
  });
}

/**
 * Extract a .tar.gz buffer to a directory.
 * Minimal tar parser that handles POSIX ustar format (sufficient for our bundles).
 */
function extractTarGz(buffer: Buffer, destDir: string): void {
  // Decompress gzip
  const decompressed = zlib.gunzipSync(buffer);
  const destinationRoot = path.resolve(destDir);

  const safeArchivePath = (archivePath: string): string => {
    const normalized = String(archivePath || '').replace(/\\/g, '/');
    if (!normalized || normalized.includes('\0') || normalized.startsWith('/') || /^[A-Za-z]:/.test(normalized)) {
      throw new Error(`Unsafe extension archive path: ${archivePath}`);
    }

    const resolved = path.resolve(destinationRoot, ...normalized.split('/'));
    if (resolved !== destinationRoot && !resolved.startsWith(`${destinationRoot}${path.sep}`)) {
      throw new Error(`Unsafe extension archive path: ${archivePath}`);
    }
    return resolved;
  };

  // Parse tar entries (512-byte blocks)
  let offset = 0;
  while (offset < decompressed.length - 512) {
    // Read header
    const header = decompressed.subarray(offset, offset + 512);

    // Check for end-of-archive (two zero blocks)
    if (header.every((b) => b === 0)) break;

    // Parse tar header fields
    const nameRaw = header.subarray(0, 100).toString('utf-8').replace(/\0+$/, '');
    const sizeOctal = header.subarray(124, 136).toString('utf-8').replace(/\0+$/, '').trim();
    const typeFlag = header[156];
    const prefixRaw = header.subarray(345, 500).toString('utf-8').replace(/\0+$/, '');

    const fullName = prefixRaw ? `${prefixRaw}/${nameRaw}` : nameRaw;
    const size = parseInt(sizeOctal, 8) || 0;

    offset += 512; // Move past header

    if (typeFlag === 53 || fullName.endsWith('/')) {
      // Directory entry (type '5' = 53 in ASCII)
      const dirPath = safeArchivePath(fullName);
      fs.mkdirSync(dirPath, { recursive: true });
    } else if (typeFlag === 0 || typeFlag === 48) {
      // Regular file (type '0' = 48 in ASCII, or 0 = null for old tar)
      const filePath = safeArchivePath(fullName);
      fs.mkdirSync(path.dirname(filePath), { recursive: true });
      const fileData = decompressed.subarray(offset, offset + size);
      fs.writeFileSync(filePath, fileData);
    }
    // Skip other entry types (symlinks, etc.)

    // Move past data blocks (padded to 512 bytes)
    const dataBlocks = Math.ceil(size / 512);
    offset += dataBlocks * 512;
  }
}

// ─── Machine ID ─────────────────────────────────────────────────────

let _machineId: string | null = null;

/**
 * Get or generate a persistent anonymous machine ID for install tracking.
 * Stored in the user data directory — no PII.
 */
function getMachineId(): string {
  if (_machineId) return _machineId;

  const idPath = path.join(app.getPath('userData'), '.machine-id');
  try {
    const existing = fs.readFileSync(idPath, 'utf-8').trim();
    if (existing) {
      _machineId = existing;
      return existing;
    }
  } catch {}

  // Generate a random UUID
  const id = `${randomHex(8)}-${randomHex(4)}-${randomHex(4)}-${randomHex(4)}-${randomHex(12)}`;
  try {
    fs.writeFileSync(idPath, id);
  } catch {}
  _machineId = id;
  return id;
}

function randomHex(length: number): string {
  const bytes = require('crypto').randomBytes(Math.ceil(length / 2));
  return bytes.toString('hex').slice(0, length);
}

/**
 * Uninstall a community extension by name.
 */
export async function uninstallExtension(name: string): Promise<boolean> {
  const installPath = getInstalledPath(name);

  if (!fs.existsSync(installPath)) {
    return true; // Already gone
  }

  try {
    fs.rmSync(installPath, { recursive: true, force: true });
    console.log(`Extension "${name}" uninstalled.`);

    // Report uninstall to backend (fire-and-forget)
    reportUninstall(name, getMachineId()).catch(() => {});

    return true;
  } catch (error) {
    console.error(`Failed to uninstall extension "${name}":`, error);
    return false;
  }
}

// ─── Tezbar IPC Compatibility ───────────────────────────────────────

function normalizeRaymesExtensionId(input: string): string {
  const slug = String(input || '')
    .trim()
    .replace(/^raycast\./, '');
  return slug ? `raycast.${slug}` : '';
}

function slugFromRaymesExtensionId(input: string): string {
  return String(input || '')
    .trim()
    .replace(/^raycast\./, '');
}

function extensionNameFromSlug(slug: string): string {
  return slug
    .split(/[-_]/g)
    .filter(Boolean)
    .map((part) => `${part.slice(0, 1).toUpperCase()}${part.slice(1)}`)
    .join(' ');
}

function readInstalledPackage(slug: string): any {
  const extensionPath = resolveInstalledExtensionPathForRaymes(slug);
  const pkgPath = extensionPath ? path.join(extensionPath, 'package.json') : '';
  if (!fs.existsSync(pkgPath)) return {};
  try {
    return JSON.parse(fs.readFileSync(pkgPath, 'utf-8'));
  } catch {
    return {};
  }
}

function resolvePlatformDefault(value: any): any {
  const platformKey = process.platform === 'win32' ? 'Windows' : 'macOS';
  if (
    value &&
    typeof value === 'object' &&
    !Array.isArray(value) &&
    (Object.prototype.hasOwnProperty.call(value, 'macOS') || Object.prototype.hasOwnProperty.call(value, 'Windows'))
  ) {
    if (Object.prototype.hasOwnProperty.call(value, platformKey)) {
      return value[platformKey];
    }
    return value.macOS ?? value.Windows;
  }
  return value;
}

function normalizeRegistryCommand(command: ExtensionCommandInfo): ExtensionRegistryCommand {
  return {
    name: command.cmdName,
    title: command.title,
    subtitle: command.description || command.extensionTitle,
    description: command.description,
    mode: command.mode,
    argumentDefinitions: command.commandArgumentDefinitions,
  };
}

function githubAvatarUrlForHandle(value: unknown): string | undefined {
  const raw =
    typeof value === 'object' && value
      ? String((value as { handle?: unknown; name?: unknown }).handle || (value as { name?: unknown }).name || '')
      : String(value || '');
  const handle = raw.split('<')[0].split('(')[0].trim().replace(/^@/, '');
  if (!/^[a-z0-9](?:[a-z0-9-]{0,37}[a-z0-9])?$/i.test(handle)) return undefined;
  if (handle.toLowerCase() === 'raycast community') return undefined;
  return `https://github.com/${handle}.png?size=96`;
}

function resolveInstalledIconPath(extensionPath: string, icon: unknown): string | undefined {
  if (typeof icon !== 'string' || !icon.trim()) return undefined;
  if (/^https?:\/\//i.test(icon)) return icon;

  const normalized = icon.replace(/^\.?\//, '');
  const candidates = [path.join(extensionPath, normalized), path.join(extensionPath, 'assets', normalized)];
  return candidates.find((candidate) => fs.existsSync(candidate));
}

function readAppBundleIdentifier(appPath: string): string | undefined {
  const infoPlistPath = path.join(appPath, 'Contents', 'Info.plist');
  if (!fs.existsSync(infoPlistPath)) return undefined;
  try {
    return (
      execFileSync('/usr/bin/plutil', ['-extract', 'CFBundleIdentifier', 'raw', '-o', '-', infoPlistPath], {
        encoding: 'utf8',
        timeout: 1000,
      }).trim() || undefined
    );
  } catch {
    return undefined;
  }
}

function appPickerValue(name: string, appPath: string): Record<string, string> | null {
  if (!fs.existsSync(appPath)) return null;
  return {
    name,
    path: appPath,
    bundleId: readAppBundleIdentifier(appPath) || '',
  };
}

function resolveAppPickerDefault(pref: any): Record<string, string> | string {
  const candidates =
    pref?.name === 'uninstaller_app' || pref?.key === 'uninstaller_app'
      ? [
          appPickerValue('AppCleaner', '/Applications/AppCleaner.app'),
          appPickerValue('Pearcleaner', '/Applications/PearCleaner.app'),
          appPickerValue('TrashMe 3', '/Applications/TrashMe 3.app'),
          appPickerValue('App Cleaner 8', '/Applications/App Cleaner 8.app'),
        ]
      : [];
  return candidates.find((candidate): candidate is Record<string, string> => Boolean(candidate)) || '';
}

export function listInstalledRegistryExtensions(): InstalledRegistryExtension[] {
  const commands = discoverInstalledExtensionCommands();
  const commandsBySlug = new Map<string, ExtensionCommandInfo[]>();
  for (const command of commands) {
    const list = commandsBySlug.get(command.extName) || [];
    list.push(command);
    commandsBySlug.set(command.extName, list);
  }

  return getInstalledExtensionNames()
    .map((slug) => {
      const pkg = readInstalledPackage(slug);
      const extensionPath = resolveInstalledExtensionPathForRaymes(slug) || getInstalledPath(slug);
      const id = normalizeRaymesExtensionId(slug);
      const authorRaw = pkg.author || pkg.owner || '';
      const author =
        typeof authorRaw === 'object' ? String(authorRaw?.name || authorRaw?.handle || '') : String(authorRaw || '');
      const ownerRaw = pkg.owner || pkg.author || '';
      const owner =
        typeof ownerRaw === 'object' ? String(ownerRaw?.handle || ownerRaw?.name || '') : String(ownerRaw || '');
      const authorIconUrl = githubAvatarUrlForHandle(authorRaw);
      const iconPath = resolveInstalledIconPath(extensionPath, pkg.icon || 'icon.png');

      return {
        id,
        slug,
        name: String(pkg.title || extensionNameFromSlug(slug)),
        version: String(pkg.version || '1.0.0'),
        description: String(pkg.description || ''),
        author: author || undefined,
        owner: owner || undefined,
        authorIconUrl,
        iconPath,
        packageJsonPath: path.join(extensionPath, 'package.json'),
        extensionPath,
        commands: (commandsBySlug.get(slug) || []).map(normalizeRegistryCommand),
        installedAt: (() => {
          try {
            return fs.statSync(extensionPath).mtimeMs;
          } catch {
            return Date.now();
          }
        })(),
      } satisfies InstalledRegistryExtension;
    })
    .sort((a, b) => a.name.localeCompare(b.name));
}

export function resolveInstalledPackageJsonPath(extensionId: string): string | null {
  const slug = slugFromRaymesExtensionId(extensionId);
  if (!slug) return null;
  const extensionPath = resolveInstalledExtensionPathForRaymes(slug);
  const pkgPath = extensionPath ? path.join(extensionPath, 'package.json') : '';
  return fs.existsSync(pkgPath) ? pkgPath : null;
}

function searchTokens(value: string): string[] {
  return String(value || '')
    .toLowerCase()
    .split(/[^a-z0-9]+/g)
    .filter(Boolean);
}

function tokenScore(tokens: string[], query: string, exactScore: number, prefixScore: number): number {
  if (tokens.some((token) => token === query)) return exactScore;
  if (tokens.some((token) => token.startsWith(query))) return prefixScore;
  return 0;
}

export function scoreCatalogEntrySearch(entry: CatalogEntry, query: string): number {
  const q = String(query || '')
    .trim()
    .toLowerCase();
  if (!q) return 1;

  const name = String(entry.name || '').toLowerCase();
  const title = String(entry.title || '').toLowerCase();
  const author = String(entry.author || '').toLowerCase();
  const categories = entry.categories || [];
  const commands = entry.commands || [];

  const titleTokens = searchTokens(entry.title);
  const nameTokens = searchTokens(entry.name);
  const authorTokens = searchTokens(author);
  const categoryTokens = searchTokens(categories.join(' '));
  const commandTokens = searchTokens(commands.map((command) => `${command.name} ${command.title}`).join(' '));
  const descriptionTokens = searchTokens(entry.description);

  let score = 0;

  if (name === q || title === q) score += 10000;
  if (name.startsWith(q) || title.startsWith(q)) score += 9000;
  score += tokenScore(nameTokens, q, 8200, 7200);
  score += tokenScore(titleTokens, q, 8000, 7000);
  score += tokenScore(categoryTokens, q, 3000, 2200);
  score += tokenScore(authorTokens, q, 2200, 1600);
  score += tokenScore(commandTokens, q, 1800, 1200);
  score += tokenScore(descriptionTokens, q, 900, 550);

  // Longer queries can safely use substring matching for forgiving search.
  // Short queries like "arc" must not match unrelated words such as "search".
  if (q.length >= 4) {
    if (name.includes(q) || title.includes(q)) score += 1400;
    if (entry.description.toLowerCase().includes(q)) score += 350;
  }

  if (score > 0 && entry.installCount && entry.installCount > 0) {
    score += Math.min(500, Math.log10(entry.installCount) * 80);
  }

  return score;
}

export async function searchExtensionCatalog(query: string): Promise<ExtensionManifest[]> {
  // The Raycast catalog contains macOS-first extensions and is not the right
  // discovery surface on Windows. Keep Windows discovery deterministic and
  // limited to repositories Tezbar has chosen to recommend.
  if (process.platform === 'win32') return searchLovedExtensions(query);

  const q = String(query || '')
    .trim()
    .toLowerCase();
  const catalog = await getCatalog(false);
  return catalog
    .map((entry) => {
      return { entry, score: scoreCatalogEntrySearch(entry, q) };
    })
    .filter(({ score }) => score > 0)
    .sort((a, b) => {
      return (
        b.score - a.score ||
        (b.entry.installCount ?? 0) - (a.entry.installCount ?? 0) ||
        a.entry.title.localeCompare(b.entry.title)
      );
    })
    .slice(0, 200)
    .map(({ entry }) => ({
      id: normalizeRaymesExtensionId(entry.name),
      name: entry.title || extensionNameFromSlug(entry.name),
      description: entry.description || '',
      author: entry.author || entry.contributors?.[0] || 'Raycast Community',
      version: 'latest',
      repository: `https://github.com/raycast/extensions/tree/main/extensions/${entry.name}`,
      downloadCount: entry.installCount,
      icon: entry.icon,
      iconUrl: entry.iconUrl,
      authorIconUrl: githubAvatarUrlForHandle(entry.author || entry.contributors?.[0]),
      screenshotUrls: entry.screenshotUrls,
      categories: entry.categories,
      commands: entry.commands,
      owner: entry.author || undefined,
    }));
}

export async function installRegistryExtension(extensionIdOrSlug: string): Promise<InstalledRegistryExtension> {
  const repository = parseGitHubRepositoryUrl(extensionIdOrSlug);
  if (repository) {
    const repositoryJobKey = `repository:${repository.url.toLowerCase()}`;
    const existingRepositoryJob = extensionInstallJobs.get(repositoryJobKey);
    if (existingRepositoryJob) return existingRepositoryJob;

    const repositoryJob = installRegistryExtensionFromGitHubRepository(repository);
    extensionInstallJobs.set(repositoryJobKey, repositoryJob);
    try {
      return await repositoryJob;
    } finally {
      if (extensionInstallJobs.get(repositoryJobKey) === repositoryJob) {
        extensionInstallJobs.delete(repositoryJobKey);
      }
    }
  }

  const slug = slugFromRaymesExtensionId(extensionIdOrSlug);
  if (!slug) throw new Error('A valid extension id is required');

  const existingJob = extensionInstallJobs.get(slug);
  if (existingJob) return existingJob;

  const job = (async (): Promise<InstalledRegistryExtension> => {
    const ok = await installExtension(slug);
    emitInstallProgress(slug, ok ? 100 : 0);
    if (!ok) throw new Error(`Failed to install extension: ${slug}`);

    const installed = listInstalledRegistryExtensions().find((entry) => entry.slug === slug);
    if (!installed) throw new Error(`Extension installed but could not be loaded: ${slug}`);
    return installed;
  })();
  extensionInstallJobs.set(slug, job);

  try {
    return await job;
  } finally {
    if (extensionInstallJobs.get(slug) === job) {
      extensionInstallJobs.delete(slug);
    }
  }
}

export function uninstallRegistryExtension(extensionIdOrSlug: string): boolean {
  const slug = slugFromRaymesExtensionId(extensionIdOrSlug);
  if (!slug) return false;
  let removed = false;
  for (const candidate of [
    getInstalledPath(slug),
    getLegacyInstalledPath(slug),
    getLegacyRegistryInstalledPath(slug),
  ]) {
    if (fs.existsSync(candidate)) {
      fs.rmSync(candidate, { recursive: true, force: true });
      removed = true;
    }
  }
  return removed || true;
}

export function listInstalledExtensionSlugsFromDisk(): string[] {
  return getInstalledExtensionNames();
}

export function getExtensionPreferences(extensionId: string, commandName?: string): Record<string, unknown> {
  const slug = slugFromRaymesExtensionId(extensionId);
  const pkg = readInstalledPackage(slug);
  const values: Record<string, unknown> = {};

  const applyDefaults = (preferences: any[] | undefined) => {
    for (const pref of preferences || []) {
      if (!pref?.name) continue;
      const resolvedDefault = resolvePlatformDefault(pref.default);
      if (resolvedDefault !== undefined) {
        values[pref.name] = resolvedDefault;
      } else if (pref.type === 'checkbox') {
        values[pref.name] = false;
      } else if (pref.type === 'dropdown') {
        values[pref.name] = pref.data?.[0]?.value ?? '';
      } else if (pref.type === 'appPicker') {
        values[pref.name] = resolveAppPickerDefault(pref);
      } else {
        values[pref.name] = '';
      }
    }
  };

  applyDefaults(Array.isArray(pkg.preferences) ? pkg.preferences : []);
  const command = Array.isArray(pkg.commands) ? pkg.commands.find((cmd: any) => cmd?.name === commandName) : null;
  applyDefaults(Array.isArray(command?.preferences) ? command.preferences : []);

  const extensionPath = resolveInstalledExtensionPathForRaymes(slug) || getInstalledPath(slug);
  const preferencesPath = path.join(extensionPath, 'preferences.json');
  if (fs.existsSync(preferencesPath)) {
    try {
      const saved = JSON.parse(fs.readFileSync(preferencesPath, 'utf-8'));
      if (saved && typeof saved === 'object') {
        Object.assign(values, saved);
        if (commandName && saved.commands?.[commandName]) {
          Object.assign(values, saved.commands[commandName]);
        }
      }
    } catch {}
  }

  return values;
}

export function getExtensionPreferenceSetup(
  extensionId: string,
  commandName?: string
): {
  extensionId: string;
  commandName?: string;
  title: string;
  iconPath?: string;
  preferences: any[];
  values: Record<string, unknown>;
  hasSavedPreferences: boolean;
} {
  const slug = slugFromRaymesExtensionId(extensionId);
  const pkg = readInstalledPackage(slug);
  const extensionPath = resolveInstalledExtensionPathForRaymes(slug) || getInstalledPath(slug);
  const command = Array.isArray(pkg.commands) ? pkg.commands.find((cmd: any) => cmd?.name === commandName) : null;
  const extensionPreferences = Array.isArray(pkg.preferences)
    ? pkg.preferences.map((preference: any) => ({ ...preference }))
    : [];
  const commandPreferences = Array.isArray(command?.preferences)
    ? command.preferences.map((preference: any) => ({
        ...preference,
        commandName,
        commandTitle: String(command?.title || commandName),
      }))
    : [];
  const preferences = [...extensionPreferences, ...commandPreferences];
  const preferencesPath = path.join(extensionPath, 'preferences.json');

  return {
    extensionId: normalizeRaymesExtensionId(slug),
    commandName,
    title: String(pkg.title || extensionNameFromSlug(slug)),
    iconPath: resolveInstalledIconPath(extensionPath, pkg.icon || 'icon.png') || undefined,
    preferences,
    values: getExtensionPreferences(extensionId, commandName),
    hasSavedPreferences: fs.existsSync(preferencesPath),
  };
}

export function shouldShowExtensionPreferenceSetup(extensionId: string, commandName?: string): boolean {
  const setup = getExtensionPreferenceSetup(extensionId, commandName);
  if (setup.preferences.length === 0) return false;
  const needsRequiredValue = setup.preferences.some((pref: any) => {
    if (!pref?.required || !pref?.name) return false;
    const value = setup.values[pref.name];
    return value === undefined || value === null || String(value).trim() === '';
  });
  if (needsRequiredValue) return true;

  const needsCredentialValue = setup.preferences.some((pref: any) => {
    if (pref?.type !== 'password' || !pref?.name) return false;
    const value = setup.values[pref.name];
    return value === undefined || value === null || String(value).trim() === '';
  });
  if (needsCredentialValue) return true;

  // Required fields are onboarding, not optional defaults. Always present
  // them once before the first command run so credentials are explicitly
  // confirmed and persisted for every command in the extension.
  if (!setup.hasSavedPreferences && setup.preferences.some((pref: any) => pref?.required)) {
    return true;
  }

  // Raycast shows this onboarding once for preference-backed extensions. It
  // matters for Google Translate because users need to confirm language
  // direction before no-view commands start copying/pasting text.
  return !setup.hasSavedPreferences && extensionId === 'raycast.google-translate';
}

export function saveExtensionPreferences(
  extensionId: string,
  values: Record<string, unknown>,
  commandName?: string
): Record<string, unknown> {
  const slug = slugFromRaymesExtensionId(extensionId);
  const extensionPath = resolveInstalledExtensionPathForRaymes(slug) || getInstalledPath(slug);
  fs.mkdirSync(extensionPath, { recursive: true });
  const preferencesPath = path.join(extensionPath, 'preferences.json');
  let existing: Record<string, any> = {};
  if (fs.existsSync(preferencesPath)) {
    try {
      const parsed = JSON.parse(fs.readFileSync(preferencesPath, 'utf-8'));
      if (parsed && typeof parsed === 'object') existing = parsed;
    } catch {}
  }

  if (commandName) {
    existing.commands = existing.commands && typeof existing.commands === 'object' ? existing.commands : {};
    existing.commands[commandName] = {
      ...(existing.commands[commandName] || {}),
      ...values,
    };
  } else {
    existing = {
      ...existing,
      ...values,
    };
  }

  fs.writeFileSync(preferencesPath, JSON.stringify(existing, null, 2));
  return getExtensionPreferences(extensionId, commandName);
}
