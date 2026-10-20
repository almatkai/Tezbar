"use strict";
var __create = Object.create;
var __defProp = Object.defineProperty;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __getProtoOf = Object.getPrototypeOf;
var __hasOwnProp = Object.prototype.hasOwnProperty;
var __esm = (fn, res) => function __init() {
  return fn && (res = (0, fn[__getOwnPropNames(fn)[0]])(fn = 0)), res;
};
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};
var __copyProps = (to, from, except, desc) => {
  if (from && typeof from === "object" || typeof from === "function") {
    for (let key of __getOwnPropNames(from))
      if (!__hasOwnProp.call(to, key) && key !== except)
        __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
  }
  return to;
};
var __toESM = (mod, isNodeMode, target) => (target = mod != null ? __create(__getProtoOf(mod)) : {}, __copyProps(
  // If the importer is in node compatibility mode or this is not an ESM
  // file that has been converted to a CommonJS file using a Babel-
  // compatible transform (i.e. "__esModule" has not been set), then set
  // "default" to the CommonJS "module.exports" for node compatibility.
  isNodeMode || !mod || !mod.__esModule ? __defProp(target, "default", { value: mod, enumerable: true }) : target,
  mod
));
var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);

// src/main/extensionWorker/host.ts
function send(message) {
  if (connected) process.stdout.write(`${JSON.stringify(message)}
`);
}
function request(channel, ...payload) {
  if (!connected) return Promise.reject(new Error("Extension host disconnected"));
  const id = (0, import_node_crypto.randomUUID)();
  return new Promise((resolve3, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`Extension host request timed out: ${channel}`));
    }, 12e4);
    pending.set(id, { resolve: (value) => resolve3(value), reject, timer });
    send({ type: "core-request", id, channel, payload });
  });
}
function acceptReply(message) {
  const operation = pending.get(message.id);
  if (!operation) return;
  pending.delete(message.id);
  clearTimeout(operation.timer);
  if (message.error !== void 0) operation.reject(new Error(String(message.error)));
  else operation.resolve(message.result);
}
function disconnect() {
  connected = false;
  for (const operation of pending.values()) {
    clearTimeout(operation.timer);
    operation.reject(new Error("Extension host disconnected"));
  }
  pending.clear();
}
function notify(channel, ...payload) {
  void request(channel, ...payload).catch((error) => console.error(error));
}
var import_node_crypto, pending, connected;
var init_host = __esm({
  "src/main/extensionWorker/host.ts"() {
    "use strict";
    import_node_crypto = require("node:crypto");
    pending = /* @__PURE__ */ new Map();
    connected = true;
  }
});

// src/main/extensionWorker/services.ts
async function refreshHostSnapshot() {
  applications = await request("core:apps:list");
}
function listApplications() {
  return applications;
}
function readAppBundleIdentifier(path4) {
  return applications.find((application) => application.path === path4)?.bundleId;
}
async function askExtensionAI(prompt) {
  return request("core:ai:extension-ask", prompt);
}
async function readSelectedText() {
  return request("core:platform:selected-text");
}
var applications;
var init_services = __esm({
  "src/main/extensionWorker/services.ts"() {
    "use strict";
    init_host();
    applications = [];
  }
});

// src/shared/extensionRepository.ts
function parseGitHubRepositoryUrl(value) {
  const raw = String(value || "").trim();
  if (!raw) return null;
  let parsed;
  try {
    parsed = new URL(raw);
  } catch {
    return null;
  }
  if (parsed.protocol !== "https:" || parsed.username || parsed.password || parsed.port) return null;
  const hostname = parsed.hostname.toLowerCase();
  if (hostname !== "github.com" && hostname !== "www.github.com") return null;
  if (parsed.search || parsed.hash) return null;
  const segments = parsed.pathname.split("/").filter(Boolean);
  if (segments.length !== 2) return null;
  const owner = segments[0] || "";
  const repository = (segments[1] || "").replace(/\.git$/i, "");
  if (!/^[a-z0-9](?:[a-z0-9-]{0,37}[a-z0-9])?$/i.test(owner)) return null;
  if (!/^[a-z0-9._-]+$/i.test(repository)) return null;
  return {
    owner,
    repository,
    url: `https://github.com/${owner}/${repository}`
  };
}
var init_extensionRepository = __esm({
  "src/shared/extensionRepository.ts"() {
    "use strict";
  }
});

// src/main/extensionWorker/desktop.ts
function image(bytes, sourcePath) {
  const value = {
    sourcePath,
    isEmpty: () => bytes.length === 0,
    toPNG: () => {
      if (bytes.length && (bytes.length < 8 || !bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])))) {
        throw new Error(
          "PNG encoding is required for in-memory clipboard images; use a file path for other image formats"
        );
      }
      return bytes;
    },
    getSize: () => bytes.length >= 24 && bytes.readUInt32BE(0) === 2303741511 ? { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) } : { width: 0, height: 0 },
    setTemplateImage: () => {
    },
    resize: () => value
  };
  return value;
}
var import_node_fs, import_node_os, import_node_path, app, visible, window, BrowserWindow, shell, nativeImage, clipboard, dialog;
var init_desktop = __esm({
  "src/main/extensionWorker/desktop.ts"() {
    "use strict";
    import_node_fs = require("node:fs");
    import_node_os = require("node:os");
    import_node_path = require("node:path");
    init_host();
    app = {
      isPackaged: process.env.IS_TAURI === "true",
      name: "Tezbar",
      getPath(name) {
        if (name === "userData") return process.env.APPDATA_DIR || (0, import_node_path.join)((0, import_node_os.homedir)(), ".tezbar");
        if (name === "temp") return process.env.TEMP_DIR || (0, import_node_os.tmpdir)();
        if (name === "home") return (0, import_node_os.homedir)();
        return (0, import_node_path.join)((0, import_node_os.homedir)(), `.${name}`);
      },
      getAppPath: () => process.env.TEZBAR_RESOURCE_DIR || process.cwd(),
      getVersion: () => process.env.APP_VERSION || "0.0.0",
      getName: () => "Tezbar",
      hide: () => notify("core:platform:visibility", false),
      show: () => notify("core:platform:visibility", true),
      focus: () => notify("core:platform:visibility", true)
    };
    visible = true;
    window = {
      webContents: {
        id: 1,
        send: (channel, payload) => send({ type: "event", channel, payload }),
        isDestroyed: () => false
      },
      isDestroyed: () => false,
      isVisible: () => visible,
      hide: () => {
        visible = false;
        app.hide();
      },
      show: () => {
        visible = true;
        app.show();
      },
      focus: () => app.focus()
    };
    BrowserWindow = {
      getAllWindows: () => [window],
      getFocusedWindow: () => window,
      fromWebContents: () => window
    };
    shell = {
      openExternal: async (target) => {
        await request("shell:open", target);
      },
      openPath: async (target) => {
        await request("core:platform:open-file", target);
        return "";
      },
      showItemInFolder: (target) => notify("core:platform:reveal-file", target)
    };
    nativeImage = {
      createFromPath: (path4) => image((0, import_node_fs.existsSync)(path4) ? (0, import_node_fs.readFileSync)(path4) : Buffer.alloc(0), path4),
      createFromBuffer: (bytes) => image(bytes),
      createFromDataURL: (url) => image(Buffer.from(url.split(",")[1] || "", "base64")),
      createEmpty: () => image(Buffer.alloc(0))
    };
    clipboard = {
      clear: async () => {
        await request("clipboard:write", "");
      },
      readText: () => request("clipboard:read"),
      writeText: async (text) => {
        await request("clipboard:write", text);
      },
      writeTextAsync: async (text) => {
        await request("clipboard:write", text);
      },
      write: async (value) => {
        await request("core:platform:clipboard-write", value);
      },
      writeImage: async (value) => {
        if (value.sourcePath) await request("core:platform:clipboard-write", { file: value.sourcePath });
        else
          await request(
            "clipboard:write-png",
            `data:image/png;base64,${value.toPNG().toString("base64")}`
          );
      }
    };
    dialog = {
      showMessageBox: (options) => request("core:platform:confirm", options)
    };
  }
});

// src/main/better-sqlite3-shim.ts
var import_bun_sqlite, import_node_fs2, import_node_path2, StatementShim, DatabaseShim, better_sqlite3_shim_default;
var init_better_sqlite3_shim = __esm({
  "src/main/better-sqlite3-shim.ts"() {
    "use strict";
    import_bun_sqlite = require("bun:sqlite");
    import_node_fs2 = require("node:fs");
    import_node_path2 = require("node:path");
    if (process.platform === "darwin") {
      const sqliteCandidates = [
        process.env.TEZBAR_SQLITE_LIBRARY_PATH,
        (0, import_node_path2.join)(__dirname, "libsqlite3.dylib"),
        "/opt/homebrew/opt/sqlite/lib/libsqlite3.dylib",
        "/usr/local/opt/sqlite/lib/libsqlite3.dylib",
        "/usr/local/opt/sqlite3/lib/libsqlite3.dylib"
      ].filter((path4) => Boolean(path4 && (0, import_node_fs2.existsSync)(path4)));
      const sqliteLibrary = sqliteCandidates[0];
      if (sqliteLibrary) {
        try {
          import_bun_sqlite.Database.setCustomSQLite(sqliteLibrary);
        } catch (error) {
          console.warn("[SQLite] Could not enable loadable extensions:", error);
        }
      }
    }
    StatementShim = class {
      _stmt;
      constructor(stmt) {
        this._stmt = stmt;
      }
      run(...params) {
        const result = this._stmt.run(...params);
        return {
          changes: result?.changes ?? 0,
          lastInsertRowid: result?.lastInsertRowid ?? 0
        };
      }
      get(...params) {
        return this._stmt.get(...params) ?? void 0;
      }
      all(...params) {
        return this._stmt.all(...params) ?? [];
      }
    };
    DatabaseShim = class {
      _db;
      constructor(filename, options) {
        this._db = new import_bun_sqlite.Database(filename, {
          readonly: options?.readonly ?? false,
          create: !options?.fileMustExist
        });
      }
      close() {
        this._db.close();
      }
      pragma(value) {
        this._db.exec(`PRAGMA ${value}`);
      }
      exec(sql) {
        this._db.exec(sql);
      }
      prepare(sql) {
        const stmt = this._db.prepare(sql);
        return new StatementShim(stmt);
      }
      loadExtension(path4) {
        this._db.loadExtension(path4);
      }
      transaction(fn) {
        return this._db.transaction(fn);
      }
    };
    better_sqlite3_shim_default = DatabaseShim;
  }
});

// src/main/extension-sql.ts
async function executeExtensionSQL(databasePath, query) {
  const database = new better_sqlite3_shim_default(databasePath, { readonly: true, fileMustExist: true });
  try {
    return database.prepare(query).all();
  } finally {
    database.close();
  }
}
var init_extension_sql = __esm({
  "src/main/extension-sql.ts"() {
    "use strict";
    init_better_sqlite3_shim();
  }
});

// src/main/extensionWorker/clipboard.ts
async function copyClipboardContent(value) {
  if (value && typeof value === "object") {
    const payload = value;
    if (typeof payload.file === "string" && payload.file.trim()) {
      const path4 = payload.file.trim();
      if ([".png", ".jpg", ".jpeg", ".gif", ".tif", ".tiff", ".bmp", ".webp"].includes(
        (0, import_node_path3.extname)(path4).toLowerCase()
      )) {
        const image2 = nativeImage.createFromPath(path4);
        if (image2.isEmpty()) throw new Error(`Clipboard image is unavailable: ${path4}`);
        await clipboard.writeImage(image2);
      } else {
        await clipboard.writeText(path4);
      }
      return;
    }
    if (typeof payload.html === "string") {
      await clipboard.write({
        html: payload.html,
        text: typeof payload.text === "string" ? payload.text : payload.html
      });
      return;
    }
    if (typeof payload.text === "string") {
      await clipboard.writeText(payload.text);
      return;
    }
  }
  await clipboard.writeText(String(value ?? ""));
}
async function pasteClipboardContent() {
  await request("core:platform:paste");
}
var import_node_path3;
var init_clipboard = __esm({
  "src/main/extensionWorker/clipboard.ts"() {
    "use strict";
    init_desktop();
    import_node_path3 = require("node:path");
    init_host();
  }
});

// src/main/extension-confirm-alert.ts
async function confirmExtensionAlert(options = {}) {
  const result = await dialog.showMessageBox({
    title: String(options.title || "Confirm"),
    message: String(options.title || "Confirm"),
    detail: options.message ? String(options.message) : void 0,
    buttons: [
      String(options.dismissAction?.title || "Cancel"),
      String(options.primaryAction?.title || "OK")
    ],
    defaultId: 0,
    cancelId: 0,
    type: options.primaryAction?.style === "destructive" ? "warning" : "question",
    noLink: true
  });
  const accepted = result.response === 1;
  const callback = accepted ? options.primaryAction?.onAction : options.dismissAction?.onAction;
  if (callback) await Promise.resolve(callback());
  return accepted;
}
var init_extension_confirm_alert = __esm({
  "src/main/extension-confirm-alert.ts"() {
    "use strict";
    init_desktop();
  }
});

// src/main/powershell-script.ts
function powerShellScriptArguments(source) {
  const script = `[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
${source}`;
  return [
    "-NoLogo",
    "-NoProfile",
    "-NonInteractive",
    "-ExecutionPolicy",
    "Bypass",
    "-EncodedCommand",
    Buffer.from(script, "utf16le").toString("base64")
  ];
}
async function runPowerShellScript(source, options = {}) {
  if (process.platform !== "win32") throw new Error("PowerShell is only available on Windows");
  if (typeof source !== "string" || !source.trim()) return "";
  const { stdout } = await execFileAsync("powershell.exe", powerShellScriptArguments(source), {
    encoding: "utf8",
    maxBuffer: 10 * 1024 * 1024,
    timeout: options.timeout ?? 1e4,
    signal: options.signal,
    windowsHide: true
  });
  return stdout.replace(/\r?\n$/, "");
}
var import_node_child_process, import_node_util, execFileAsync;
var init_powershell_script = __esm({
  "src/main/powershell-script.ts"() {
    "use strict";
    import_node_child_process = require("node:child_process");
    import_node_util = require("node:util");
    execFileAsync = (0, import_node_util.promisify)(import_node_child_process.execFile);
  }
});

// src/main/esbuild-runtime.ts
function configurePackagedEsbuildBinary() {
}
var init_esbuild_runtime = __esm({
  "src/main/esbuild-runtime.ts"() {
    "use strict";
  }
});

// src/main/extensionWorker/windowState.ts
function setSuppressBlurHide(value) {
  notify("core:platform:suppress-blur", value);
}
var init_windowState = __esm({
  "src/main/extensionWorker/windowState.ts"() {
    init_host();
  }
});

// src/main/extension-platform.ts
function normalizePlatform(value) {
  if (typeof value !== "string") return null;
  const normalized = value.trim().toLowerCase();
  if (!normalized) return null;
  if (normalized === "macos" || normalized === "darwin" || normalized === "mac") {
    return "macOS";
  }
  if (normalized === "windows" || normalized === "win32" || normalized === "win") {
    return "Windows";
  }
  if (normalized === "linux") {
    return "Linux";
  }
  return null;
}
function getCurrentRaycastPlatform() {
  if (process.platform === "win32") return "Windows";
  if (process.platform === "linux") return "Linux";
  return "macOS";
}
function getManifestPlatforms(manifest) {
  if (!manifest || typeof manifest !== "object") return [];
  const declared = Array.isArray(manifest.platforms) ? manifest.platforms : Array.isArray(manifest.tezbar?.platforms) ? manifest.tezbar.platforms : ["macOS"];
  const supported = /* @__PURE__ */ new Set();
  for (const raw of declared) {
    const normalized = normalizePlatform(raw);
    if (normalized) supported.add(normalized);
  }
  return [...supported];
}
function isManifestPlatformCompatible(manifest) {
  const supported = getManifestPlatforms(manifest);
  if (supported.length === 0) return false;
  return supported.includes(getCurrentRaycastPlatform());
}
function isCommandPlatformCompatible(cmd) {
  if (!cmd || typeof cmd !== "object") return false;
  if (!Object.prototype.hasOwnProperty.call(cmd, "platforms")) return true;
  return isManifestPlatformCompatible(cmd);
}
var init_extension_platform = __esm({
  "src/main/extension-platform.ts"() {
    "use strict";
  }
});

// src/main/extension-builder.ts
var extension_builder_exports = {};
__export(extension_builder_exports, {
  buildAllCommands: () => buildAllCommands,
  buildSingleCommand: () => buildSingleCommand,
  discoverInstalledExtensionCommands: () => discoverInstalledExtensionCommands,
  getExtensionBundle: () => getExtensionBundle,
  getInstalledExtensionsSettingsSchema: () => getInstalledExtensionsSettingsSchema
});
function requireEsbuild() {
  configurePackagedEsbuildBinary();
  return require("esbuild");
}
function legacyCheerioInteropPlugin() {
  return {
    name: "legacy-cheerio-default-interop",
    setup(build) {
      build.onLoad({ filter: /\.[cm]?[jt]sx?$/ }, (args) => {
        const source = fs.readFileSync(args.path, "utf8");
        if (!/import\s+[A-Za-z_$][\w$]*\s+from\s+['"]cheerio['"]/.test(source)) return null;
        const extension = path.extname(args.path).toLowerCase();
        const loader = extension.endsWith("x") ? extension.slice(1) : extension.slice(1) || "js";
        return {
          contents: source.replace(
            /import\s+([A-Za-z_$][\w$]*)\s+from\s+(['"])cheerio\2/g,
            "import * as $1 from $2cheerio$2"
          ),
          loader
        };
      });
    }
  };
}
function nativeExtensionModulePlugin() {
  return {
    name: "native-extension-module-external",
    setup(build) {
      build.onResolve({ filter: /^(?:swift|rust):/ }, (args) => ({
        path: args.path,
        external: true
      }));
    }
  };
}
function getManagedExtensionsDir() {
  const dir = path.join(app.getPath("userData"), "extensions");
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  return dir;
}
function getBuildDir(extPath) {
  const dir = path.join(extPath, ".sc-build");
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  return dir;
}
function expandHome(inputPath) {
  const raw = String(inputPath || "").trim();
  if (!raw) return "";
  if (raw.startsWith("~/")) return path.join(os.homedir(), raw.slice(2));
  return raw;
}
function normalizeFsPath(inputPath) {
  return path.resolve(expandHome(inputPath));
}
function normalizeExtensionName(name) {
  const raw = String(name || "").trim();
  if (!raw) return "";
  return raw.replace(/^@/, "").replace(/^raycast\./, "").replace(/[\\/]/g, "-");
}
function getConfiguredExtensionRoots() {
  const settingsPaths = [];
  const envPaths = String(process.env.SUPERCMD_EXTENSION_PATHS || "").split(path.delimiter).map((value) => value.trim()).filter(Boolean);
  const unique = /* @__PURE__ */ new Set();
  for (const root of [
    getManagedExtensionsDir(),
    path.join(getManagedExtensionsDir(), "packages"),
    path.join(app.getPath("userData"), "extension-registry", "packages"),
    ...settingsPaths,
    ...envPaths
  ]) {
    const normalized = normalizeFsPath(root);
    if (!normalized) continue;
    unique.add(normalized);
  }
  return [...unique];
}
function collectInstalledExtensions() {
  const results = [];
  const seen = /* @__PURE__ */ new Set();
  const addIfValid = (extPath, sourceRoot, fallbackName) => {
    const pkgPath = path.join(extPath, "package.json");
    if (!fs.existsSync(pkgPath)) return;
    try {
      if (!fs.statSync(extPath).isDirectory()) return;
    } catch {
      return;
    }
    const extName = normalizeExtensionName(fallbackName);
    if (!extName) return;
    const dedupeKey = extName.toLowerCase();
    if (seen.has(dedupeKey)) return;
    seen.add(dedupeKey);
    results.push({ extName, extPath, sourceRoot });
  };
  for (const sourceRoot of getConfiguredExtensionRoots()) {
    if (!fs.existsSync(sourceRoot)) continue;
    const sourceRootPkg = path.join(sourceRoot, "package.json");
    if (fs.existsSync(sourceRootPkg)) {
      addIfValid(sourceRoot, sourceRoot, path.basename(sourceRoot));
      continue;
    }
    let entries = [];
    try {
      entries = fs.readdirSync(sourceRoot);
    } catch {
      continue;
    }
    for (const entry of entries) {
      addIfValid(path.join(sourceRoot, entry), sourceRoot, entry);
    }
  }
  return results;
}
function resolveInstalledExtensionPath(extName) {
  const normalized = normalizeExtensionName(extName);
  if (!normalized) return null;
  const match = collectInstalledExtensions().find((entry) => entry.extName === normalized);
  return match?.extPath || null;
}
function getExtensionIconDataUrl(extPath, iconFile) {
  const candidates = [path.join(extPath, "assets", iconFile), path.join(extPath, iconFile)];
  for (const p of candidates) {
    if (!fs.existsSync(p)) continue;
    try {
      const ext = path.extname(p).toLowerCase();
      const data = fs.readFileSync(p);
      if (data.length < 50) continue;
      const mime = ext === ".svg" ? "image/svg+xml" : ext === ".jpg" || ext === ".jpeg" ? "image/jpeg" : "image/png";
      return `data:${mime};base64,${data.toString("base64")}`;
    } catch {
    }
  }
  return void 0;
}
function resolvePlatformDefault(value) {
  const platformKey = process.platform === "win32" ? "Windows" : "macOS";
  if (value && typeof value === "object" && !Array.isArray(value) && (Object.prototype.hasOwnProperty.call(value, "macOS") || Object.prototype.hasOwnProperty.call(value, "Windows"))) {
    if (Object.prototype.hasOwnProperty.call(value, platformKey)) {
      return value[platformKey];
    }
    return value.macOS ?? value.Windows;
  }
  return value;
}
function normalizePreferenceSchema(pref, scope) {
  if (!pref || typeof pref !== "object" || !pref.name) return null;
  return {
    scope,
    name: String(pref.name),
    title: pref.title,
    label: pref.label,
    description: pref.description,
    placeholder: pref.placeholder,
    required: Boolean(pref.required),
    type: pref.type,
    default: resolvePlatformDefault(pref.default),
    data: Array.isArray(pref.data) ? pref.data : void 0
  };
}
function discoverInstalledExtensionCommands() {
  const results = [];
  for (const source of collectInstalledExtensions()) {
    const extPath = source.extPath;
    const pkgPath = path.join(extPath, "package.json");
    const extName = source.extName;
    try {
      const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf-8"));
      if (!isManifestPlatformCompatible(pkg)) continue;
      const iconDataUrl = getExtensionIconDataUrl(extPath, pkg.icon || "icon.png");
      const ownerRaw = pkg.owner || pkg.author || "";
      const owner = (typeof ownerRaw === "object" ? ownerRaw?.name || "" : String(ownerRaw || "")).trim();
      for (const cmd of pkg.commands || []) {
        if (!cmd.name) continue;
        if (!isCommandPlatformCompatible(cmd)) continue;
        results.push({
          id: `ext-${extName}-${cmd.name}`,
          title: cmd.title || cmd.name,
          extensionTitle: pkg.title || extName,
          extName,
          cmdName: cmd.name,
          owner: owner || void 0,
          description: cmd.description || "",
          mode: cmd.mode || "view",
          interval: typeof cmd.interval === "string" ? cmd.interval : void 0,
          disabledByDefault: Boolean(cmd.disabledByDefault),
          commandArgumentDefinitions: Array.isArray(cmd.arguments) ? cmd.arguments.filter((arg) => arg && arg.name).map((arg) => ({
            name: String(arg.name),
            required: Boolean(arg.required),
            type: arg.type,
            placeholder: arg.placeholder,
            title: arg.title,
            data: Array.isArray(arg.data) ? arg.data : void 0
          })) : [],
          keywords: [extName, pkg.title || "", cmd.name, cmd.title || "", cmd.description || ""].filter(Boolean).map((s) => s.toLowerCase()),
          iconDataUrl
        });
      }
    } catch {
    }
  }
  return results;
}
function getInstalledExtensionsSettingsSchema() {
  const results = [];
  for (const source of collectInstalledExtensions()) {
    const extPath = source.extPath;
    const pkgPath = path.join(extPath, "package.json");
    const extName = source.extName;
    try {
      const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf-8"));
      if (!isManifestPlatformCompatible(pkg)) continue;
      const iconDataUrl = getExtensionIconDataUrl(extPath, pkg.icon || "icon.png");
      const ownerRaw = pkg.owner || pkg.author || "";
      const owner = typeof ownerRaw === "object" ? ownerRaw.name || "" : String(ownerRaw || "");
      const extensionPreferences = Array.isArray(pkg.preferences) ? pkg.preferences.map((pref) => normalizePreferenceSchema(pref, "extension")).filter(Boolean) : [];
      const commands = Array.isArray(pkg.commands) ? pkg.commands.filter((cmd) => cmd && cmd.name && isCommandPlatformCompatible(cmd)).map((cmd) => ({
        name: cmd.name,
        title: cmd.title || cmd.name,
        description: cmd.description || "",
        mode: cmd.mode || "view",
        interval: typeof cmd.interval === "string" ? cmd.interval : void 0,
        disabledByDefault: Boolean(cmd.disabledByDefault),
        preferences: Array.isArray(cmd.preferences) ? cmd.preferences.map((pref) => normalizePreferenceSchema(pref, "command")).filter(Boolean) : []
      })) : [];
      results.push({
        extName,
        title: pkg.title || extName,
        description: pkg.description || "",
        owner,
        iconDataUrl,
        preferences: extensionPreferences,
        commands
      });
    } catch {
    }
  }
  return results.sort((a, b) => a.title.localeCompare(b.title));
}
function getInstallableRuntimeDeps(pkg) {
  const deps = {
    ...pkg?.dependencies || {},
    ...pkg?.optionalDependencies || {}
  };
  return Object.entries(deps).filter(([name]) => typeof name === "string" && !name.startsWith("@raycast/")).map(([name, version]) => `${name}@${String(version || "").trim()}`).filter((value) => {
    const atIndex = value.lastIndexOf("@");
    return atIndex > 0 && atIndex < value.length - 1;
  });
}
function extensionRequiresNodeModules(pkg) {
  return getInstallableRuntimeDeps(pkg).length > 0;
}
function parseJsonc(source) {
  let out = "";
  let i = 0;
  const n = source.length;
  while (i < n) {
    const ch = source[i];
    if (ch === '"') {
      out += ch;
      i++;
      while (i < n) {
        const c = source[i];
        out += c;
        i++;
        if (c === "\\" && i < n) {
          out += source[i];
          i++;
          continue;
        }
        if (c === '"') break;
      }
      continue;
    }
    if (ch === "/" && source[i + 1] === "/") {
      i += 2;
      while (i < n && source[i] !== "\n") i++;
      continue;
    }
    if (ch === "/" && source[i + 1] === "*") {
      i += 2;
      while (i < n && !(source[i] === "*" && source[i + 1] === "/")) i++;
      i += 2;
      continue;
    }
    out += ch;
    i++;
  }
  out = out.replace(/,(\s*[}\]])/g, "$1");
  return JSON.parse(out);
}
function getExtensionCompilerOptions(extPath) {
  const tsconfigPath = path.join(extPath, "tsconfig.json");
  if (!fs.existsSync(tsconfigPath)) return {};
  try {
    const parsed = parseJsonc(fs.readFileSync(tsconfigPath, "utf-8"));
    const compilerOptions = parsed && typeof parsed === "object" && parsed.compilerOptions && typeof parsed.compilerOptions === "object" ? parsed.compilerOptions : {};
    const options = {};
    if (typeof compilerOptions.baseUrl === "string" && compilerOptions.baseUrl.trim()) {
      options.baseUrl = compilerOptions.baseUrl;
    }
    if (compilerOptions.paths && typeof compilerOptions.paths === "object" && !Array.isArray(compilerOptions.paths)) {
      options.paths = compilerOptions.paths;
      if (!options.baseUrl) options.baseUrl = ".";
    }
    if (typeof compilerOptions.jsx === "string" && compilerOptions.jsx.trim()) {
      options.jsx = compilerOptions.jsx;
    }
    if (typeof compilerOptions.jsxImportSource === "string" && compilerOptions.jsxImportSource.trim()) {
      options.jsxImportSource = compilerOptions.jsxImportSource;
    }
    return options;
  } catch (error) {
    console.warn(`Failed to parse tsconfig for ${path.basename(extPath)}:`, error?.message || error);
    return {};
  }
}
function getEsbuildTsconfigRaw(extPath) {
  const extensionCompilerOptions = getExtensionCompilerOptions(extPath);
  return JSON.stringify({
    compilerOptions: {
      target: "ES2020",
      jsx: "react-jsx",
      jsxImportSource: "react",
      strict: false,
      esModuleInterop: true,
      moduleResolution: "node",
      ...extensionCompilerOptions
    }
  });
}
function resolveEntryFile(extPath, cmd) {
  const cmdName = String(cmd?.name || "").trim();
  if (!cmdName) return null;
  const srcDir = path.join(extPath, "src");
  const validExt = /\.(tsx?|jsx?)$/i;
  const explicitEntry = typeof cmd?.path === "string" ? cmd.path : typeof cmd?.entrypoint === "string" ? cmd.entrypoint : typeof cmd?.entry === "string" ? cmd.entry : typeof cmd?.file === "string" ? cmd.file : typeof cmd?.source === "string" ? cmd.source : "";
  const candidates = [
    explicitEntry ? path.join(extPath, explicitEntry) : "",
    path.join(srcDir, `${cmdName}.tsx`),
    path.join(srcDir, `${cmdName}.ts`),
    path.join(srcDir, `${cmdName}.jsx`),
    path.join(srcDir, `${cmdName}.js`),
    path.join(srcDir, cmdName, "index.tsx"),
    path.join(srcDir, cmdName, "index.ts"),
    path.join(srcDir, cmdName, "index.jsx"),
    path.join(srcDir, cmdName, "index.js"),
    path.join(srcDir, "commands", `${cmdName}.tsx`),
    path.join(srcDir, "commands", `${cmdName}.ts`),
    path.join(srcDir, "commands", `${cmdName}.jsx`),
    path.join(srcDir, "commands", `${cmdName}.js`)
  ].filter(Boolean);
  const found = candidates.find((p) => fs.existsSync(p));
  if (found) return found;
  if (!fs.existsSync(srcDir)) return null;
  const stack = [srcDir];
  const normalized = cmdName.toLowerCase();
  while (stack.length > 0) {
    const dir = stack.pop();
    let entries = [];
    try {
      entries = fs.readdirSync(dir);
    } catch {
      continue;
    }
    for (const entry of entries) {
      const full = path.join(dir, entry);
      let stat;
      try {
        stat = fs.statSync(full);
      } catch {
        continue;
      }
      if (stat.isDirectory()) {
        stack.push(full);
        continue;
      }
      if (!validExt.test(entry)) continue;
      const base = path.basename(entry, path.extname(entry)).toLowerCase();
      if (base === normalized) return full;
    }
  }
  return null;
}
async function buildAllCommands(extName, extPathOverride) {
  const extPath = extPathOverride ? normalizeFsPath(extPathOverride) : resolveInstalledExtensionPath(extName);
  if (!extPath) {
    console.error(`Extension path not found for ${extName}`);
    return 0;
  }
  const pkgPath = path.join(extPath, "package.json");
  if (!fs.existsSync(pkgPath)) {
    console.error(`No package.json found for extension ${extName}`);
    return 0;
  }
  let commands;
  let requiresNodeModules = false;
  let manifestExternal = [];
  try {
    const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf-8"));
    if (!isManifestPlatformCompatible(pkg)) {
      console.warn(`Skipping build for incompatible extension ${extName}`);
      return 0;
    }
    commands = pkg.commands || [];
    requiresNodeModules = extensionRequiresNodeModules(pkg);
    manifestExternal = Array.isArray(pkg.external) ? pkg.external.filter((v) => typeof v === "string" && v.trim().length > 0) : [];
  } catch {
    return 0;
  }
  if (commands.length === 0) return 0;
  const esbuild = requireEsbuild();
  const extNodeModules = path.join(extPath, "node_modules");
  if (requiresNodeModules && !fs.existsSync(extNodeModules)) {
    try {
      const { installExtensionDeps: installDeps } = { installExtensionDeps };
      await installDeps(extPath);
    } catch (e) {
      console.error(`Failed to install dependencies for ${extName}:`, e?.message || e);
      return 0;
    }
    if (!fs.existsSync(extNodeModules)) {
      console.error(`Dependencies missing for ${extName}: ${extNodeModules} not found`);
      return 0;
    }
  }
  const buildDir = getBuildDir(extPath);
  try {
    fs.rmSync(buildDir, { recursive: true, force: true });
  } catch {
  }
  fs.mkdirSync(buildDir, { recursive: true });
  let built = 0;
  for (const cmd of commands) {
    if (!cmd.name) continue;
    if (!isCommandPlatformCompatible(cmd)) continue;
    const entryFile = resolveEntryFile(extPath, cmd);
    if (!entryFile) {
      console.warn(`No entry file for ${extName}/${cmd.name}, skipping`);
      continue;
    }
    const outFile = path.join(buildDir, `${cmd.name}.js`);
    fs.mkdirSync(path.dirname(outFile), { recursive: true });
    try {
      console.log(`  Building ${extName}/${cmd.name}\u2026`);
      await runEsbuildBuild(
        esbuild,
        {
          entryPoints: [entryFile],
          absWorkingDir: extPath,
          bundle: true,
          format: "cjs",
          platform: "node",
          conditions: ["require", "node"],
          outfile: outFile,
          plugins: [legacyCheerioInteropPlugin(), nativeExtensionModulePlugin()],
          external: [
            // React — provided by the renderer at runtime
            "react",
            "react-dom",
            "react-dom/*",
            "react/jsx-runtime",
            "react/jsx-dev-runtime",
            // Raycast — provided by our shim
            "@raycast/api",
            "@raycast/utils",
            // Native C++ addons — cannot be bundled, we stub them at runtime
            "re2",
            "better-sqlite3",
            "fsevents",
            // Cross-extension calls — not supported, stubbed
            "raycast-cross-extension",
            // Fetch libs — use runtime shims in renderer instead of bundling Node internals
            "node-fetch",
            "undici",
            "undici/*",
            // HTTP / file-download / archive packages — must be kept external so our renderer
            // shim can intercept them and route file I/O through the main process (which has
            // real filesystem access). Bundling them inline breaks binary downloads because the
            // browser renderer cannot do streaming file writes or archive extraction natively.
            "axios",
            "tar",
            "extract-zip",
            "sha256-file",
            // Respect extension-defined externals from manifest
            ...manifestExternal,
            // Node.js built-ins — stubbed at runtime in the renderer
            ...nodeBuiltins
          ],
          nodePaths: fs.existsSync(extNodeModules) ? [extNodeModules] : [],
          target: "es2020",
          jsx: "automatic",
          jsxImportSource: "react",
          tsconfigRaw: getEsbuildTsconfigRaw(extPath),
          define: {
            "process.env.NODE_ENV": '"production"',
            global: "globalThis"
          },
          logLevel: "warning"
        },
        extPath,
        `${extName}/${cmd.name}`
      );
      if (fs.existsSync(outFile)) {
        built++;
      }
    } catch (e) {
      console.error(`  esbuild failed for ${extName}/${cmd.name}:`, e);
    }
  }
  console.log(`Built ${built}/${commands.length} commands for ${extName}`);
  return built;
}
function parsePreferences(pkg, cmdName) {
  const extensionPrefs = {};
  const commandPrefs = {};
  const definitions = [];
  for (const pref of pkg.preferences || []) {
    if (!pref.name) continue;
    const resolvedDefault = resolvePlatformDefault(pref.default);
    definitions.push({
      scope: "extension",
      name: pref.name,
      title: pref.title,
      description: pref.description,
      placeholder: pref.placeholder,
      required: Boolean(pref.required),
      type: pref.type,
      default: resolvedDefault,
      data: Array.isArray(pref.data) ? pref.data : void 0
    });
    if (resolvedDefault !== void 0) {
      extensionPrefs[pref.name] = resolvedDefault;
    } else if (pref.type === "checkbox") {
      extensionPrefs[pref.name] = false;
    } else if (pref.type === "textfield" || pref.type === "password") {
      extensionPrefs[pref.name] = "";
    } else if (pref.type === "dropdown") {
      extensionPrefs[pref.name] = pref.data?.[0]?.value ?? "";
    }
  }
  const cmd = (pkg.commands || []).find((c) => c.name === cmdName);
  if (cmd?.preferences) {
    for (const pref of cmd.preferences) {
      if (!pref.name) continue;
      const resolvedDefault = resolvePlatformDefault(pref.default);
      definitions.push({
        scope: "command",
        name: pref.name,
        title: pref.title,
        description: pref.description,
        placeholder: pref.placeholder,
        required: Boolean(pref.required),
        type: pref.type,
        default: resolvedDefault,
        data: Array.isArray(pref.data) ? pref.data : void 0
      });
      if (resolvedDefault !== void 0) {
        commandPrefs[pref.name] = resolvedDefault;
      } else if (pref.type === "checkbox") {
        commandPrefs[pref.name] = false;
      } else if (pref.type === "textfield" || pref.type === "password") {
        commandPrefs[pref.name] = "";
      } else if (pref.type === "dropdown") {
        commandPrefs[pref.name] = pref.data?.[0]?.value ?? "";
      }
    }
  }
  return { extensionPrefs, commandPrefs, definitions };
}
async function buildSingleCommand(extName, cmdName) {
  const extPath = resolveInstalledExtensionPath(extName);
  if (!extPath) {
    console.error(`buildSingleCommand: extension path not found for ${extName}`);
    return false;
  }
  const pkgPath = path.join(extPath, "package.json");
  if (!fs.existsSync(pkgPath)) {
    console.error(`buildSingleCommand: package.json not found at ${pkgPath}`);
    return false;
  }
  let cmd;
  let requiresNodeModules = false;
  let manifestExternal = [];
  try {
    const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf-8"));
    if (!isManifestPlatformCompatible(pkg)) {
      console.error(`buildSingleCommand: platform not compatible for ${extName}`);
      return false;
    }
    const commands = pkg.commands || [];
    cmd = commands.find((c) => c.name === cmdName);
    requiresNodeModules = extensionRequiresNodeModules(pkg);
    manifestExternal = Array.isArray(pkg.external) ? pkg.external.filter((v) => typeof v === "string" && v.trim().length > 0) : [];
  } catch (e) {
    console.error(`buildSingleCommand: failed to parse package.json for ${extName}:`, e?.message);
    return false;
  }
  if (!cmd) {
    console.error(`buildSingleCommand: command "${cmdName}" not found in ${extName} package.json`);
    return false;
  }
  if (!isCommandPlatformCompatible(cmd)) {
    console.error(`buildSingleCommand: command "${cmdName}" not compatible with current platform`);
    return false;
  }
  const entryFile = resolveEntryFile(extPath, cmd);
  if (!entryFile) {
    console.error(`buildSingleCommand: entry file not found for ${extName}/${cmdName}`);
    return false;
  }
  const buildDir = getBuildDir(extPath);
  fs.mkdirSync(buildDir, { recursive: true });
  const outFile = path.join(buildDir, `${cmdName}.js`);
  fs.mkdirSync(path.dirname(outFile), { recursive: true });
  const extNodeModules = path.join(extPath, "node_modules");
  if (requiresNodeModules && !fs.existsSync(extNodeModules)) {
    console.log(`  node_modules missing for ${extName}, installing dependencies\u2026`);
    try {
      const { installExtensionDeps: installDeps } = { installExtensionDeps };
      await installDeps(extPath);
    } catch (e) {
      console.error(`  Failed to install dependencies for ${extName}:`, e?.message);
      return false;
    }
    if (!fs.existsSync(extNodeModules)) return false;
  }
  try {
    const esbuild = requireEsbuild();
    console.log(`  On-demand building ${extName}/${cmdName}\u2026`);
    await runEsbuildBuild(
      esbuild,
      {
        entryPoints: [entryFile],
        absWorkingDir: extPath,
        bundle: true,
        format: "cjs",
        platform: "node",
        conditions: ["require", "node"],
        outfile: outFile,
        plugins: [legacyCheerioInteropPlugin(), nativeExtensionModulePlugin()],
        external: [
          "react",
          "react-dom",
          "react-dom/*",
          "react/jsx-runtime",
          "react/jsx-dev-runtime",
          "@raycast/api",
          "@raycast/utils",
          "re2",
          "better-sqlite3",
          "fsevents",
          "raycast-cross-extension",
          "node-fetch",
          "undici",
          "undici/*",
          "axios",
          "tar",
          "extract-zip",
          "sha256-file",
          ...manifestExternal,
          ...nodeBuiltins
        ],
        nodePaths: fs.existsSync(extNodeModules) ? [extNodeModules] : [],
        target: "es2020",
        jsx: "automatic",
        jsxImportSource: "react",
        tsconfigRaw: getEsbuildTsconfigRaw(extPath),
        define: {
          "process.env.NODE_ENV": '"production"',
          global: "globalThis"
        },
        logLevel: "warning"
      },
      extPath,
      `${extName}/${cmdName}`
    );
    return fs.existsSync(outFile);
  } catch (e) {
    console.error(`  On-demand esbuild failed for ${extName}/${cmdName}:`, e);
    lastBuildError.set(`${extName}/${cmdName}`, e?.message || String(e));
    return false;
  }
}
function extractMissingBareImports(error) {
  const errors = Array.isArray(error?.errors) ? error.errors : [];
  const found = /* @__PURE__ */ new Set();
  for (const err of errors) {
    const text = String(err?.text || "");
    const match = text.match(/Could not resolve\s+"([^"]+)"/);
    if (!match) continue;
    const specifier = match[1];
    if (!specifier || specifier.startsWith(".") || specifier.startsWith("/") || specifier.includes(":")) {
      continue;
    }
    const parts = specifier.split("/");
    const pkgName = specifier.startsWith("@") ? parts.slice(0, 2).join("/") : parts[0];
    if (!pkgName) continue;
    if (nodeBuiltins.includes(pkgName)) continue;
    if (pkgName.startsWith("@raycast/")) continue;
    found.add(pkgName);
  }
  return [...found];
}
async function runEsbuildBuild(esbuild, options, extPath, label) {
  try {
    await esbuild.build(options);
  } catch (error) {
    const missing = extractMissingBareImports(error);
    if (missing.length === 0) throw error;
    console.log(`  Missing packages for ${label} (${missing.join(", ")}); installing and retrying\u2026`);
    try {
      await installSpecificPackages(extPath, missing);
    } catch (installError) {
      console.error(
        `  Failed to install missing packages for ${label}: ${installError?.message || installError}`
      );
      throw error;
    }
    await esbuild.build(options);
  }
}
async function getExtensionBundle(extName, cmdName) {
  const normalizedExtName = normalizeExtensionName(extName);
  const extPath = resolveInstalledExtensionPath(normalizedExtName);
  if (!extPath) {
    const searchRoots = getConfiguredExtensionRoots();
    const msg = `Extension directory not found: ${normalizedExtName}. Searched roots: ${searchRoots.join(", ")}`;
    console.error(msg);
    throw new Error(msg);
  }
  let outFile = path.join(extPath, ".sc-build", `${cmdName}.js`);
  if (!fs.existsSync(outFile)) {
    console.log(
      `Pre-built bundle not found for ${normalizedExtName}/${cmdName}, building on-demand\u2026`
    );
    const built = await buildSingleCommand(normalizedExtName, cmdName);
    if (!built || !fs.existsSync(outFile)) {
      try {
        console.log(
          `Single-command build failed for ${normalizedExtName}/${cmdName}; trying full extension rebuild\u2026`
        );
        await buildAllCommands(normalizedExtName);
      } catch (rebuildError) {
        console.warn(`Full rebuild fallback failed for ${normalizedExtName}:`, rebuildError);
      }
    }
    if (!fs.existsSync(outFile)) {
      let diagnostic = "";
      try {
        const pkgPath = path.join(extPath, "package.json");
        const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf-8"));
        const commands = Array.isArray(pkg?.commands) ? pkg.commands : [];
        const cmd = commands.find((c) => c?.name === cmdName);
        const nodeModulesExists = fs.existsSync(path.join(extPath, "node_modules"));
        const requiresNodeModules = extensionRequiresNodeModules(pkg);
        if (!cmd) {
          diagnostic = ` Command "${cmdName}" not found in package.json.`;
        } else {
          const entry = resolveEntryFile(extPath, cmd);
          if (!entry) {
            diagnostic = ` Entry file not found for "${cmdName}".`;
          } else if (requiresNodeModules && !nodeModulesExists) {
            diagnostic = " node_modules is missing (dependency installation likely failed).";
          }
        }
      } catch {
      }
      const underlying = lastBuildError.get(`${normalizedExtName}/${cmdName}`);
      const underlyingSuffix = underlying ? ` Underlying error: ${underlying}` : "";
      const msg = `On-demand build failed for ${normalizedExtName}/${cmdName}. Extension path: ${extPath}. Expected output: ${outFile}.${diagnostic}${underlyingSuffix}`;
      console.error(msg);
      throw new Error(msg);
    }
  }
  const code = fs.readFileSync(outFile, "utf-8");
  if (!code) {
    const msg = `Pre-built bundle is empty: ${outFile}`;
    console.error(msg);
    throw new Error(msg);
  }
  let title = cmdName;
  let mode = "view";
  let owner = "";
  let extensionDisplayName = extName;
  let extensionIconDataUrl;
  let preferences = {};
  let commandPreferences = {};
  let preferenceDefinitions = [];
  let commandArgumentDefinitions = [];
  try {
    const pkgPath = path.join(extPath, "package.json");
    const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf-8"));
    if (!isManifestPlatformCompatible(pkg)) {
      return null;
    }
    const cmd = (pkg.commands || []).find((c) => c.name === cmdName);
    if (cmd && !isCommandPlatformCompatible(cmd)) {
      return null;
    }
    if (cmd?.title) title = cmd.title;
    if (cmd?.mode) mode = cmd.mode;
    if (pkg?.title) extensionDisplayName = pkg.title;
    extensionIconDataUrl = getExtensionIconDataUrl(extPath, pkg.icon || "icon.png");
    const rawOwner = pkg.owner || pkg.author || "";
    owner = typeof rawOwner === "object" ? rawOwner.name || "" : rawOwner;
    const { extensionPrefs, commandPrefs, definitions } = parsePreferences(pkg, cmdName);
    preferences = extensionPrefs;
    commandPreferences = commandPrefs;
    preferenceDefinitions = definitions;
    commandArgumentDefinitions = Array.isArray(cmd?.arguments) ? cmd.arguments.filter((arg) => arg && arg.name).map((arg) => ({
      name: arg.name,
      required: Boolean(arg.required),
      type: arg.type,
      placeholder: arg.placeholder,
      title: arg.title,
      data: Array.isArray(arg.data) ? arg.data : void 0
    })) : [];
  } catch {
  }
  const assetsPath = path.join(extPath, "assets");
  const supportPath = path.join(app.getPath("userData"), "extension-support", normalizedExtName);
  if (!fs.existsSync(supportPath)) {
    fs.mkdirSync(supportPath, { recursive: true });
  }
  return {
    code,
    title,
    mode,
    extensionName: normalizedExtName,
    extensionDisplayName,
    extensionIconDataUrl,
    commandName: cmdName,
    assetsPath,
    supportPath,
    extensionPath: extPath,
    owner,
    preferences: { ...preferences, ...commandPreferences },
    commandPreferences,
    preferenceDefinitions,
    commandArgumentDefinitions
  };
}
var fs, os, path, nodeBuiltins, lastBuildError;
var init_extension_builder = __esm({
  "src/main/extension-builder.ts"() {
    "use strict";
    init_desktop();
    fs = __toESM(require("fs"));
    os = __toESM(require("os"));
    path = __toESM(require("path"));
    init_extension_platform();
    init_esbuild_runtime();
    init_extension_registry();
    nodeBuiltins = [
      "assert",
      "buffer",
      "child_process",
      "cluster",
      "crypto",
      "dgram",
      "dns",
      "events",
      "fs",
      "fs/promises",
      "http",
      "http2",
      "https",
      "module",
      "net",
      "os",
      "path",
      "perf_hooks",
      "process",
      "querystring",
      "readline",
      "stream",
      "stream/promises",
      "string_decoder",
      "timers",
      "timers/promises",
      "tls",
      "tty",
      "url",
      "util",
      "v8",
      "vm",
      "worker_threads",
      "zlib",
      "async_hooks",
      "node:assert",
      "node:buffer",
      "node:child_process",
      "node:crypto",
      "node:events",
      "node:fs",
      "node:fs/promises",
      "node:http",
      "node:https",
      "node:module",
      "node:net",
      "node:os",
      "node:path",
      "node:process",
      "node:querystring",
      "node:stream",
      "node:timers",
      "node:timers/promises",
      "node:url",
      "node:util",
      "node:vm",
      "node:worker_threads",
      "node:zlib",
      "node:async_hooks"
    ];
    lastBuildError = /* @__PURE__ */ new Map();
  }
});

// src/main/extension-api.ts
function getApiBaseUrl() {
  return process.env.RAYMES_EXTENSION_API_URL || DEFAULT_API_URL;
}
function jsonRequest(method, urlPath, body) {
  return new Promise((resolve3, reject) => {
    const baseUrl = getApiBaseUrl();
    const fullUrl = new URL(urlPath, baseUrl);
    const isHttps = fullUrl.protocol === "https:";
    const transport = isHttps ? https : http;
    const payload = body ? JSON.stringify(body) : void 0;
    const options = {
      method,
      hostname: fullUrl.hostname,
      port: fullUrl.port || (isHttps ? 443 : 80),
      path: fullUrl.pathname + fullUrl.search,
      headers: {
        "User-Agent": "Tezbar",
        Accept: "application/json",
        ...payload ? {
          "Content-Type": "application/json",
          "Content-Length": Buffer.byteLength(payload)
        } : {}
      },
      timeout: REQUEST_TIMEOUT
    };
    const req = transport.request(options, (res) => {
      const chunks = [];
      res.on("data", (chunk) => chunks.push(chunk));
      res.on("end", () => {
        const statusCode = res.statusCode ?? 0;
        const rawBody = Buffer.concat(chunks).toString("utf-8");
        if (statusCode < 200 || statusCode >= 300) {
          reject(
            new Error(
              `API request failed: ${method} ${urlPath} \u2192 ${statusCode} ${res.statusMessage}
${rawBody}`
            )
          );
          return;
        }
        try {
          resolve3(JSON.parse(rawBody));
        } catch (parseError) {
          reject(new Error(`Failed to parse API response as JSON: ${rawBody}`));
        }
      });
    });
    req.on("error", (err) => reject(err));
    req.on("timeout", () => {
      req.destroy();
      reject(new Error(`API request timed out: ${method} ${urlPath}`));
    });
    if (payload) req.write(payload);
    req.end();
  });
}
async function fetchCatalogFromAPI() {
  const data = await jsonRequest("GET", "/extensions/catalog");
  return data.map((entry) => ({
    name: entry.name ?? "",
    title: entry.title ?? "",
    description: entry.description ?? "",
    author: entry.author ?? "",
    contributors: entry.contributors ?? [],
    icon: entry.icon ?? "",
    iconUrl: entry.iconUrl ?? entry.icon_url ?? "",
    screenshotUrls: entry.screenshotUrls ?? entry.screenshot_urls ?? [],
    categories: entry.categories ?? [],
    platforms: entry.platforms ?? [],
    commands: (entry.commands ?? []).map((cmd) => ({
      name: cmd.name ?? "",
      title: cmd.title ?? "",
      description: cmd.description ?? ""
    })),
    installCount: entry.installCount ?? entry.install_count ?? 0
  }));
}
async function getExtensionBundleUrl(name) {
  return jsonRequest(
    "GET",
    `/extensions/${encodeURIComponent(name)}/bundle`
  );
}
async function getExtensionScreenshotsFromAPI(name) {
  try {
    return await jsonRequest(
      "GET",
      `/extensions/${encodeURIComponent(name)}/screenshots`
    );
  } catch {
    return [];
  }
}
async function reportInstall(name, machineId) {
  try {
    await jsonRequest(
      "POST",
      `/extensions/${encodeURIComponent(name)}/install`,
      machineId ? { machineId } : {}
    );
  } catch (err) {
    console.warn("Failed to report install:", err);
  }
}
async function reportUninstall(name, machineId) {
  try {
    await jsonRequest(
      "POST",
      `/extensions/${encodeURIComponent(name)}/uninstall`,
      machineId ? { machineId } : {}
    );
  } catch (err) {
    console.warn("Failed to report uninstall:", err);
  }
}
var https, http, DEFAULT_API_URL, REQUEST_TIMEOUT;
var init_extension_api = __esm({
  "src/main/extension-api.ts"() {
    "use strict";
    https = __toESM(require("https"));
    http = __toESM(require("http"));
    DEFAULT_API_URL = "https://api.supercmd.sh";
    REQUEST_TIMEOUT = 3e4;
  }
});

// src/main/bun-manager.ts
function createBunInstallEnvironment(bunPath, sourceEnv = process.env) {
  const env = { ...sourceEnv };
  for (const key of Object.keys(env)) {
    if (key.toUpperCase() === "ESBUILD_BINARY_PATH") delete env[key];
  }
  const pathKey = Object.keys(env).find((key) => key.toUpperCase() === "PATH") ?? "PATH";
  env[pathKey] = [path2.dirname(bunPath), env[pathKey]].filter(Boolean).join(path2.delimiter);
  return env;
}
function broadcastInstallStatus(message) {
  for (const window2 of BrowserWindow.getAllWindows()) {
    if (window2.isDestroyed()) continue;
    try {
      window2.webContents.send("extension-install-status", message);
    } catch {
    }
  }
}
function getBunDownloadUrl() {
  const arch = process.arch === "arm64" ? "aarch64" : "x64";
  if (process.platform === "darwin") {
    return `https://github.com/oven-sh/bun/releases/download/bun-v${BUN_VERSION}/bun-darwin-${arch}.zip`;
  }
  if (process.platform === "linux") {
    return `https://github.com/oven-sh/bun/releases/download/bun-v${BUN_VERSION}/bun-linux-${arch}.zip`;
  }
  if (process.platform === "win32") {
    return `https://github.com/oven-sh/bun/releases/download/bun-v${BUN_VERSION}/bun-windows-${arch}.zip`;
  }
  return "";
}
function getBunDir() {
  return path2.join(app.getPath("userData"), "bun");
}
function getBunBinaryPath() {
  return path2.join(getBunDir(), process.platform === "win32" ? "bun.exe" : "bun");
}
function isBunAvailable() {
  const binPath = getBunBinaryPath();
  try {
    return fs2.existsSync(binPath) && fs2.statSync(binPath).size > 1e6;
  } catch {
    return false;
  }
}
async function ensureBun() {
  if (isBunAvailable()) {
    return getBunBinaryPath();
  }
  const url = getBunDownloadUrl();
  if (!url) {
    console.warn("Bun download not supported on this platform");
    return null;
  }
  const bunDir = getBunDir();
  fs2.mkdirSync(bunDir, { recursive: true });
  console.log(`Downloading Bun v${BUN_VERSION}...`);
  broadcastInstallStatus("Setting up installer for first use\u2026");
  try {
    const zipBuffer = await downloadFile(url);
    broadcastInstallStatus("Setting up installer\u2026");
    console.log(`Downloaded Bun (${(zipBuffer.length / 1024 / 1024).toFixed(1)}MB), extracting...`);
    const tmpZipPath = path2.join(app.getPath("temp"), `bun-${Date.now()}.zip`);
    fs2.writeFileSync(tmpZipPath, zipBuffer);
    const tmpExtractDir = path2.join(app.getPath("temp"), `bun-extract-${Date.now()}`);
    fs2.mkdirSync(tmpExtractDir, { recursive: true });
    if (process.platform === "win32") {
      await execFileAsync2(
        "powershell.exe",
        [
          "-NoLogo",
          "-NoProfile",
          "-NonInteractive",
          "-Command",
          "Expand-Archive -LiteralPath $env:TEZBAR_BUN_ZIP -DestinationPath $env:TEZBAR_BUN_EXTRACT -Force"
        ],
        {
          timeout: 3e4,
          env: {
            ...process.env,
            TEZBAR_BUN_ZIP: tmpZipPath,
            TEZBAR_BUN_EXTRACT: tmpExtractDir
          }
        }
      );
    } else {
      await execFileAsync2("unzip", ["-o", tmpZipPath, "-d", tmpExtractDir], { timeout: 3e4 });
    }
    const bunBinary = findFile(tmpExtractDir, process.platform === "win32" ? "bun.exe" : "bun");
    if (!bunBinary) {
      throw new Error("Bun binary not found in downloaded archive");
    }
    const destPath = getBunBinaryPath();
    fs2.copyFileSync(bunBinary, destPath);
    if (process.platform !== "win32") fs2.chmodSync(destPath, 493);
    try {
      fs2.rmSync(tmpZipPath, { force: true });
    } catch {
    }
    try {
      fs2.rmSync(tmpExtractDir, { recursive: true, force: true });
    } catch {
    }
    const { stdout } = await execFileAsync2(destPath, ["--version"], { timeout: 5e3 });
    console.log(`Bun installed successfully: ${stdout.trim()}`);
    return destPath;
  } catch (error) {
    console.error("Failed to download/install Bun:", error?.message || error);
    try {
      fs2.rmSync(getBunBinaryPath(), { force: true });
    } catch {
    }
    return null;
  }
}
async function installDepsWithBun(extPath) {
  const bunPath = await ensureBun();
  if (!bunPath) return false;
  const pkgPath = path2.join(extPath, "package.json");
  if (!fs2.existsSync(pkgPath)) return true;
  let pkg;
  try {
    pkg = JSON.parse(fs2.readFileSync(pkgPath, "utf-8"));
  } catch {
    return true;
  }
  const deps = {
    ...pkg.dependencies || {},
    ...pkg.optionalDependencies || {}
  };
  const thirdPartyDeps = Object.entries(deps).filter(([name]) => !name.startsWith("@raycast/")).map(([name, version]) => `${name}@${version}`).filter(Boolean);
  if (thirdPartyDeps.length === 0) {
    console.log(`No third-party deps for ${path2.basename(extPath)} \u2014 skipping bun install`);
    return true;
  }
  console.log(`Installing ${thirdPartyDeps.length} deps via Bun for ${path2.basename(extPath)}...`);
  try {
    const cleanPkg = {
      name: pkg.name || "extension",
      version: pkg.version || "1.0.0",
      private: true,
      dependencies: Object.fromEntries(
        Object.entries(deps).filter(([name]) => !name.startsWith("@raycast/"))
      )
    };
    const originalPkg = fs2.readFileSync(pkgPath, "utf-8");
    fs2.writeFileSync(pkgPath, JSON.stringify(cleanPkg, null, 2));
    for (const lockfile of [
      "package-lock.json",
      "bun.lockb",
      "bun.lock",
      "yarn.lock",
      "pnpm-lock.yaml"
    ]) {
      try {
        fs2.rmSync(path2.join(extPath, lockfile), { force: true });
      } catch {
      }
    }
    await execFileAsync2(bunPath, ["install", "--production", "--no-save"], {
      cwd: extPath,
      timeout: 12e4,
      env: createBunInstallEnvironment(bunPath)
    });
    fs2.writeFileSync(pkgPath, originalPkg);
    const hasNodeModules2 = fs2.existsSync(path2.join(extPath, "node_modules"));
    if (hasNodeModules2) {
      console.log(`Bun install succeeded for ${path2.basename(extPath)}`);
      return true;
    }
    console.warn(`Bun completed but node_modules missing for ${path2.basename(extPath)}`);
    fs2.writeFileSync(pkgPath, originalPkg);
    return false;
  } catch (error) {
    console.warn(`Bun install failed for ${path2.basename(extPath)}:`, error?.message);
    try {
      const originalContent = JSON.stringify(pkg, null, 2);
      fs2.writeFileSync(pkgPath, originalContent);
    } catch {
    }
    return false;
  }
}
async function installSpecificPackagesWithBun(extPath, packageNames) {
  const bunPath = await ensureBun();
  if (!bunPath) return false;
  const unique = Array.from(
    new Set(packageNames.map((name) => String(name || "").trim()).filter(Boolean))
  );
  if (unique.length === 0) return true;
  const validPackageName = /^(?:@[A-Za-z0-9][A-Za-z0-9._-]*\/)?[A-Za-z0-9][A-Za-z0-9._-]*$/;
  const invalid = unique.find((name) => !validPackageName.test(name));
  if (invalid) {
    console.warn(`Refusing invalid package name from extension build: ${invalid}`);
    return false;
  }
  console.log(
    `Installing specific packages via Bun for ${path2.basename(extPath)}: ${unique.join(", ")}`
  );
  try {
    await execFileAsync2(bunPath, ["add", "--no-save", ...unique], {
      cwd: extPath,
      timeout: 3e5,
      env: createBunInstallEnvironment(bunPath)
    });
    return fs2.existsSync(path2.join(extPath, "node_modules"));
  } catch (error) {
    console.warn(`Bun add failed for ${path2.basename(extPath)}:`, error?.message);
    return false;
  }
}
function downloadFile(url) {
  return new Promise((resolve3, reject) => {
    const makeRequest = (requestUrl, redirects = 0) => {
      if (redirects > 10) {
        reject(new Error("Too many redirects"));
        return;
      }
      const parsedUrl = new URL(requestUrl);
      const transport = parsedUrl.protocol === "https:" ? https2 : http2;
      transport.get(requestUrl, { timeout: 12e4 }, (res) => {
        if (res.statusCode && res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          makeRequest(res.headers.location, redirects + 1);
          return;
        }
        if (res.statusCode !== 200) {
          reject(new Error(`Download failed: ${res.statusCode} ${res.statusMessage}`));
          return;
        }
        const chunks = [];
        res.on("data", (chunk) => chunks.push(chunk));
        res.on("error", reject);
        res.on("end", () => resolve3(Buffer.concat(chunks)));
      }).on("error", reject);
    };
    makeRequest(url);
  });
}
function findFile(dir, name) {
  try {
    const entries = fs2.readdirSync(dir, { withFileTypes: true });
    for (const entry of entries) {
      const full = path2.join(dir, entry.name);
      if (entry.isFile() && entry.name === name) return full;
      if (entry.isDirectory()) {
        const found = findFile(full, name);
        if (found) return found;
      }
    }
  } catch {
  }
  return null;
}
var import_child_process, import_util, fs2, path2, https2, http2, execFileAsync2, BUN_VERSION;
var init_bun_manager = __esm({
  "src/main/bun-manager.ts"() {
    "use strict";
    init_desktop();
    import_child_process = require("child_process");
    import_util = require("util");
    fs2 = __toESM(require("fs"));
    path2 = __toESM(require("path"));
    https2 = __toESM(require("https"));
    http2 = __toESM(require("http"));
    execFileAsync2 = (0, import_util.promisify)(import_child_process.execFile);
    BUN_VERSION = "1.2.5";
  }
});

// src/shared/lovedExtensions.ts
function searchableText(extension) {
  return [
    extension.name,
    extension.id,
    extension.description,
    extension.author,
    extension.owner,
    extension.repository,
    ...extension.categories ?? [],
    ...(extension.commands ?? []).flatMap((command) => [
      command.name,
      command.title,
      command.description
    ])
  ].filter(Boolean).join(" ").toLowerCase();
}
function searchLovedExtensions(query) {
  const normalizedQuery = String(query || "").trim().toLowerCase();
  if (!normalizedQuery) return [...LOVED_EXTENSIONS];
  return LOVED_EXTENSIONS.filter((extension) => searchableText(extension).includes(normalizedQuery));
}
var LOVED_EXTENSIONS;
var init_lovedExtensions = __esm({
  "src/shared/lovedExtensions.ts"() {
    "use strict";
    LOVED_EXTENSIONS = [
      {
        id: "raycast.tezbar-color-picker",
        name: "Color Picker",
        description: "Pick an exact pixel from any screen and inspect it in RGB, HEX, HSL, HSV, and CSS formats.",
        author: "Tezbar",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/tezbar-color-picker-extension",
        iconUrl: "https://raw.githubusercontent.com/almatkai/tezbar-color-picker-extension/master/assets/icon.svg",
        screenshotUrls: [
          "https://raw.githubusercontent.com/almatkai/tezbar-color-picker-extension/master/assets/picker-preview.png",
          "https://raw.githubusercontent.com/almatkai/tezbar-color-picker-extension/master/assets/color-wheel-preview.png"
        ],
        categories: ["Design", "Developer Tools"],
        commands: [
          {
            name: "pick-color",
            title: "Pick Color",
            description: "Open the native screen sampler and inspect the selected color."
          }
        ]
      },
      {
        id: "raycast.asciimath-to-latex-converter",
        name: "AsciiMath to LaTeX Converter",
        description: "Convert AsciiMath expressions to LaTeX with live preview.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/asciimath-to-latex-converter",
        iconUrl: "https://raw.githubusercontent.com/almatkai/asciimath-to-latex-converter/main/assets/icon.png",
        categories: ["Productivity"],
        commands: [
          {
            name: "convert-asciimath-to-latex",
            title: "Convert AsciiMath to LaTeX",
            description: "Convert AsciiMath input to LaTeX and preview the result."
          }
        ]
      },
      {
        id: "raycast.base64",
        name: "Base64",
        description: "Encode and decode Base64 text from Tezbar.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/base64",
        iconUrl: "https://raw.githubusercontent.com/almatkai/base64/main/assets/icon.svg",
        categories: ["Developer Tools", "Data"],
        commands: [
          {
            name: "index",
            title: "Base64",
            description: "Encode or decode Base64 text interactively."
          },
          {
            name: "encode",
            title: "Base64 Encode Clipboard",
            description: "Replace clipboard text with its Base64 representation."
          },
          {
            name: "decode",
            title: "Base64 Decode Clipboard",
            description: "Decode Base64 clipboard text and copy the result."
          }
        ]
      },
      {
        id: "raycast.base64-to-file",
        name: "Base64 to File",
        description: "convert base64 to file",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/base64-to-file",
        iconUrl: "https://raw.githubusercontent.com/almatkai/base64-to-file/main/assets/command-icon.png",
        categories: ["Data", "Applications"],
        commands: [
          {
            name: "index",
            title: "Convert Base64 to File",
            description: "Convert base64 text to file"
          }
        ]
      },
      {
        id: "raycast.bitly-url-shortener",
        name: "Bitly URL Shortener",
        description: "Quickly shorten the selected URL or current clipboard URL with Bitly",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/bitly-url-shortener",
        iconUrl: "https://raw.githubusercontent.com/almatkai/bitly-url-shortener/main/assets/command-icon.png",
        categories: ["Communication"],
        commands: [
          {
            name: "index",
            title: "Shorten URL",
            description: "Overwrite the clipboard contents with a shortened URL"
          },
          {
            name: "list-links",
            title: "List Links",
            description: "List all the links you have shortened with Bitly"
          }
        ]
      },
      {
        id: "raycast.blurhash",
        name: "BlurHash",
        description: "Generate blurhash from clipboard content or Finder",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/blurhash",
        iconUrl: "https://raw.githubusercontent.com/almatkai/blurhash/main/assets/extension-icon.png",
        categories: ["Developer Tools"],
        commands: [
          {
            name: "clipboard-blurhash",
            title: "BlurHash",
            description: "Generate blurhash from clipboard content or Finder"
          }
        ]
      },
      {
        id: "raycast.change-case",
        name: "Change Case",
        description: "Transform a string between camelCase, snake_case, CONSTANT_CASE, and more",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/change-case",
        iconUrl: "https://raw.githubusercontent.com/almatkai/change-case/main/assets/command-icon.png",
        categories: ["Developer Tools", "Productivity"],
        commands: [
          {
            name: "change-case",
            title: "Change Case",
            description: "Change the case of the selected text or text in the clipboard"
          },
          {
            name: "convert-camel-case",
            title: "Convert to camelCase",
            description: "Convert selected text or clipboard to camelCase"
          },
          {
            name: "convert-capital-case",
            title: "Convert to Capital Case",
            description: "Convert selected text or clipboard to Capital Case"
          },
          {
            name: "convert-constant-case",
            title: "Convert to CONSTANT_CASE",
            description: "Convert selected text or clipboard to CONSTANT_CASE"
          },
          {
            name: "convert-dot-case",
            title: "Convert to dot.case",
            description: "Convert selected text or clipboard to dot.case"
          },
          {
            name: "convert-header-case",
            title: "Convert to Header-Case",
            description: "Convert selected text or clipboard to Header-Case"
          },
          {
            name: "convert-lower-case",
            title: "Convert to lower case",
            description: "Convert selected text or clipboard to lower case"
          },
          {
            name: "convert-lower-first",
            title: "Convert to lower First",
            description: "Convert selected text or clipboard to lower First"
          },
          {
            name: "convert-no-case",
            title: "Convert to no case",
            description: "Convert selected text or clipboard to no case"
          },
          {
            name: "convert-kebab-case",
            title: "Convert to kebab-case",
            description: "Convert selected text or clipboard to kebab-case"
          },
          {
            name: "convert-kebab-upper-case",
            title: "Convert to KEBAB-UPPER-CASE",
            description: "Convert selected text or clipboard to KEBAB-UPPER-CASE"
          },
          {
            name: "convert-pascal-case",
            title: "Convert to PascalCase",
            description: "Convert selected text or clipboard to PascalCase"
          },
          {
            name: "convert-pascal-snake-case",
            title: "Convert to Pascal_Snake_Case",
            description: "Convert selected text or clipboard to Pascal_Snake_Case"
          },
          {
            name: "convert-path-case",
            title: "Convert to path/case",
            description: "Convert selected text or clipboard to path/case"
          },
          {
            name: "convert-random-case",
            title: "Convert to rAndOm cAsE",
            description: "Convert selected text or clipboard to rAndOm cAsE"
          },
          {
            name: "convert-sentence-case",
            title: "Convert to Sentence case",
            description: "Convert selected text or clipboard to Sentence case"
          },
          {
            name: "convert-snake-case",
            title: "Convert to snake_case",
            description: "Convert selected text or clipboard to snake_case"
          },
          {
            name: "convert-alternating-case",
            title: "Convert to aLtErNaTiNg cAsE",
            description: "Convert selected text or clipboard to aLtErNaTiNg cAsE"
          },
          {
            name: "convert-swap-case",
            title: "Convert to sWAP cASE",
            description: "Convert selected text or clipboard to sWAP cASE"
          },
          {
            name: "convert-title-case",
            title: "Convert to Title Case",
            description: "Convert selected text or clipboard to Title Case"
          },
          {
            name: "convert-upper-case",
            title: "Convert to UPPER CASE",
            description: "Convert selected text or clipboard to UPPER CASE"
          },
          {
            name: "convert-upper-first",
            title: "Convert to Upper first",
            description: "Convert selected text or clipboard to Upper first"
          }
        ]
      },
      {
        id: "raycast.clipboard-editor",
        name: "Clipboard Editor",
        description: "Edit your clipboard text",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/clipboard-editor",
        iconUrl: "https://raw.githubusercontent.com/almatkai/clipboard-editor/main/assets/icon.png",
        categories: ["Productivity"],
        commands: [
          {
            name: "edit-clipboard-text",
            title: "Edit Clipboard Content",
            description: "Edit your clipboard text"
          }
        ]
      },
      {
        id: "raycast.clipboard-formatter",
        name: "Clipboard Formatter",
        description: "Removes formatting and html from the text stored in the clipboard, returning the unformatted text to the clipboard.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/clipboard-formatter",
        iconUrl: "https://raw.githubusercontent.com/almatkai/clipboard-formatter/main/assets/extension_icon.png",
        categories: ["Productivity", "System"],
        commands: [
          {
            name: "index",
            title: "Strip Clipboard Formatting",
            description: "Removes formatting and html from the text stored in the clipboard, returning the unformatted text to the clipboard."
          }
        ]
      },
      {
        id: "raycast.clipboard-sequential-paste",
        name: "Clipboard Sequential Paste",
        description: "Copy a list to your clipboard and paste items one by one.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/clipboard-sequential-paste",
        iconUrl: "https://raw.githubusercontent.com/almatkai/clipboard-sequential-paste/main/assets/extension-icon.png",
        categories: ["Productivity"],
        commands: [
          {
            name: "paste-next-item",
            title: "Paste Next Item",
            description: "Pastes the next item from your copied list."
          }
        ]
      },
      {
        id: "raycast.code-review-emojis",
        name: "Code Review Emoji Guide",
        description: "A simple emoji legend to help convey intention and added meaning in code review comments.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/code-review-emojis",
        iconUrl: "https://raw.githubusercontent.com/almatkai/code-review-emojis/main/assets/command-icon.png",
        categories: ["Documentation", "Developer Tools"],
        commands: [
          {
            name: "index",
            title: "Show Review Emoji",
            description: "Shows a list of emojis that can be used for code review, in line with the Code Review Emoji Guide."
          }
        ]
      },
      {
        id: "raycast.color-shades",
        name: "Color Shades",
        description: "Generate color shades from a base color and manage your generated color shades palettes.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/color-shades",
        iconUrl: "https://raw.githubusercontent.com/almatkai/color-shades/main/assets/command-icon.png",
        categories: ["Design Tools", "Developer Tools"],
        commands: [
          {
            name: "generate-color-shades",
            title: "Generate Color Shades",
            description: "Generate color shades palette for the given base color."
          },
          {
            name: "view-color-shades",
            title: "View Color Shades",
            description: "View your generated color shades palettes."
          }
        ]
      },
      {
        id: "raycast.convert",
        name: "Web Converter",
        description: "Converts everything web related: rem, px, pt, hex, hex (with opacity), rgb, rgba, hsl, hsla, oklch and shows the closest Tailwind CSS color.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/convert",
        iconUrl: "https://raw.githubusercontent.com/almatkai/convert/main/assets/icon.png",
        categories: ["Developer Tools", "Productivity", "Web"],
        commands: [
          {
            name: "convert",
            title: "Convert Unit",
            description: "Converts everything web related: rem, px, pt, hex, hex (with opacity), rgb, rgba, hsl, hsla, oklch and shows the closest Tailwind CSS color."
          }
        ]
      },
      {
        id: "raycast.convert-3d-models",
        name: "Convert 3D Models",
        description: "An extension to convert 3D models to different formats.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/convert-3d-models",
        iconUrl: "https://raw.githubusercontent.com/almatkai/convert-3d-models/main/assets/extension_icon.png",
        categories: ["Design Tools", "Productivity", "Other"],
        commands: [
          {
            name: "convert",
            title: "Convert 3D Model Files",
            description: "Convert the selected model to the specified format."
          },
          {
            name: "quickConvertSTL",
            title: "Quick Convert to STL",
            description: "Quickly convert selected models to STL"
          },
          {
            name: "quickConvertOBJ",
            title: "Quick Convert to OBJ",
            description: "Quickly convert selected models to OBJ"
          },
          {
            name: "quickConvertSTP",
            title: "Quick Convert to STP",
            description: "Quickly convert selected models to STP"
          }
        ]
      },
      {
        id: "raycast.converter",
        name: "Converter",
        description: "Arbitrary input, direct output. Includes Base converter, Byte converter, Code converter, etc.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/converter",
        iconUrl: "https://raw.githubusercontent.com/almatkai/converter/main/assets/extension-icon.png",
        categories: ["Design Tools", "Developer Tools", "Productivity"],
        commands: [
          {
            name: "base-converter",
            title: "Base Converter",
            description: "Convert number from any base to any base: decimal, binary, octal, hexadecimal, etc."
          },
          {
            name: "code-converter",
            title: "Code Converter",
            description: "Convert between different character encodings: Unicode, Base64, UTF-8, ASCII, &#xXXXX, URL."
          },
          {
            name: "byte-converter",
            title: "Byte Converter",
            description: "Convert among different byte unit: b, B, KB, MB, GB, etc."
          }
        ]
      },
      {
        id: "raycast.convert-px-to-vw-vh",
        name: "Pixels to Viewport Width or Height",
        description: "Fast conversion of pixels to vw or vh depending on the size of your viewport.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/convert-px-to-vw-vh",
        iconUrl: "https://raw.githubusercontent.com/almatkai/convert-px-to-vw-vh/main/assets/logo.png",
        categories: ["Developer Tools", "Design Tools", "Web", "Productivity"],
        commands: [
          {
            name: "index",
            title: "Convert PX to VW or VH",
            description: "All commands in one place for easy convertion"
          },
          { name: "height", title: "Pixels Viewport Height", description: "Pixels to viewport height" },
          { name: "width", title: "Pixels Viewport Width", description: "Pixels to viewport width" },
          {
            name: "menu-bar",
            title: "Conversion History",
            description: "History of your last conversions"
          }
        ]
      },
      {
        id: "raycast.convert-typescript-to-javascript",
        name: "Convert TypeScript to JavaScript",
        description: "Remove type information from TypeScript code to make it compatible with JavaScript REPLs and other applications",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/convert-typescript-to-javascript",
        iconUrl: "https://raw.githubusercontent.com/almatkai/convert-typescript-to-javascript/main/assets/icon.png",
        categories: ["Developer Tools"],
        commands: [
          {
            name: "paste-from-clipboard",
            title: "Convert & Paste as JavaScript",
            description: "Retrieve clipboard content, transform it to JavaScript, and paste it"
          },
          {
            name: "copy-to-clipboard",
            title: "Copy TypeScript as JavaScript",
            description: "Get selected text from the active application, transform it to JavaScript, and keep it in the clipboard"
          }
        ]
      },
      {
        id: "raycast.counter",
        name: "Counter",
        description: "A simple counter, in the comfort of Raycast root!",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/counter",
        iconUrl: "https://raw.githubusercontent.com/almatkai/counter/main/command-icon.png",
        categories: ["Productivity", "Fun"],
        commands: [
          {
            name: "increment-counter",
            title: "Increment Counter",
            description: "Increments the count of the counter."
          },
          {
            name: "decrement-counter",
            title: "Decrement Counter",
            description: "Decrements the count of the counter."
          },
          { name: "reset-counter", title: "Reset Counter", description: "Resets the counter." }
        ]
      },
      {
        id: "raycast.count-numbers",
        name: "Count Numbers",
        description: "A simple but powerful number counter!",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/count-numbers",
        iconUrl: "https://raw.githubusercontent.com/almatkai/count-numbers/main/assets/extension_icon.png",
        categories: ["Fun", "Productivity"],
        commands: [{ name: "count-numbers", title: "Count Numbers", description: "Count numbers." }]
      },
      {
        id: "raycast.country-lookup",
        name: "Country Lookup",
        description: "Search and explore detailed data for every country: flags, capitals, languages, currencies, regions and more, powered by the REST Countries API.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/country-lookup",
        iconUrl: "https://raw.githubusercontent.com/almatkai/country-lookup/main/assets/extension_iconv1.png",
        categories: ["Data", "Web"],
        commands: [
          {
            name: "index",
            title: "Search Countries",
            description: "Search countries by name, capital, or language and browse rich details like flags, currencies, and regions."
          }
        ]
      },
      {
        id: "raycast.crypto-portfolio-tracker",
        name: "Crypto Portfolio Tracker",
        description: "Add crypto wallets by their addresses and track your entire portfolio.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/crypto-portfolio-tracker",
        iconUrl: "https://raw.githubusercontent.com/almatkai/crypto-portfolio-tracker/main/assets/extension-icon.png",
        categories: ["Data", "Finance"],
        commands: [
          {
            name: "add-ethereum-address",
            title: "Add Ethereum Address",
            description: "Import an Ethereum address to add to your portfolio."
          },
          {
            name: "portfolio",
            title: "View Portfolio",
            description: "View the total value and breakdown of your crypto portfolio."
          }
        ]
      },
      {
        id: "raycast.crypto-price",
        name: "Crypto Price",
        description: "Keep an eye on Bitcoin (BTC), Ethereum (ETH), and other cryptocurrency prices and more directly from your system menu bar",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/crypto-price",
        iconUrl: "https://raw.githubusercontent.com/almatkai/crypto-price/main/assets/command-icon.png",
        categories: ["Finance"],
        commands: [
          { name: "menu-bar", title: "Crypto Price", description: "See cryptocurency prices and more" }
        ]
      },
      {
        id: "raycast.css-calculations",
        name: "CSS Calculations",
        description: "Useful CSS Calculations in Web Development",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/css-calculations",
        iconUrl: "https://raw.githubusercontent.com/almatkai/css-calculations/main/assets/css.png",
        categories: ["Developer Tools", "Productivity"],
        commands: [
          {
            name: "clamp_px",
            title: "Calculate Clamp PX",
            description: "Generate responsive clamp function based on px values"
          },
          {
            name: "clamp_rem",
            title: "Calculate Clamp REM",
            description: "Generate responsive clamp function based on rem values"
          }
        ]
      },
      {
        id: "raycast.csv-to-excel",
        name: "Convert CSV to Excel",
        description: "Convert CSV table to Excel compatible table.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/csv-to-excel",
        iconUrl: "https://raw.githubusercontent.com/almatkai/csv-to-excel/main/assets/command-icon.png",
        categories: ["Applications", "Productivity"],
        commands: [
          {
            name: "index",
            title: "Convert CSV to Excel",
            description: "Convert CSV table to Excel compatible table"
          }
        ]
      },
      {
        id: "raycast.curl",
        name: "cURL",
        description: "Keyboard-first HTTP client",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/curl",
        iconUrl: "https://raw.githubusercontent.com/almatkai/curl/main/assets/app-icon.png",
        categories: ["Productivity", "Developer Tools"],
        commands: [
          {
            name: "index",
            title: "Make HTTP Request",
            description: "Shortcut to make an HTTP request"
          },
          { name: "requests", title: "Requests History", description: "HTTP requests history" }
        ]
      },
      {
        id: "raycast.currency-exchange",
        name: "Currency Exchange",
        description: "Simple Currency Exchange with a selectable rate provider (ExchangeRate-API or UniRateAPI)",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/currency-exchange",
        iconUrl: "https://raw.githubusercontent.com/almatkai/currency-exchange/main/assets/currency.png",
        categories: ["Developer Tools"],
        commands: [
          { name: "index", title: "Currency Exchange", description: "Calculate with Exchange Rate" }
        ]
      },
      {
        id: "raycast.custom-wordle",
        name: "Custom Wordle",
        description: "Get a link to play wordle with your own word",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/custom-wordle",
        iconUrl: "https://raw.githubusercontent.com/almatkai/custom-wordle/main/assets/custom-wordle-icon.png",
        categories: ["Fun"],
        commands: [
          {
            name: "create-custom-wordle",
            title: "Create Custom Wordle",
            description: "Create a wordle with your own word and share it with your friends to have them guess your word."
          }
        ]
      },
      {
        id: "raycast.date-converter",
        name: "Date Converter",
        description: "Convert a variety of date formats",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/date-converter",
        iconUrl: "https://raw.githubusercontent.com/almatkai/date-converter/main/extension-icon.png",
        categories: ["Developer Tools", "Productivity"],
        commands: [{ name: "index", title: "Convert Date", description: "Parse and convert a date" }]
      },
      {
        id: "raycast.datetime-format-converter",
        name: "Date Format Converter",
        description: "Convert timestamps and datetime strings into various formats.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/datetime-format-converter",
        iconUrl: "https://raw.githubusercontent.com/almatkai/datetime-format-converter/main/assets/command-icon.png",
        categories: ["Developer Tools"],
        commands: [
          {
            name: "index",
            title: "Format Date",
            description: "Convert timestamps and datetime strings into various formats."
          },
          {
            name: "insert-natural-language-date",
            title: "Insert Natural Language Date",
            description: "Convert a natural language prompt into a formatted date and insert it into the frontmost application"
          }
        ]
      },
      {
        id: "raycast.dictionary",
        name: "Web Dictionaries",
        description: "Search any word with multiple online dictionaries or translation engines in one place\u0432\u0402\u201Dlightweight, zero dependencies.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/dictionary",
        iconUrl: "https://raw.githubusercontent.com/almatkai/dictionary/main/assets/icon.png",
        categories: ["Productivity", "Data", "Web"],
        commands: [
          {
            name: "fromCmd",
            title: "Search in Dictionaries",
            description: "Search any word with multiple online dictionaries."
          },
          {
            name: "fromSelected",
            title: "Search From Cursor Selection",
            description: "Search your current cursor-selected words."
          }
        ]
      },
      {
        id: "raycast.diff-checker",
        name: "Diff Checker",
        description: "Compare content of two texts",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/diff-checker",
        iconUrl: "https://raw.githubusercontent.com/almatkai/diff-checker/main/assets/icon.png",
        categories: ["Data", "Developer Tools"],
        commands: [
          {
            name: "check-diff",
            title: "Check Diff",
            description: "Compare two text snippets line by line"
          },
          {
            name: "compare-json",
            title: "Compare JSON",
            description: "Paste two JSON snippets to see the differences"
          }
        ]
      },
      {
        id: "raycast.emoji",
        name: "Emoji Search",
        description: "Finds emojis and inserts or copies them.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/emoji",
        iconUrl: "https://raw.githubusercontent.com/almatkai/emoji/main/assets/command-icon.png",
        categories: ["System", "Media"],
        commands: [
          {
            name: "emoji",
            title: "Search Emoji",
            description: "Finds emojis and inserts or copies them."
          }
        ]
      },
      {
        id: "raycast.emoji-kitchen",
        name: "Emoji Mashups",
        description: "Discover and copy Google Emoji Kitchen mashups fast. Browse unique emoji combinations and use them as fun, expressive images in messages and posts. Copy/paste or save.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/emoji-kitchen",
        iconUrl: "https://raw.githubusercontent.com/almatkai/emoji-kitchen/main/assets/icon.png",
        categories: ["Fun", "Media"],
        commands: [
          {
            name: "index",
            title: "Search Emoji Kitchen",
            description: "Search for emojis and see their mashups."
          }
        ]
      },
      {
        id: "raycast.epoch-to-timestamp",
        name: "Epoch to Timestamp",
        description: "Quickly convert a Unix epoch, in seconds or milliseconds, to a human-readible timestamp.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/epoch-to-timestamp",
        iconUrl: "https://raw.githubusercontent.com/almatkai/epoch-to-timestamp/main/assets/extension-icon.png",
        categories: ["Developer Tools"],
        commands: [
          {
            name: "epoch",
            title: "Epoch",
            description: "Convert a Unix epoch in your clipboard, in seconds or milliseconds, to a human-readible timestamp."
          }
        ]
      },
      {
        id: "raycast.file-info",
        name: "File Info",
        description: "Quickly view or strip metadata (EXIF, GPS, IPTC, etc.) from selected files for privacy and cleaner sharing. Works on macOS and Windows.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/file-info",
        iconUrl: "https://raw.githubusercontent.com/almatkai/file-info/main/assets/extension-icon.png",
        categories: ["Productivity", "Developer Tools", "System"],
        commands: [
          {
            name: "show-file-info",
            title: "Show File Info",
            description: "Display metadata of the currently selected file"
          },
          {
            name: "clear-file-metadata",
            title: "Clear File Metadata",
            description: "Strip all embedded metadata (EXIF, GPS, IPTC, XMP, etc.) and system attributes from selected files."
          }
        ]
      },
      {
        id: "raycast.file-tree-generator",
        name: "File Tree Generator",
        description: "Generates file tree from text",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/file-tree-generator",
        iconUrl: "https://raw.githubusercontent.com/almatkai/file-tree-generator/main/assets/command-icon.png",
        categories: ["Developer Tools", "Productivity"],
        commands: [
          { name: "index", title: "Create File Tree", description: "Generates file tree from text" }
        ]
      },
      {
        id: "raycast.find-website",
        name: "Find Website",
        description: "Quickly find the website you are looking for.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/find-website",
        iconUrl: "https://raw.githubusercontent.com/almatkai/find-website/main/assets/logo.png",
        categories: ["Productivity"],
        commands: [
          {
            name: "find",
            title: "Find Website",
            description: "Quickly find the website you are looking for."
          }
        ]
      },
      {
        id: "raycast.fontawesome",
        name: "Font Awesome",
        description: "Search Font Awesome icons",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/fontawesome",
        iconUrl: "https://raw.githubusercontent.com/almatkai/fontawesome/main/assets/command-icon.png",
        categories: ["Design Tools", "Developer Tools"],
        commands: [
          { name: "index", title: "Search Font Awesome", description: "Search Font Awesome icons" }
        ]
      },
      {
        id: "raycast.font-converter",
        name: "Font Converter",
        description: "Convert fonts to TTF, WOFF, WOFF2, and EOT formats directly from Raycast",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/font-converter",
        iconUrl: "https://raw.githubusercontent.com/almatkai/font-converter/main/assets/extension-icon.png",
        categories: ["Developer Tools", "Design Tools"],
        commands: [
          {
            name: "convert-font",
            title: "Convert Font",
            description: "Convert a font file into one or more formats."
          },
          {
            name: "generate-font-face-css",
            title: "Generate @Font-Face CSS",
            description: "Generate a CSS snippet for the converted font files."
          },
          {
            name: "preview-font",
            title: "Preview Font",
            description: "Preview any installed or custom font."
          }
        ]
      },
      {
        id: "raycast.font-search",
        name: "Font Search",
        description: "Search locally installed fonts, as displayed in font book.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/font-search",
        iconUrl: "https://raw.githubusercontent.com/almatkai/font-search/main/assets/extension-icon.png",
        categories: ["Design Tools", "Developer Tools"],
        commands: [
          {
            name: "search-fonts",
            title: "Search Fonts",
            description: "Search locally installed fonts"
          }
        ]
      },
      {
        id: "raycast.format-graphql",
        name: "Format GraphQL",
        description: "Formats a GraphQL document",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/format-graphql",
        iconUrl: "https://raw.githubusercontent.com/almatkai/format-graphql/main/assets/command-icon.png",
        categories: ["Developer Tools", "Productivity"],
        commands: [
          { name: "index", title: "Format GraphQL", description: "Formats a GraphQL document" }
        ]
      },
      {
        id: "raycast.geohash-encode-decode",
        name: "Geohash",
        description: "Encode coordinates as geohash, or decode a geohash and get useful info",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/geohash-encode-decode",
        iconUrl: "https://raw.githubusercontent.com/almatkai/geohash-encode-decode/main/assets/command-icon.png",
        categories: ["Developer Tools"],
        commands: [
          {
            name: "encode-coordinates",
            title: "Encode as Geohash",
            description: "Enter a latitude and longitude and get that point's representation as a geohash for any precision"
          },
          {
            name: "decode-geohash",
            title: "Decode Geohash",
            description: "Enter a geohash and get insights such as the GeoJSON or WKT Polygon representation, centroid, neighbors, and more"
          }
        ]
      },
      {
        id: "raycast.hashrate-no",
        name: "Hashrate",
        description: "Crypto mining resources via Hashrate.no",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/hashrate-no",
        iconUrl: "https://raw.githubusercontent.com/almatkai/hashrate-no/main/assets/hashrate-no.png",
        categories: ["Data", "Finance"],
        commands: [
          { name: "coins", title: "Coins", description: "Crypto Coins with Pricing and Estimates" },
          {
            name: "gpu_estimates",
            title: "GPU Estimates",
            description: "GPU Estimates according to Power Cost ($/kWh)"
          }
        ]
      },
      {
        id: "raycast.html-colors",
        name: "HTML Colors",
        description: "Search through pallettes of standard HTML colors. Offers basic + extended set. Search by name or browse by looking at the color itself.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/html-colors",
        iconUrl: "https://raw.githubusercontent.com/almatkai/html-colors/main/assets/icon.png",
        categories: ["Developer Tools", "Design Tools"],
        commands: [
          {
            name: "html-color",
            title: "Search HTML Colors",
            description: "Quickly find HTML colors by name, hex, or RGB. Supports fuzzy search with typos and shows both basic and extended color sets."
          }
        ]
      },
      {
        id: "raycast.image-base64",
        name: "Image Base64 Converter",
        description: "Convert between base64 strings and images easily",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/image-base64",
        iconUrl: "https://raw.githubusercontent.com/almatkai/image-base64/main/assets/extension-icon.png",
        categories: ["Developer Tools"],
        commands: [
          { name: "base64-to-image", title: "Base64 to Image", description: "Convert base64 to image" },
          { name: "image-to-base64", title: "Image to Base64", description: "Convert image to base64" }
        ]
      },
      {
        id: "raycast.image-diff-checker",
        name: "Image Diff Checker",
        description: "Image Diff Checker compares two images and highlights the differences. It supports file formats such as JPEG, JPG, PNG, and GIF.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/image-diff-checker",
        iconUrl: "https://raw.githubusercontent.com/almatkai/image-diff-checker/main/assets/image-diff-checker-icon.png",
        categories: ["Developer Tools"],
        commands: [
          {
            name: "image-diff-check",
            title: "Image Diff Check",
            description: "Compares two images to identify differences."
          }
        ]
      },
      {
        id: "raycast.image-host",
        name: "Image Host",
        description: "Host an image on imgBB",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/image-host",
        iconUrl: "https://raw.githubusercontent.com/almatkai/image-host/main/assets/logo.png",
        categories: ["Media"],
        commands: [{ name: "host-image", title: "Host Image", description: "Host an image on imgBB" }]
      },
      {
        id: "raycast.image-search",
        name: "Image Web Search",
        description: "Searches image using Google Image API",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/image-search",
        iconUrl: "https://raw.githubusercontent.com/almatkai/image-search/main/assets/extension-icon.png",
        categories: ["Developer Tools"],
        commands: [
          {
            name: "search-images",
            title: "Search Web Images",
            description: "Searches Images on the web"
          }
        ]
      },
      {
        id: "raycast.image-shield",
        name: "Image Shield",
        description: "Protect images with advanced encryption and fragmentation.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/image-shield",
        iconUrl: "https://raw.githubusercontent.com/almatkai/image-shield/main/assets/extension-icon.png",
        categories: ["Security", "Media"],
        commands: [
          {
            name: "encrypt-images",
            title: "Encrypt Images",
            description: "Encrypt images into secure fragments"
          },
          {
            name: "decrypt-images",
            title: "Decrypt Images",
            description: "Restore original images from the fragments"
          }
        ]
      },
      {
        id: "raycast.image-to-ascii",
        name: "Image to Ascii",
        description: "Convert image to Ascii",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/image-to-ascii",
        iconUrl: "https://raw.githubusercontent.com/almatkai/image-to-ascii/main/assets/extension-icon.png",
        categories: ["Applications", "Media"],
        commands: [
          {
            name: "convert-image-to-ascii",
            title: "Convert Image to Ascii",
            description: "Convert image to Ascii"
          }
        ]
      },
      {
        id: "raycast.ipapi-is",
        name: "ipapi.is",
        description: "Lookup IP or ASN via ipapi.is",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/ipapi-is",
        iconUrl: "https://raw.githubusercontent.com/almatkai/ipapi-is/main/assets/ipapi-is.png",
        categories: ["Data"],
        commands: [
          { name: "lookup-ip-or-asn", title: "Lookup IP or ASN", description: "Lookup IP or ASN" }
        ]
      },
      {
        id: "raycast.ipcheck-ing",
        name: "IPCheck",
        description: "Show your local and external IPs from multiple sources, look up the location and network behind any IP address, and keep your current IP in the menu bar",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/ipcheck-ing",
        iconUrl: "https://raw.githubusercontent.com/almatkai/ipcheck-ing/main/assets/ipchecking.png",
        categories: ["Developer Tools"],
        commands: [
          { name: "main", title: "Show My IPs", description: "Show All My IPs" },
          {
            name: "query-ip",
            title: "Query IP",
            description: "Look up the location, ISP and network behind any IP address"
          },
          {
            name: "menubar",
            title: "IP in Menu Bar",
            description: "Keep your current external IP in the menu bar"
          }
        ]
      },
      {
        id: "raycast.ip-finder",
        name: "Ip Finder - Network Scanner",
        description: "Scan your local network to find assigned IPs and get recommendations for available addresses",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/ip-finder",
        iconUrl: "https://raw.githubusercontent.com/almatkai/ip-finder/main/assets/command-icon.png",
        categories: ["Developer Tools", "System"],
        commands: [
          {
            name: "index",
            title: "Scan Network",
            description: "Scan local network for assigned IPs and get recommendations"
          }
        ]
      },
      {
        id: "raycast.ip-geolocation",
        name: "IP Geolocation",
        description: "Show local and public IPv4/IPv6 address.\nQuery geolocation for any IP address or domain.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/ip-geolocation",
        iconUrl: "https://raw.githubusercontent.com/almatkai/ip-geolocation/main/assets/extension-icon.png",
        categories: ["Developer Tools", "Web"],
        commands: [
          {
            name: "query-ip-geolocation",
            title: "Query IP Geolocation",
            description: "Query geolocation for any IP address or domain."
          },
          {
            name: "my-ip-geolocation",
            title: "My IP Geolocation",
            description: "Show geolocation for local and public IPv4/IPv6 address."
          },
          { name: "copy-ip", title: "Copy IP", description: "Copy local/public IPv4/IPv6 address." }
        ]
      },
      {
        id: "raycast.ip-tools",
        name: "IP Tools",
        description: "This is an extension plugin for converting, validating, and calculating IP subnets. (net, mask, netmask, ip2long, long2ip, cidr, ipv4, ipv6 & geoLocation)",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/ip-tools",
        iconUrl: "https://raw.githubusercontent.com/almatkai/ip-tools/main/assets/extension_icon.png",
        categories: ["Developer Tools", "Productivity"],
        commands: [
          {
            name: "ip2long",
            title: "Convert IP Address to Number",
            description: "Convert IPv4 or IPv6 Address to Number"
          },
          {
            name: "long2ip",
            title: "Convert Number to IP Address",
            description: "Convert Number to IPv4 or IPv6"
          },
          {
            name: "ip2BinHex",
            title: "Convert IP Address to Bin and Hex",
            description: "Convert IPv4 or IPv6 Address to binary and hexadecimal"
          },
          {
            name: "parseCIDR",
            title: "Convert CIDR to IP Range",
            description: "Convert CIDR to IPv4 or IPv6 Range"
          },
          {
            name: "isValidIP",
            title: "Verify IP Address Is Valid",
            description: "Verify if the IP address Is valid"
          },
          {
            name: "isValidCIDR",
            title: "Verify CIDR Is Valid",
            description: "Verify if the CIDR is valid"
          },
          {
            name: "toIPv6Format",
            title: "Converts IPv4 to IPv6 Address",
            description: "Converts IPv4 address to IPv6 address"
          },
          {
            name: "toIPv6Expand",
            title: "Converts IPv6 to Expanded or Compressed",
            description: "Converts IPv6 to expanded or compressed format"
          },
          {
            name: "geoLocation",
            title: "Query IP Geolocation Information",
            description: "Query IPv4 or Ipv6 Geolocation Information"
          }
        ]
      },
      {
        id: "raycast.json2ts",
        name: "Json2TS",
        description: "Generate TypeScript interfaces from JSON",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/json2ts",
        iconUrl: "https://raw.githubusercontent.com/almatkai/json2ts/main/assets/command-icon.png",
        categories: ["Developer Tools"],
        commands: [
          { name: "index", title: "Json2ts", description: "Generate TypeScript interfaces from JSON" }
        ]
      },
      {
        id: "raycast.json-editor",
        name: "JSON Editor",
        description: "Format, filter, and convert JSON directly in Tezbar.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/json-editor",
        iconUrl: "https://raw.githubusercontent.com/almatkai/json-editor/main/assets/icon.svg",
        categories: ["Productivity", "Developer Tools", "Data"],
        commands: [
          {
            name: "edit-json",
            title: "Edit JSON",
            description: "Format, filter, minify, escape, and convert JSON."
          }
        ]
      },
      {
        id: "raycast.json-format",
        name: "Format JSON",
        description: "Formats a JSON file with a selected indentation. If the JSON is stringified, it will be parsed before formatting.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/json-format",
        iconUrl: "https://raw.githubusercontent.com/almatkai/json-format/main/assets/command-icon.png",
        categories: ["Data", "Developer Tools"],
        commands: [
          {
            name: "formatClipboard",
            title: "Format Clipboard JSON",
            description: "Formats a JSON/JS Object string stored in clipboard and copies/pastes it back"
          },
          {
            name: "formatSelection",
            title: "Format Selected JSON",
            description: "Formats a JSON/JS Object string selected in the foremost editor and copies/pastes it back"
          },
          {
            name: "formatSelectionAndShow",
            title: "Format Selected JSON and Show",
            description: "Formats a JSON/JS Object string selected in the foremost editor and shows it in the viewer"
          },
          { name: "index", title: "Format JSON", description: "Formats a JSON/JS Object" },
          {
            name: "formatJsonLines",
            title: "Format Array of JSON to JSONLines",
            description: "Formats an array of JSON/JS Object to JSONLines"
          },
          {
            name: "formatToJsonValue",
            title: "Format Text into Valid JSON Value",
            description: "Formats text into a valid JSON value with double quotes and escapes"
          }
        ]
      },
      {
        id: "raycast.json-resume",
        name: "JSON Resume",
        description: "Parse JSON Resume",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/json-resume",
        iconUrl: "https://raw.githubusercontent.com/almatkai/json-resume/main/assets/extension-icon.png",
        categories: ["Productivity", "Communication"],
        commands: [
          {
            name: "open-resume",
            title: "Open Resume",
            description: "Open a resume by providing a JSON resume URL"
          },
          {
            name: "search-resumes",
            title: "View Saved Resumes",
            description: "View and manage saved resumes"
          }
        ]
      },
      {
        id: "raycast.json-to-toon-converter",
        name: "JSON to TOON Converter",
        description: "Convert JSON data into TOON format instantly.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/json-to-toon-converter",
        iconUrl: "https://raw.githubusercontent.com/almatkai/json-to-toon-converter/main/assets/extension-icon.png",
        categories: ["Developer Tools"],
        commands: [
          { name: "convert-json", title: "Convert JSON", description: "Convert JSON to TOON format" }
        ]
      },
      {
        id: "raycast.kind-words",
        name: "Kind Words",
        description: "Random compliments and gratitude prompts to lift the mood.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/kind-words",
        iconUrl: "https://raw.githubusercontent.com/almatkai/kind-words/main/assets/extension-icon.png",
        categories: ["Fun"],
        commands: [
          {
            name: "random-compliment",
            title: "Random Compliment",
            description: "Browse compliments with one preselected at random; filter by tone."
          },
          {
            name: "quick-compliment",
            title: "Quick Compliment",
            description: "Copy a random compliment instantly."
          },
          {
            name: "gratitude-note",
            title: "Gratitude Note",
            description: "Browse gratitude prompts with usage notes and opener examples."
          }
        ]
      },
      {
        id: "raycast.language-tool",
        name: "Language Tool - Spell & Grammar Checker",
        description: "Raycast extension that provides instant spelling and grammar correction using LanguageTool. Type any text, get real-time suggestions, and copy the improved version. Ideal for developers, writers, and anyone who wants fast, high-quality text corrections in multiple languages.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/language-tool",
        iconUrl: "https://raw.githubusercontent.com/almatkai/language-tool/main/assets/extension-icon.png",
        categories: ["Communication"],
        commands: [
          {
            name: "check-text",
            title: "Check Text",
            description: "Check a text with LanguageTool for possible style and grammar issues."
          },
          {
            name: "check-text-instant",
            title: "Check Text Instant",
            description: "Check a text with LanguageTool using the clipboard content and paste the result instantly. The language is detected automatically."
          }
        ]
      },
      {
        id: "raycast.latex-math-symbols",
        name: "LateX Math Symbols",
        description: "Search and copy LateX Math Symbols",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/latex-math-symbols",
        iconUrl: "https://raw.githubusercontent.com/almatkai/latex-math-symbols/main/assets/icon.png",
        categories: ["Developer Tools"],
        commands: [
          {
            name: "index",
            title: "Search LateX Symbols",
            description: "Search through a library of latex math symbols from amsmath"
          }
        ]
      },
      {
        id: "raycast.list-randomizer",
        name: "List Randomizer",
        description: "Randomizes a list of items",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/list-randomizer",
        iconUrl: "https://raw.githubusercontent.com/almatkai/list-randomizer/main/assets/command-icon.png",
        categories: ["Data"],
        commands: [
          {
            name: "index",
            title: "Randomize List",
            description: "Add items to a list and randomize it"
          }
        ]
      },
      {
        id: "raycast.lorem-ipsum",
        name: "Lorem Ipsum",
        description: "Generate placeholder content",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/lorem-ipsum",
        iconUrl: "https://raw.githubusercontent.com/almatkai/lorem-ipsum/main/assets/paragraph-icon.png",
        categories: ["Data", "Design Tools", "Developer Tools", "Web"],
        commands: [
          {
            name: "paragraphs",
            title: "Generate Paragraphs",
            description: "Generate random paragraphs and copy them to the clipboard"
          },
          {
            name: "sentences",
            title: "Generate Sentences",
            description: "Generate random sentences and copy them to the clipboard"
          },
          {
            name: "words",
            title: "Generate Words",
            description: "Generate random words and copy them to the clipboard"
          },
          {
            name: "ai-generate",
            title: "Generate with AI",
            description: "Use AI to generate some placeholder text"
          }
        ]
      },
      {
        id: "raycast.markdown-image-to-html",
        name: "Markdown Image to HTML",
        description: "Transform a Markdown Image to HTML. This allows us to fix image size in Markdown base files.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/markdown-image-to-html",
        iconUrl: "https://raw.githubusercontent.com/almatkai/markdown-image-to-html/main/assets/command-icon.png",
        categories: ["Developer Tools", "Productivity"],
        commands: [
          {
            name: "index",
            title: "Markdown Image to HTML",
            description: "Transform a Markdown Image to HTML. This allows us to fix image size in Markdown base files."
          }
        ]
      },
      {
        id: "raycast.markdown-preview",
        name: "Markdown Preview",
        description: "A simple way to preview markdown content",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/markdown-preview",
        iconUrl: "https://raw.githubusercontent.com/almatkai/markdown-preview/main/assets/md-preview-icon.png",
        categories: ["Productivity", "Web", "Developer Tools", "Documentation"],
        commands: [
          {
            name: "preview-markdown",
            title: "Preview Markdown",
            description: "Write and preview Markdown content with real-time rendering"
          }
        ]
      },
      {
        id: "raycast.markdown-reference",
        name: "Markdown Reference",
        description: "Markdown documentation at your fingertips",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/markdown-reference",
        iconUrl: "https://raw.githubusercontent.com/almatkai/markdown-reference/main/assets/logo.png",
        categories: ["Developer Tools"],
        commands: [
          {
            name: "markdown-reference",
            title: "Search Markdown Syntax",
            description: "Quickly search up how to format with Markdown"
          }
        ]
      },
      {
        id: "raycast.markdown-styler",
        name: "Markdown Styler",
        description: "Convert Markdown to styled HTML with inline CSS for content platforms",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/markdown-styler",
        iconUrl: "https://raw.githubusercontent.com/almatkai/markdown-styler/main/assets/command-icon.png",
        categories: ["Productivity"],
        commands: [
          {
            name: "mp-style-markdown",
            title: "Style Markdown",
            description: "Convert Markdown to styled HTML with inline CSS for content platforms"
          }
        ]
      },
      {
        id: "raycast.markdown-table-generator",
        name: "Markdown Table Generator",
        description: "Specify your width and height, and receive a markdown table that you can input your own values into.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/markdown-table-generator",
        iconUrl: "https://raw.githubusercontent.com/almatkai/markdown-table-generator/main/assets/command-icon.png",
        categories: ["Developer Tools"],
        commands: [
          {
            name: "generate-markdown-table",
            title: "Generate Markdown Table",
            description: "Based on the inputted integer arguments, number of rows and number of columns, you will receive a markdown table."
          }
        ]
      },
      {
        id: "raycast.markdown-this",
        name: "Markdown This",
        description: "Convert Markdown text in clipboard to HTML",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/markdown-this",
        iconUrl: "https://raw.githubusercontent.com/almatkai/markdown-this/main/assets/command-icon.png",
        categories: ["Developer Tools"],
        commands: [
          {
            name: "index",
            title: "Convert to HTML",
            description: "Convert markdown text in clipboard to HTML"
          }
        ]
      },
      {
        id: "raycast.markdown-to-jira",
        name: "Markdown to Jira",
        description: "Format a Markdown text into Jira markup.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/markdown-to-jira",
        iconUrl: "https://raw.githubusercontent.com/almatkai/markdown-to-jira/main/assets/icon.png",
        categories: ["Documentation", "Productivity"],
        commands: [
          {
            name: "markdown-to-jira",
            title: "Markdown to Jira",
            description: "Format a Markdown text into Jira markup."
          }
        ]
      },
      {
        id: "raycast.mastodon-search",
        name: "Mastodon Search",
        description: "Search for People or Hashtags on Mastodon.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/mastodon-search",
        iconUrl: "https://raw.githubusercontent.com/almatkai/mastodon-search/main/assets/command-icon.png",
        categories: ["Communication", "Web"],
        commands: [
          {
            name: "search",
            title: "Search Mastodon",
            description: "Search Mastodon for people or hashtags."
          }
        ]
      },
      {
        id: "raycast.modify-hash",
        name: "Modify Hash",
        description: "Batch modify the hash of media files, which can be simply used for uploading online disk to prevent censorship.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/modify-hash",
        iconUrl: "https://raw.githubusercontent.com/almatkai/modify-hash/main/assets/hasher.png",
        categories: ["Developer Tools", "Productivity"],
        commands: [
          {
            name: "modifyHash",
            title: "Modify Hash",
            description: "Batch modify the hash of media files."
          },
          {
            name: "restoreHash",
            title: "Restore Hash",
            description: "Batch restore the hash of media files."
          },
          {
            name: "zipCompress",
            title: "Zip Compress",
            description: "Compress files to a portable zip archive."
          },
          {
            name: "zipExtract",
            title: "Zip Extract",
            description: "Extract files from a portable zip archive."
          }
        ]
      },
      {
        id: "raycast.node-release-notes",
        name: "Node Release Notes",
        description: "Quickly browse and access detailed information of Node.js versions. Find release notes, publication dates, and documentation links for any Node.js version. Ideal for developers and sysadmins to stay informed about Node.js updates and ensure compatibility with their projects.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/node-release-notes",
        iconUrl: "https://raw.githubusercontent.com/almatkai/node-release-notes/main/assets/app-icon.png",
        categories: ["Developer Tools"],
        commands: [
          {
            name: "index",
            title: "Node Versions",
            description: "Quickly browse and access detailed information of Node.js versions"
          }
        ]
      },
      {
        id: "raycast.oklch-color-converter",
        name: "OKLCH Color Converter",
        description: "Convert colors between formats with wide gamut support",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/oklch-color-converter",
        iconUrl: "https://raw.githubusercontent.com/almatkai/oklch-color-converter/main/icon.png",
        categories: ["Design Tools"],
        commands: [
          {
            name: "convert-color",
            title: "Convert Color",
            description: "Convert colors between different color spaces and formats"
          }
        ]
      },
      {
        id: "raycast.onelook-thesaurus",
        name: "OneLook Thesaurus",
        description: "A powerful English thesaurus and brainstorming tool that lets you describe what you're looking for in plain terms.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/onelook-thesaurus",
        iconUrl: "https://raw.githubusercontent.com/almatkai/onelook-thesaurus/main/assets/command-icon.png",
        categories: ["Productivity"],
        commands: [
          {
            name: "search",
            title: "Search",
            description: "Enter a word, phrase, description, or pattern above to find synonyms, related words, and more."
          }
        ]
      },
      {
        id: "raycast.one-time-password",
        name: "One Time Password",
        description: "Generate time-based one-time passwords",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/one-time-password",
        iconUrl: "https://raw.githubusercontent.com/almatkai/one-time-password/main/assets/icon.png",
        categories: ["Security", "Productivity"],
        commands: [
          {
            name: "one-time-password",
            title: "One Time Password",
            description: "Generate time-based one-time passwords"
          },
          {
            name: "paste-recent-otp",
            title: "Paste Recent OTP",
            description: "Paste the OTP code from the most recently used account into the active app"
          }
        ]
      },
      {
        id: "raycast.password-generator",
        name: "Password Generator",
        description: "Generate secure random and memorable passwords.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/password-generator",
        iconUrl: "https://raw.githubusercontent.com/almatkai/password-generator/main/assets/icon.svg",
        categories: ["Productivity", "Developer Tools", "Security"],
        commands: [
          {
            name: "generate-random-password",
            title: "Generate Random Password",
            description: "Generate a secure password with configurable character groups."
          },
          {
            name: "copy-random-password",
            title: "Copy Random Password",
            description: "Generate a random password and copy it immediately."
          },
          {
            name: "generate-memorable-password",
            title: "Generate Memorable Password",
            description: "Generate a passphrase from words, numbers, and symbols."
          }
        ]
      },
      {
        id: "raycast.password-strength",
        name: "Password Strength",
        description: "Check your passwords",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/password-strength",
        iconUrl: "https://raw.githubusercontent.com/almatkai/password-strength/main/assets/extension-icon.png",
        categories: ["Web", "Security"],
        commands: [
          {
            name: "check-password-strength",
            title: "Check Password Strength",
            description: "Check the strength of your password"
          }
        ]
      },
      {
        id: "raycast.paste-as-plain-text",
        name: "Paste as Plain Text",
        description: "Paste text from the clipboard as any format.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/paste-as-plain-text",
        iconUrl: "https://raw.githubusercontent.com/almatkai/paste-as-plain-text/main/assets/paste-icon.png",
        categories: ["Developer Tools", "Productivity", "System"],
        commands: [
          {
            name: "paste-as-plain-text",
            title: "Paste as",
            description: "Paste text from the clipboard as any format."
          }
        ]
      },
      {
        id: "raycast.qrcode-generator",
        name: "QR Code Generator",
        description: "Generate QR codes from text or URLs.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/qrcode-generator",
        iconUrl: "https://raw.githubusercontent.com/almatkai/qrcode-generator/main/assets/command-icon.png",
        categories: ["Developer Tools"],
        commands: [
          {
            name: "index",
            title: "Generate QR Code",
            description: "Generate a QR code from text or URL"
          },
          {
            name: "clipboard",
            title: "Generate QR Code from Clipboard",
            description: "Generate a QR code from clipboard content"
          },
          {
            name: "selection",
            title: "Generate QR Code from Selection",
            description: "Generate a QR code from selection content"
          }
        ]
      },
      {
        id: "raycast.random-data-generator",
        name: "Random Data Generator",
        description: "Generate random data using Faker library",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/random-data-generator",
        iconUrl: "https://raw.githubusercontent.com/almatkai/random-data-generator/main/assets/random.png",
        categories: ["Data", "Developer Tools"],
        commands: [
          {
            name: "index",
            title: "Generate Random Data",
            description: "Generate random data using Faker library"
          },
          {
            name: "open-quicklink",
            title: "Open Quicklink",
            description: "Used internally to run quicklinks from the generate command."
          }
        ]
      },
      {
        id: "raycast.regex-tester",
        name: "Regex Tester",
        description: "Preview and test regular expressions",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/regex-tester",
        iconUrl: "https://raw.githubusercontent.com/almatkai/regex-tester/main/assets/command-icon.png",
        categories: ["Developer Tools", "Productivity"],
        commands: [
          { name: "index", title: "Test Regex", description: "Preview and test regular expressions" }
        ]
      },
      {
        id: "raycast.text-differ",
        name: "Text Differ",
        description: "Compare two selected text files directly in Tezbar",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/text-differ",
        iconUrl: "https://raw.githubusercontent.com/almatkai/text-differ/main/assets/command-icon.png",
        categories: ["Applications", "Developer Tools"],
        commands: [
          {
            name: "open-with-text-differ",
            title: "Compare Selected Files",
            description: "Show a line-by-line diff for two files selected in Finder or File Explorer"
          }
        ]
      },
      {
        id: "raycast.text-rewrap",
        name: "Text Rewrap",
        description: "Rewraps Text at a given width",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/text-rewrap",
        iconUrl: "https://raw.githubusercontent.com/almatkai/text-rewrap/main/assets/text-rewrap.png",
        categories: ["Developer Tools"],
        commands: [
          {
            name: "text-rewrap-form",
            title: "Text Rewrap from Form",
            description: "Rewraps Text provided in the form at the chosen width"
          },
          {
            name: "text-rewrap-clipboard",
            title: "Text Rewrap from Clipboard",
            description: "Rewraps text from clipboard at the chosen width"
          },
          {
            name: "text-rewrap-selection",
            title: "Text Rewrap from Selection",
            description: "Rewraps text from currently highlighted selection at the chosen width"
          }
        ]
      },
      {
        id: "raycast.timezone-converter",
        name: "Timezone Converter",
        description: "Converts any time to any timezone",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/timezone-converter",
        iconUrl: "https://raw.githubusercontent.com/almatkai/timezone-converter/main/assets/command-icon.png",
        categories: ["Developer Tools"],
        commands: [
          {
            name: "index",
            title: "Timezone Converter",
            description: "Converts any time to any timezone"
          }
        ]
      },
      {
        id: "raycast.unicode-symbols",
        name: "Unicode Symbols Search",
        description: "Browse and copy-paste common Unicode symbols.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/unicode-symbols",
        iconUrl: "https://raw.githubusercontent.com/almatkai/unicode-symbols/main/assets/command-icon.png",
        categories: ["Developer Tools"],
        commands: [
          {
            name: "index",
            title: "Search Unicode Symbol",
            description: "Browse and copy-paste common Unicode symbols."
          }
        ]
      },
      {
        id: "raycast.uuid-generator",
        name: "UUID Generator",
        description: "A quick way to generate UUIDs without opening the browser",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/uuid-generator",
        iconUrl: "https://raw.githubusercontent.com/almatkai/uuid-generator/main/assets/command-icon.png",
        categories: ["Developer Tools"],
        commands: [
          {
            name: "generate",
            title: "Generate UUIDs",
            description: "Copy generated UUIDs to the clipboard"
          },
          {
            name: "generateV5",
            title: "Generate V5 UUIDs",
            description: "Copy generated UUIDs V5 to the clipboard"
          },
          {
            name: "generateV7",
            title: "Generate V7 UUIDs",
            description: "Copy generated UUIDs V7 to the clipboard"
          },
          {
            name: "generateUlid",
            title: "Generate ULIDs",
            description: "Copy generated ULIDs to the clipboard"
          },
          {
            name: "generateTypeID",
            title: "Generate TypeIDs",
            description: "Copy generated TypeIDs to the clipboard"
          },
          {
            name: "parseTypeID",
            title: "Parse TypeIDs",
            description: "Parse TypeID and copy resulting UUID to the clipboard"
          },
          {
            name: "formatUuid",
            title: "Format UUID",
            description: "Format UUID without dashes into standard UUID format and copy to the clipboard"
          },
          {
            name: "packTypeID",
            title: "Pack TypeID",
            description: "Convert a UUID to a packed Type ID with a suffix"
          },
          { name: "viewHistory", title: "View History", description: "View the UUID history" }
        ]
      },
      {
        id: "raycast.word-count",
        name: "Word Count",
        description: "Count characters, words, sentences, and paragraphs in text.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/word-count",
        iconUrl: "https://raw.githubusercontent.com/almatkai/word-count/main/assets/word-count.png",
        categories: ["Productivity"],
        commands: [
          {
            name: "count",
            title: "Count",
            description: "Count characters, words, sentences, and paragraphs in text."
          },
          {
            name: "count-screenshot",
            title: "Count Clipboard Text",
            description: "Count words, characters, sentences, and paragraphs in clipboard text."
          },
          {
            name: "count-overlay",
            title: "Word Count in Overlay",
            description: "Show word count in an overlay on top of other applications."
          }
        ]
      },
      {
        id: "raycast.world-clock",
        name: "World Clock",
        description: "Query the current time of a region, ip or domain.",
        author: "almatkai",
        owner: "almatkai",
        version: "0.1.0",
        repository: "https://github.com/almatkai/world-clock",
        iconUrl: "https://raw.githubusercontent.com/almatkai/world-clock/main/assets/command-icon.png",
        categories: ["Data", "Other"],
        commands: [
          {
            name: "query-world-time",
            title: "Query World Time",
            description: "Query the current time for a timezone with region."
          },
          {
            name: "query-world-time-menu-bar",
            title: "Menubar World Time",
            description: "Query the current time of starred timezones from the menu bar."
          },
          {
            name: "query-ip-time",
            title: "Query IP Time",
            description: "Query the current time based on IP or domain."
          }
        ]
      }
      // Additional Tezbar-maintained extensions are appended below.
    ];
  }
});

// src/main/extension-registry.ts
var extension_registry_exports = {};
__export(extension_registry_exports, {
  extensionRegistryEvents: () => extensionRegistryEvents,
  getCatalog: () => getCatalog,
  getExtensionPreferenceSetup: () => getExtensionPreferenceSetup,
  getExtensionPreferences: () => getExtensionPreferences,
  getExtensionScreenshotUrls: () => getExtensionScreenshotUrls,
  getInstalledExtensionNames: () => getInstalledExtensionNames,
  installExtension: () => installExtension,
  installExtensionDeps: () => installExtensionDeps,
  installRegistryExtension: () => installRegistryExtension,
  installSpecificPackages: () => installSpecificPackages,
  isExtensionInstalled: () => isExtensionInstalled,
  listInstalledExtensionSlugsFromDisk: () => listInstalledExtensionSlugsFromDisk,
  listInstalledRegistryExtensions: () => listInstalledRegistryExtensions,
  resolveInstalledPackageJsonPath: () => resolveInstalledPackageJsonPath,
  saveExtensionPreferences: () => saveExtensionPreferences,
  scoreCatalogEntrySearch: () => scoreCatalogEntrySearch,
  searchExtensionCatalog: () => searchExtensionCatalog,
  shouldShowExtensionPreferenceSetup: () => shouldShowExtensionPreferenceSetup,
  uninstallExtension: () => uninstallExtension,
  uninstallRegistryExtension: () => uninstallRegistryExtension
});
function emitInstallProgress(name, progress) {
  extensionRegistryEvents.emit("progress", {
    id: normalizeRaymesExtensionId(name),
    progress: Math.max(0, Math.min(100, progress))
  });
}
async function installDependenciesWithProgress(name, installPath) {
  let progress = 60;
  emitInstallProgress(name, progress);
  const heartbeat = setInterval(() => {
    progress = Math.min(72, progress + 1);
    emitInstallProgress(name, progress);
  }, 1500);
  heartbeat.unref?.();
  try {
    return await installDepsWithBun(installPath);
  } finally {
    clearInterval(heartbeat);
  }
}
function hasNodeModules(extPath) {
  try {
    return fs3.existsSync(path3.join(extPath, "node_modules"));
  } catch {
    return false;
  }
}
function githubApiHeaders() {
  return {
    "User-Agent": "Tezbar",
    Accept: "application/vnd.github+json"
  };
}
async function fetchWithTimeout(url, options = {}, timeoutMs = 45e3) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetch(url, {
      ...options,
      signal: controller.signal
    });
  } finally {
    clearTimeout(timeout);
  }
}
async function fetchRepoTreeEntries(forceRefresh = false) {
  if (!forceRefresh && repoTreeCache && Date.now() - repoTreeCache.fetchedAt < REPO_TREE_TTL_MS) {
    return repoTreeCache.entries;
  }
  const response = await fetchWithTimeout(GITHUB_TREE_API, { headers: githubApiHeaders() }, 9e4);
  if (!response.ok) {
    throw new Error(`GitHub tree fetch failed with ${response.status} ${response.statusText}`);
  }
  const data = await response.json();
  const rawEntries = Array.isArray(data?.tree) ? data.tree : [];
  const entries = rawEntries.map((entry) => ({
    path: String(entry?.path || ""),
    type: String(entry?.type || ""),
    size: typeof entry?.size === "number" ? entry.size : void 0
  })).filter((entry) => Boolean(entry.path));
  repoTreeCache = {
    fetchedAt: Date.now(),
    entries
  };
  return entries;
}
async function downloadExtensionFromTree(name, tmpDir) {
  const treeEntries = await fetchRepoTreeEntries();
  const prefix = `extensions/${name}/`;
  const fileEntries = treeEntries.filter((entry) => entry.type === "blob" && entry.path.startsWith(prefix));
  if (fileEntries.length === 0) return null;
  const srcDir = path3.join(tmpDir, "extensions", name);
  fs3.mkdirSync(srcDir, { recursive: true });
  for (const entry of fileEntries) {
    const relativePath = entry.path.slice(prefix.length);
    if (!relativePath) continue;
    const destination = path3.join(srcDir, relativePath);
    fs3.mkdirSync(path3.dirname(destination), { recursive: true });
  }
  const CONCURRENCY = 30;
  let index = 0;
  const downloadOne = async () => {
    while (index < fileEntries.length) {
      const i = index++;
      const entry = fileEntries[i];
      const relativePath = entry.path.slice(prefix.length);
      if (!relativePath) continue;
      const destination = path3.join(srcDir, relativePath);
      const fileUrl = `${GITHUB_RAW}/${entry.path}`;
      const response = await fetchWithTimeout(
        fileUrl,
        {
          headers: {
            "User-Agent": "Tezbar",
            Accept: "application/octet-stream"
          }
        },
        9e4
      );
      if (!response.ok) {
        throw new Error(`Failed to download ${entry.path} (${response.status} ${response.statusText})`);
      }
      const data = await response.arrayBuffer();
      fs3.writeFileSync(destination, Buffer.from(data));
    }
  };
  const workers = Array.from({ length: Math.min(CONCURRENCY, fileEntries.length) }, () => downloadOne());
  await Promise.all(workers);
  console.log(`Downloaded ${fileEntries.length} files for "${name}"`);
  return srcDir;
}
function downloadExtensionViaSparseGit(name, tmpDir) {
  const checkoutDir = path3.join(tmpDir, "raymes-extensions-source");
  try {
    (0, import_child_process2.execFileSync)(
      "git",
      ["clone", "--depth", "1", "--filter=blob:none", "--sparse", RAYMES_EXTENSIONS_GIT, checkoutDir],
      { stdio: "ignore", timeout: 18e4 }
    );
    (0, import_child_process2.execFileSync)("git", ["-C", checkoutDir, "sparse-checkout", "set", `extensions/${name}`], {
      stdio: "ignore",
      timeout: 12e4
    });
    const extensionDir = path3.join(checkoutDir, "extensions", name);
    return fs3.existsSync(path3.join(extensionDir, "package.json")) ? extensionDir : null;
  } catch (error) {
    console.warn(`Sparse git fallback failed for "${name}":`, error);
    return null;
  }
}
function coerceCatalogEntry(raw) {
  if (!raw || typeof raw !== "object") return null;
  const name = typeof raw.name === "string" ? raw.name : "";
  if (!name) return null;
  const commands = Array.isArray(raw.commands) ? raw.commands.filter((cmd) => cmd && typeof cmd === "object" && cmd.name).map((cmd) => ({
    name: String(cmd.name || ""),
    title: String(cmd.title || cmd.name || ""),
    description: String(cmd.description || "")
  })) : [];
  return {
    name,
    title: typeof raw.title === "string" ? raw.title : name,
    description: typeof raw.description === "string" ? raw.description : "",
    author: typeof raw.author === "string" ? raw.author : "",
    contributors: Array.isArray(raw.contributors) ? raw.contributors.filter((v) => typeof v === "string") : [],
    icon: typeof raw.icon === "string" ? raw.icon : "",
    iconUrl: typeof raw.iconUrl === "string" ? raw.iconUrl : "",
    screenshotUrls: Array.isArray(raw.screenshotUrls) ? raw.screenshotUrls.filter((v) => typeof v === "string") : [],
    categories: Array.isArray(raw.categories) ? raw.categories.filter((v) => typeof v === "string") : [],
    platforms: Array.isArray(raw.platforms) ? raw.platforms.filter((v) => typeof v === "string") : [],
    commands,
    installCount: typeof raw.installCount === "number" ? raw.installCount : typeof raw.install_count === "number" ? raw.install_count : void 0
  };
}
function getCatalogPath() {
  return path3.join(app.getPath("userData"), "extension-catalog.json");
}
function getExtensionsDir() {
  const dir = path3.join(app.getPath("userData"), "extensions");
  if (!fs3.existsSync(dir)) {
    fs3.mkdirSync(dir, { recursive: true });
  }
  return dir;
}
function getInstalledPath(name) {
  return path3.join(getExtensionsDir(), name);
}
function getLegacyInstalledPath(name) {
  const slug = slugFromRaymesExtensionId(name);
  return path3.join(getExtensionsDir(), "packages", normalizeRaymesExtensionId(slug));
}
function getLegacyRegistryInstalledPath(name) {
  const slug = slugFromRaymesExtensionId(name);
  return path3.join(app.getPath("userData"), "extension-registry", "packages", normalizeRaymesExtensionId(slug));
}
function resolveInstalledExtensionPathForRaymes(name) {
  const slug = slugFromRaymesExtensionId(name);
  if (!slug) return null;
  const candidates = [
    getInstalledPath(slug),
    getInstalledPath(normalizeRaymesExtensionId(slug)),
    getLegacyInstalledPath(slug),
    getLegacyRegistryInstalledPath(slug)
  ];
  for (const candidate of candidates) {
    if (fs3.existsSync(path3.join(candidate, "package.json"))) return candidate;
  }
  return null;
}
function loadCatalogFromDisk(allowAnyVersion = false) {
  try {
    const data = fs3.readFileSync(getCatalogPath(), "utf-8");
    const parsed = JSON.parse(data);
    if (!allowAnyVersion && typeof parsed.version === "number" && parsed.version < CATALOG_VERSION) {
      return null;
    }
    const entries = Array.isArray(parsed.entries) ? parsed.entries.map((entry) => coerceCatalogEntry(entry)).filter(Boolean) : [];
    if (entries.length === 0) return null;
    return {
      entries,
      fetchedAt: typeof parsed.fetchedAt === "number" ? parsed.fetchedAt : Date.now(),
      version: typeof parsed.version === "number" ? parsed.version : CATALOG_VERSION
    };
  } catch {
  }
  return null;
}
function saveCatalogToDisk(catalog) {
  try {
    fs3.writeFileSync(getCatalogPath(), JSON.stringify(catalog));
  } catch (e) {
    console.error("Failed to save catalog:", e);
  }
}
async function getCatalogWithoutSharedRequest(forceRefresh = false) {
  if (!forceRefresh && catalogCache && Date.now() - catalogCache.fetchedAt < CATALOG_TTL) {
    return catalogCache.entries;
  }
  if (!forceRefresh) {
    const diskCache2 = loadCatalogFromDisk(false);
    if (diskCache2 && Date.now() - diskCache2.fetchedAt < CATALOG_TTL) {
      catalogCache = diskCache2;
      return diskCache2.entries;
    }
  }
  try {
    console.log("Fetching extension catalog from API\u2026");
    const entries = await fetchCatalogFromAPI();
    const cache = {
      entries,
      fetchedAt: Date.now(),
      version: CATALOG_VERSION
    };
    catalogCache = cache;
    saveCatalogToDisk(cache);
    console.log(`Extension catalog (API): ${entries.length} extensions cached.`);
    return entries;
  } catch (apiError) {
    console.warn("API catalog fetch failed:", apiError?.message || apiError);
  }
  const diskCache = loadCatalogFromDisk(true);
  if (diskCache) {
    catalogCache = diskCache;
    console.log(`Extension catalog (disk cache): ${diskCache.entries.length} extensions from cache.`);
    return diskCache.entries;
  }
  return [];
}
async function getCatalog(forceRefresh = false) {
  if (catalogFetchPromise) return catalogFetchPromise;
  const request2 = getCatalogWithoutSharedRequest(forceRefresh);
  catalogFetchPromise = request2;
  try {
    return await request2;
  } finally {
    if (catalogFetchPromise === request2) catalogFetchPromise = null;
  }
}
async function getExtensionScreenshotUrls(name) {
  if (!name) return [];
  try {
    const urls = await getExtensionScreenshotsFromAPI(name);
    if (urls.length > 0) return urls;
  } catch (apiError) {
    console.warn(`API screenshots fetch failed for ${name}:`, apiError?.message || apiError);
  }
  try {
    const url = `${GITHUB_API}/extensions/${encodeURIComponent(name)}/metadata`;
    const response = await fetch(url, {
      headers: {
        "User-Agent": "Tezbar",
        Accept: "application/vnd.github+json"
      }
    });
    if (!response.ok) return [];
    const data = await response.json();
    if (!Array.isArray(data)) return [];
    const imagePattern = /\.(png|jpe?g|webp|gif)$/i;
    return data.filter((entry) => entry?.type === "file" && imagePattern.test(entry?.name || "")).sort(
      (a, b) => String(a?.name || "").localeCompare(String(b?.name || ""), void 0, {
        numeric: true
      })
    ).map((entry) => String(entry?.download_url || "")).filter(Boolean);
  } catch (e) {
    console.warn(`Failed to load screenshots for ${name}:`, e);
    return [];
  }
}
async function installSpecificPackages(extPath, packageNames) {
  const unique = Array.from(new Set(packageNames.map((name) => String(name || "").trim()).filter(Boolean)));
  if (unique.length === 0) return;
  console.log(`Installing missing packages for ${path3.basename(extPath)}: ${unique.join(", ")}`);
  const ok = await installSpecificPackagesWithBun(extPath, unique);
  if (!ok) {
    throw new Error(`Bun failed to install packages for ${path3.basename(extPath)}`);
  }
}
async function installExtensionDeps(extPath) {
  const pkgPath = path3.join(extPath, "package.json");
  if (!fs3.existsSync(pkgPath)) return;
  let pkg;
  try {
    pkg = JSON.parse(fs3.readFileSync(pkgPath, "utf-8"));
  } catch {
    return;
  }
  const deps = {
    ...pkg.dependencies || {},
    ...pkg.optionalDependencies || {}
  };
  const thirdPartyDeps = Object.entries(deps).filter(([name]) => !name.startsWith("@raycast/")).map(([name, version]) => `${name}@${version}`).filter(Boolean);
  if (thirdPartyDeps.length === 0) {
    console.log(`No third-party dependencies for ${path3.basename(extPath)}`);
    return;
  }
  console.log(
    `Installing ${thirdPartyDeps.length} dependencies for ${path3.basename(extPath)}: ${thirdPartyDeps.join(", ")}`
  );
  const ok = await installDepsWithBun(extPath);
  if (!ok) {
    throw new Error(`Bun dependency installation failed for ${path3.basename(extPath)}`);
  }
  if (!hasNodeModules(extPath)) {
    throw new Error("Bun completed but node_modules is still missing");
  }
  console.log(`Dependencies installed for ${path3.basename(extPath)}`);
}
function isExtensionInstalled(name) {
  return resolveInstalledExtensionPathForRaymes(name) !== null;
}
function getInstalledExtensionNames() {
  const names = /* @__PURE__ */ new Set();
  const scanRoot = (root, stripRaycastPrefix) => {
    if (!fs3.existsSync(root)) return;
    try {
      for (const d of fs3.readdirSync(root)) {
        const p = path3.join(root, d);
        if (fs3.statSync(p).isDirectory() && fs3.existsSync(path3.join(p, "package.json"))) {
          names.add(stripRaycastPrefix ? slugFromRaymesExtensionId(d) : d);
        }
      }
    } catch {
    }
  };
  scanRoot(getExtensionsDir(), false);
  scanRoot(path3.join(getExtensionsDir(), "packages"), true);
  scanRoot(path3.join(app.getPath("userData"), "extension-registry", "packages"), true);
  return [...names].filter(Boolean).sort((a, b) => a.localeCompare(b));
}
async function installExtension(name) {
  if (!/^[A-Za-z0-9._-]+$/.test(String(name || ""))) {
    console.error(`Invalid extension name: "${name}"`);
    return false;
  }
  emitInstallProgress(name, 5);
  try {
    const success = await installExtensionFromBundle(name);
    if (success) return true;
  } catch (bundleError) {
    console.warn(`Bundle install failed for "${name}":`, bundleError?.message || bundleError);
  }
  emitInstallProgress(name, 45);
  try {
    const success = await installExtensionViaAPI(name);
    if (success) return true;
  } catch (apiError) {
    console.warn(`API install failed for "${name}":`, apiError?.message || apiError);
  }
  return false;
}
async function installExtensionFromBundle(name) {
  const installPath = getInstalledPath(name);
  const hadExistingInstall = fs3.existsSync(installPath);
  const backupPath = hadExistingInstall ? path3.join(getExtensionsDir(), `${name}.backup-${Date.now()}`) : "";
  const tmpDir = path3.join(app.getPath("temp"), `supercmd-bundle-${Date.now()}`);
  try {
    const t0 = Date.now();
    const { url } = await getExtensionBundleUrl(name);
    emitInstallProgress(name, 10);
    console.log(`Downloading pre-built bundle for "${name}"\u2026`);
    fs3.mkdirSync(tmpDir, { recursive: true });
    await downloadAndExtractTarball(url, tmpDir);
    emitInstallProgress(name, 35);
    const nestedPath = path3.join(tmpDir, name);
    let srcDir = tmpDir;
    if (fs3.existsSync(path3.join(nestedPath, "package.json"))) {
      srcDir = nestedPath;
    } else if (!fs3.existsSync(path3.join(srcDir, "package.json"))) {
      const subdirs = fs3.readdirSync(tmpDir, { withFileTypes: true }).filter((d) => d.isDirectory());
      for (const sub of subdirs) {
        if (fs3.existsSync(path3.join(tmpDir, sub.name, "package.json"))) {
          srcDir = path3.join(tmpDir, sub.name);
          break;
        }
      }
    }
    if (!fs3.existsSync(path3.join(srcDir, "package.json"))) {
      throw new Error("Bundle has no package.json");
    }
    if (!fs3.existsSync(path3.join(srcDir, ".sc-build"))) {
      throw new Error("Bundle has no .sc-build/ directory \u2014 not a pre-built bundle");
    }
    const bundleManifest = JSON.parse(fs3.readFileSync(path3.join(srcDir, "package.json"), "utf8"));
    const missingCommandBundles = (Array.isArray(bundleManifest.commands) ? bundleManifest.commands : []).filter((command) => command?.name).filter((command) => !fs3.existsSync(path3.join(srcDir, ".sc-build", `${command.name}.js`))).map((command) => command.name);
    if (missingCommandBundles.length > 0) {
      throw new Error(`Bundle is incomplete; missing commands: ${missingCommandBundles.join(", ")}`);
    }
    if (hadExistingInstall) {
      fs3.renameSync(installPath, backupPath);
    }
    fs3.cpSync(srcDir, installPath, { recursive: true });
    emitInstallProgress(name, 95);
    if (backupPath && fs3.existsSync(backupPath)) {
      fs3.rmSync(backupPath, { recursive: true, force: true });
    }
    reportInstall(name, getMachineId()).catch(() => {
    });
    console.log(`Extension "${name}" installed from pre-built bundle in ${Date.now() - t0}ms`);
    return true;
  } catch (error) {
    try {
      fs3.rmSync(installPath, { recursive: true, force: true });
    } catch {
    }
    if (backupPath && fs3.existsSync(backupPath)) {
      try {
        fs3.renameSync(backupPath, installPath);
      } catch {
      }
    }
    throw error;
  } finally {
    try {
      fs3.rmSync(tmpDir, { recursive: true, force: true });
    } catch {
    }
    if (backupPath && fs3.existsSync(backupPath)) {
      try {
        fs3.rmSync(backupPath, { recursive: true, force: true });
      } catch {
      }
    }
  }
}
async function installExtensionViaAPI(name) {
  const installPath = getInstalledPath(name);
  const hadExistingInstall = fs3.existsSync(installPath);
  const backupPath = hadExistingInstall ? path3.join(getExtensionsDir(), `${name}.backup-${Date.now()}`) : "";
  const tmpDir = path3.join(app.getPath("temp"), `supercmd-api-install-${Date.now()}`);
  try {
    const t0 = Date.now();
    console.log(`Installing extension: ${name}\u2026`);
    fs3.mkdirSync(tmpDir, { recursive: true });
    let srcDir = null;
    try {
      srcDir = await downloadExtensionFromTree(name, tmpDir);
    } catch (error) {
      console.warn(`GitHub API source download failed for "${name}"; trying sparse git.`, error);
    }
    if (!srcDir) srcDir = downloadExtensionViaSparseGit(name, tmpDir);
    console.log(`  Download: ${Date.now() - t0}ms`);
    emitInstallProgress(name, 55);
    if (!srcDir || !fs3.existsSync(path3.join(srcDir, "package.json"))) {
      throw new Error(`Extension "${name}" not found or has no package.json`);
    }
    const srcPkg = JSON.parse(fs3.readFileSync(path3.join(srcDir, "package.json"), "utf-8"));
    if (!isManifestPlatformCompatible(srcPkg)) {
      const supported = getManifestPlatforms(srcPkg);
      console.error(
        `Extension "${name}" is not compatible with ${getCurrentRaycastPlatform()} (supports: ${supported.join(", ")})`
      );
      return false;
    }
    if (hadExistingInstall) {
      fs3.renameSync(installPath, backupPath);
    }
    fs3.cpSync(srcDir, installPath, { recursive: true });
    {
      const extPkg = JSON.parse(fs3.readFileSync(path3.join(installPath, "package.json"), "utf-8"));
      const allDeps = { ...extPkg.dependencies || {}, ...extPkg.optionalDependencies || {} };
      const thirdPartyDeps = Object.keys(allDeps).filter((d) => !d.startsWith("@raycast/"));
      if (thirdPartyDeps.length === 0) {
        console.log(`No third-party dependencies for "${name}" \u2014 skipping install`);
      } else {
        const depsInstalled = await installDependenciesWithProgress(name, installPath);
        if (!depsInstalled) {
          throw new Error(`Could not install dependencies for "${name}"`);
        }
      }
      emitInstallProgress(name, 75);
      const t1 = Date.now();
      console.log(`  Deps: ${t1 - t0}ms. Pre-building commands for "${name}"\u2026`);
      const { buildAllCommands: buildAllCommands2 } = (init_extension_builder(), __toCommonJS(extension_builder_exports));
      const builtCount = await buildAllCommands2(name);
      const expectedCount = (Array.isArray(extPkg.commands) ? extPkg.commands : []).filter(
        (command) => command?.name
      ).length;
      if (builtCount < expectedCount) {
        throw new Error(`Built ${builtCount}/${expectedCount} commands for "${name}"`);
      }
      emitInstallProgress(name, 95);
      console.log(
        `  Build: ${Date.now() - t1}ms. Extension "${name}" installed (${builtCount} commands) in ${Date.now() - t0}ms total`
      );
    }
    if (backupPath && fs3.existsSync(backupPath)) {
      fs3.rmSync(backupPath, { recursive: true, force: true });
    }
    reportInstall(name, getMachineId()).catch(() => {
    });
    return true;
  } catch (error) {
    console.error(`API install failed for "${name}":`, error);
    try {
      fs3.rmSync(installPath, { recursive: true, force: true });
    } catch {
    }
    if (backupPath && fs3.existsSync(backupPath)) {
      try {
        fs3.renameSync(backupPath, installPath);
      } catch {
      }
    }
    throw error;
  } finally {
    try {
      fs3.rmSync(tmpDir, { recursive: true, force: true });
    } catch {
    }
    if (backupPath && fs3.existsSync(backupPath)) {
      try {
        fs3.rmSync(backupPath, { recursive: true, force: true });
      } catch {
      }
    }
  }
}
function findRepositoryExtensionRoot(extractedDir) {
  if (fs3.existsSync(path3.join(extractedDir, "package.json"))) return extractedDir;
  const directories = fs3.readdirSync(extractedDir, { withFileTypes: true }).filter((entry) => entry.isDirectory());
  for (const directory of directories) {
    const candidate = path3.join(extractedDir, directory.name);
    if (fs3.existsSync(path3.join(candidate, "package.json"))) return candidate;
  }
  return null;
}
function extensionSlugFromRepositoryManifest(pkg, fallback) {
  const packageName = String(pkg?.name || "").trim();
  if (/^[A-Za-z0-9._-]+$/.test(packageName)) return packageName;
  const sanitized = String(fallback || "").trim().replace(/[^A-Za-z0-9._-]+/g, "-").replace(/^-+|-+$/g, "");
  if (!sanitized) throw new Error("The repository does not provide a valid extension name");
  return sanitized;
}
function missingPrebuiltCommands(extensionPath, pkg) {
  const commands = Array.isArray(pkg?.commands) ? pkg.commands : [];
  return commands.map((command) => String(command?.name || "").trim()).filter(Boolean).filter((name) => !fs3.existsSync(path3.join(extensionPath, ".sc-build", `${name}.js`)));
}
async function installRegistryExtensionFromGitHubRepository(reference) {
  const metadataUrl = `https://api.github.com/repos/${encodeURIComponent(reference.owner)}/${encodeURIComponent(reference.repository)}`;
  const metadataResponse = await fetchWithTimeout(metadataUrl, { headers: githubApiHeaders() }, 3e4);
  if (!metadataResponse.ok) {
    if (metadataResponse.status === 404) {
      throw new Error(`Repository not found or private: ${reference.url}`);
    }
    throw new Error(`GitHub repository lookup failed (${metadataResponse.status} ${metadataResponse.statusText})`);
  }
  const metadata = await metadataResponse.json();
  const defaultBranch = String(metadata?.default_branch || "").trim();
  if (!defaultBranch) throw new Error("GitHub did not return a default branch for this repository");
  const tmpDir = path3.join(app.getPath("temp"), `tezbar-repository-install-${Date.now()}-${randomHex(8)}`);
  let installPath = "";
  let backupPath = "";
  let installedSlug = "";
  try {
    fs3.mkdirSync(tmpDir, { recursive: true });
    const archiveUrl = `https://codeload.github.com/${encodeURIComponent(reference.owner)}/${encodeURIComponent(reference.repository)}/tar.gz/${encodeURIComponent(defaultBranch)}`;
    await downloadAndExtractTarball(archiveUrl, tmpDir);
    const sourcePath = findRepositoryExtensionRoot(tmpDir);
    if (!sourcePath) {
      throw new Error("This repository has no package.json at its root");
    }
    const pkgPath = path3.join(sourcePath, "package.json");
    const pkg = JSON.parse(fs3.readFileSync(pkgPath, "utf-8"));
    const commands = Array.isArray(pkg?.commands) ? pkg.commands.filter((command) => String(command?.name || "").trim()) : [];
    if (commands.length === 0) {
      throw new Error("This repository is not a Tezbar extension: package.json has no commands");
    }
    if (!isManifestPlatformCompatible(pkg)) {
      const supported = getManifestPlatforms(pkg);
      throw new Error(
        `This extension does not support ${getCurrentRaycastPlatform()}` + (supported.length > 0 ? ` (supports: ${supported.join(", ")})` : "")
      );
    }
    installedSlug = extensionSlugFromRepositoryManifest(pkg, reference.repository);
    installPath = getInstalledPath(installedSlug);
    const hadExistingInstall = fs3.existsSync(installPath);
    backupPath = hadExistingInstall ? path3.join(getExtensionsDir(), `${installedSlug}.backup-${Date.now()}`) : "";
    emitInstallProgress(installedSlug, 20);
    if (hadExistingInstall) fs3.renameSync(installPath, backupPath);
    fs3.cpSync(sourcePath, installPath, { recursive: true });
    emitInstallProgress(installedSlug, 45);
    const installedPkgPath = path3.join(installPath, "package.json");
    const installedPkg = JSON.parse(fs3.readFileSync(installedPkgPath, "utf-8"));
    const missingBundles = missingPrebuiltCommands(installPath, installedPkg);
    if (missingBundles.length > 0) {
      const dependenciesInstalled = await installDepsWithBun(installPath);
      if (!dependenciesInstalled) {
        throw new Error(`Could not install dependencies for ${installedSlug}`);
      }
      emitInstallProgress(installedSlug, 70);
      const { buildAllCommands: buildAllCommands2 } = (init_extension_builder(), __toCommonJS(extension_builder_exports));
      const builtCount = await buildAllCommands2(installedSlug);
      const expectedCount = (Array.isArray(installedPkg.commands) ? installedPkg.commands : []).filter(
        (command) => String(command?.name || "").trim()
      ).length;
      if (builtCount < expectedCount) {
        throw new Error(`Built ${builtCount}/${expectedCount} commands for ${installedSlug}`);
      }
    }
    const stillMissing = missingPrebuiltCommands(installPath, installedPkg);
    if (stillMissing.length > 0) {
      throw new Error(`Extension bundle is missing commands: ${stillMissing.join(", ")}`);
    }
    if (backupPath && fs3.existsSync(backupPath)) {
      fs3.rmSync(backupPath, { recursive: true, force: true });
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
        fs3.rmSync(installPath, { recursive: true, force: true });
      } catch {
      }
    }
    if (backupPath && fs3.existsSync(backupPath)) {
      try {
        fs3.renameSync(backupPath, installPath);
      } catch {
      }
    }
    if (installedSlug) emitInstallProgress(installedSlug, 0);
    throw error;
  } finally {
    try {
      fs3.rmSync(tmpDir, { recursive: true, force: true });
    } catch {
    }
    if (backupPath && fs3.existsSync(backupPath)) {
      try {
        fs3.rmSync(backupPath, { recursive: true, force: true });
      } catch {
      }
    }
  }
}
async function downloadAndExtractTarball(url, destDir) {
  return new Promise((resolve3, reject) => {
    const makeRequest = (requestUrl, redirectCount = 0) => {
      if (redirectCount > 5) {
        reject(new Error("Too many redirects"));
        return;
      }
      const parsedUrl = new URL(requestUrl);
      const isHttps = parsedUrl.protocol === "https:";
      const transport = isHttps ? require("https") : require("http");
      transport.get(
        requestUrl,
        {
          timeout: 12e4,
          headers: {
            "User-Agent": "Tezbar",
            Accept: "application/octet-stream"
          }
        },
        (res) => {
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
          const declaredSize = Number(res.headers["content-length"] || 0);
          if (declaredSize > MAX_EXTENSION_ARCHIVE_BYTES) {
            res.resume();
            reject(new Error("Extension archive is larger than 150 MB"));
            return;
          }
          const chunks = [];
          let receivedSize = 0;
          res.on("data", (chunk) => {
            receivedSize += chunk.length;
            if (receivedSize > MAX_EXTENSION_ARCHIVE_BYTES) {
              res.destroy(new Error("Extension archive is larger than 150 MB"));
              return;
            }
            chunks.push(chunk);
          });
          res.on("error", reject);
          res.on("end", () => {
            try {
              const buffer = Buffer.concat(chunks);
              extractTarGz(buffer, destDir);
              resolve3();
            } catch (err) {
              reject(err);
            }
          });
        }
      ).on("error", reject);
    };
    makeRequest(url);
  });
}
function extractTarGz(buffer, destDir) {
  const decompressed = zlib.gunzipSync(buffer);
  const destinationRoot = path3.resolve(destDir);
  const safeArchivePath = (archivePath) => {
    const normalized = String(archivePath || "").replace(/\\/g, "/");
    if (!normalized || normalized.includes("\0") || normalized.startsWith("/") || /^[A-Za-z]:/.test(normalized)) {
      throw new Error(`Unsafe extension archive path: ${archivePath}`);
    }
    const resolved = path3.resolve(destinationRoot, ...normalized.split("/"));
    if (resolved !== destinationRoot && !resolved.startsWith(`${destinationRoot}${path3.sep}`)) {
      throw new Error(`Unsafe extension archive path: ${archivePath}`);
    }
    return resolved;
  };
  let offset = 0;
  while (offset < decompressed.length - 512) {
    const header = decompressed.subarray(offset, offset + 512);
    if (header.every((b) => b === 0)) break;
    const nameRaw = header.subarray(0, 100).toString("utf-8").replace(/\0+$/, "");
    const sizeOctal = header.subarray(124, 136).toString("utf-8").replace(/\0+$/, "").trim();
    const typeFlag = header[156];
    const prefixRaw = header.subarray(345, 500).toString("utf-8").replace(/\0+$/, "");
    const fullName = prefixRaw ? `${prefixRaw}/${nameRaw}` : nameRaw;
    const size = parseInt(sizeOctal, 8) || 0;
    offset += 512;
    if (typeFlag === 53 || fullName.endsWith("/")) {
      const dirPath = safeArchivePath(fullName);
      fs3.mkdirSync(dirPath, { recursive: true });
    } else if (typeFlag === 0 || typeFlag === 48) {
      const filePath = safeArchivePath(fullName);
      fs3.mkdirSync(path3.dirname(filePath), { recursive: true });
      const fileData = decompressed.subarray(offset, offset + size);
      fs3.writeFileSync(filePath, fileData);
    }
    const dataBlocks = Math.ceil(size / 512);
    offset += dataBlocks * 512;
  }
}
function getMachineId() {
  if (_machineId) return _machineId;
  const idPath = path3.join(app.getPath("userData"), ".machine-id");
  try {
    const existing = fs3.readFileSync(idPath, "utf-8").trim();
    if (existing) {
      _machineId = existing;
      return existing;
    }
  } catch {
  }
  const id = `${randomHex(8)}-${randomHex(4)}-${randomHex(4)}-${randomHex(4)}-${randomHex(12)}`;
  try {
    fs3.writeFileSync(idPath, id);
  } catch {
  }
  _machineId = id;
  return id;
}
function randomHex(length) {
  const bytes = require("crypto").randomBytes(Math.ceil(length / 2));
  return bytes.toString("hex").slice(0, length);
}
async function uninstallExtension(name) {
  const installPath = getInstalledPath(name);
  if (!fs3.existsSync(installPath)) {
    return true;
  }
  try {
    fs3.rmSync(installPath, { recursive: true, force: true });
    console.log(`Extension "${name}" uninstalled.`);
    reportUninstall(name, getMachineId()).catch(() => {
    });
    return true;
  } catch (error) {
    console.error(`Failed to uninstall extension "${name}":`, error);
    return false;
  }
}
function normalizeRaymesExtensionId(input) {
  const slug = String(input || "").trim().replace(/^raycast\./, "");
  return slug ? `raycast.${slug}` : "";
}
function slugFromRaymesExtensionId(input) {
  return String(input || "").trim().replace(/^raycast\./, "");
}
function extensionNameFromSlug(slug) {
  return slug.split(/[-_]/g).filter(Boolean).map((part) => `${part.slice(0, 1).toUpperCase()}${part.slice(1)}`).join(" ");
}
function readInstalledPackage(slug) {
  const extensionPath = resolveInstalledExtensionPathForRaymes(slug);
  const pkgPath = extensionPath ? path3.join(extensionPath, "package.json") : "";
  if (!fs3.existsSync(pkgPath)) return {};
  try {
    return JSON.parse(fs3.readFileSync(pkgPath, "utf-8"));
  } catch {
    return {};
  }
}
function resolvePlatformDefault2(value) {
  const platformKey = process.platform === "win32" ? "Windows" : "macOS";
  if (value && typeof value === "object" && !Array.isArray(value) && (Object.prototype.hasOwnProperty.call(value, "macOS") || Object.prototype.hasOwnProperty.call(value, "Windows"))) {
    if (Object.prototype.hasOwnProperty.call(value, platformKey)) {
      return value[platformKey];
    }
    return value.macOS ?? value.Windows;
  }
  return value;
}
function normalizeRegistryCommand(command) {
  return {
    name: command.cmdName,
    title: command.title,
    subtitle: command.description || command.extensionTitle,
    description: command.description,
    mode: command.mode,
    argumentDefinitions: command.commandArgumentDefinitions
  };
}
function githubAvatarUrlForHandle(value) {
  const raw = typeof value === "object" && value ? String(value.handle || value.name || "") : String(value || "");
  const handle = raw.split("<")[0].split("(")[0].trim().replace(/^@/, "");
  if (!/^[a-z0-9](?:[a-z0-9-]{0,37}[a-z0-9])?$/i.test(handle)) return void 0;
  if (handle.toLowerCase() === "raycast community") return void 0;
  return `https://github.com/${handle}.png?size=96`;
}
function resolveInstalledIconPath(extensionPath, icon) {
  if (typeof icon !== "string" || !icon.trim()) return void 0;
  if (/^https?:\/\//i.test(icon)) return icon;
  const normalized = icon.replace(/^\.?\//, "");
  const candidates = [path3.join(extensionPath, normalized), path3.join(extensionPath, "assets", normalized)];
  return candidates.find((candidate) => fs3.existsSync(candidate));
}
function appPickerValue(name, appPath) {
  if (!fs3.existsSync(appPath)) return null;
  return {
    name,
    path: appPath,
    bundleId: readAppBundleIdentifier(appPath) || ""
  };
}
function resolveAppPickerDefault(pref) {
  const candidates = pref?.name === "uninstaller_app" || pref?.key === "uninstaller_app" ? [
    appPickerValue("AppCleaner", "/Applications/AppCleaner.app"),
    appPickerValue("Pearcleaner", "/Applications/PearCleaner.app"),
    appPickerValue("TrashMe 3", "/Applications/TrashMe 3.app"),
    appPickerValue("App Cleaner 8", "/Applications/App Cleaner 8.app")
  ] : [];
  return candidates.find((candidate) => Boolean(candidate)) || "";
}
function listInstalledRegistryExtensions() {
  const commands = discoverInstalledExtensionCommands();
  const commandsBySlug = /* @__PURE__ */ new Map();
  for (const command of commands) {
    const list = commandsBySlug.get(command.extName) || [];
    list.push(command);
    commandsBySlug.set(command.extName, list);
  }
  return getInstalledExtensionNames().map((slug) => {
    const pkg = readInstalledPackage(slug);
    const extensionPath = resolveInstalledExtensionPathForRaymes(slug) || getInstalledPath(slug);
    const id = normalizeRaymesExtensionId(slug);
    const authorRaw = pkg.author || pkg.owner || "";
    const author = typeof authorRaw === "object" ? String(authorRaw?.name || authorRaw?.handle || "") : String(authorRaw || "");
    const ownerRaw = pkg.owner || pkg.author || "";
    const owner = typeof ownerRaw === "object" ? String(ownerRaw?.handle || ownerRaw?.name || "") : String(ownerRaw || "");
    const authorIconUrl = githubAvatarUrlForHandle(authorRaw);
    const iconPath = resolveInstalledIconPath(extensionPath, pkg.icon || "icon.png");
    return {
      id,
      slug,
      name: String(pkg.title || extensionNameFromSlug(slug)),
      version: String(pkg.version || "1.0.0"),
      description: String(pkg.description || ""),
      author: author || void 0,
      owner: owner || void 0,
      authorIconUrl,
      iconPath,
      packageJsonPath: path3.join(extensionPath, "package.json"),
      extensionPath,
      commands: (commandsBySlug.get(slug) || []).map(normalizeRegistryCommand),
      installedAt: (() => {
        try {
          return fs3.statSync(extensionPath).mtimeMs;
        } catch {
          return Date.now();
        }
      })()
    };
  }).sort((a, b) => a.name.localeCompare(b.name));
}
function resolveInstalledPackageJsonPath(extensionId) {
  const slug = slugFromRaymesExtensionId(extensionId);
  if (!slug) return null;
  const extensionPath = resolveInstalledExtensionPathForRaymes(slug);
  const pkgPath = extensionPath ? path3.join(extensionPath, "package.json") : "";
  return fs3.existsSync(pkgPath) ? pkgPath : null;
}
function searchTokens(value) {
  return String(value || "").toLowerCase().split(/[^a-z0-9]+/g).filter(Boolean);
}
function tokenScore(tokens, query, exactScore, prefixScore) {
  if (tokens.some((token) => token === query)) return exactScore;
  if (tokens.some((token) => token.startsWith(query))) return prefixScore;
  return 0;
}
function scoreCatalogEntrySearch(entry, query) {
  const q = String(query || "").trim().toLowerCase();
  if (!q) return 1;
  const name = String(entry.name || "").toLowerCase();
  const title = String(entry.title || "").toLowerCase();
  const author = String(entry.author || "").toLowerCase();
  const categories = entry.categories || [];
  const commands = entry.commands || [];
  const titleTokens = searchTokens(entry.title);
  const nameTokens = searchTokens(entry.name);
  const authorTokens = searchTokens(author);
  const categoryTokens = searchTokens(categories.join(" "));
  const commandTokens = searchTokens(commands.map((command) => `${command.name} ${command.title}`).join(" "));
  const descriptionTokens = searchTokens(entry.description);
  let score = 0;
  if (name === q || title === q) score += 1e4;
  if (name.startsWith(q) || title.startsWith(q)) score += 9e3;
  score += tokenScore(nameTokens, q, 8200, 7200);
  score += tokenScore(titleTokens, q, 8e3, 7e3);
  score += tokenScore(categoryTokens, q, 3e3, 2200);
  score += tokenScore(authorTokens, q, 2200, 1600);
  score += tokenScore(commandTokens, q, 1800, 1200);
  score += tokenScore(descriptionTokens, q, 900, 550);
  if (q.length >= 4) {
    if (name.includes(q) || title.includes(q)) score += 1400;
    if (entry.description.toLowerCase().includes(q)) score += 350;
  }
  if (score > 0 && entry.installCount && entry.installCount > 0) {
    score += Math.min(500, Math.log10(entry.installCount) * 80);
  }
  return score;
}
async function searchExtensionCatalog(query, options) {
  const offset = Math.max(0, options?.offset ?? 0);
  const limit = Math.max(1, options?.limit ?? 25);
  if (process.platform === "win32") {
    const loved = searchLovedExtensions(query);
    const items2 = loved.slice(offset, offset + limit);
    return {
      items: items2,
      total: loved.length,
      offset,
      limit,
      hasMore: offset + items2.length < loved.length
    };
  }
  const q = String(query || "").trim().toLowerCase();
  const catalog = await getCatalog(false);
  const matched = catalog.map((entry) => {
    return { entry, score: scoreCatalogEntrySearch(entry, q) };
  }).filter(({ score }) => score > 0).sort((a, b) => {
    return b.score - a.score || (b.entry.installCount ?? 0) - (a.entry.installCount ?? 0) || a.entry.title.localeCompare(b.entry.title);
  });
  const total = matched.length;
  const pageEntries = matched.slice(offset, offset + limit);
  const items = pageEntries.map(({ entry }) => ({
    id: normalizeRaymesExtensionId(entry.name),
    name: entry.title || extensionNameFromSlug(entry.name),
    description: entry.description || "",
    author: entry.author || entry.contributors?.[0] || "Raycast Community",
    version: "latest",
    repository: `https://github.com/raycast/extensions/tree/main/extensions/${entry.name}`,
    downloadCount: entry.installCount,
    icon: entry.icon,
    iconUrl: entry.iconUrl,
    authorIconUrl: githubAvatarUrlForHandle(entry.author || entry.contributors?.[0]),
    screenshotUrls: entry.screenshotUrls,
    categories: entry.categories,
    commands: entry.commands,
    owner: entry.author || void 0
  }));
  return {
    items,
    total,
    offset,
    limit,
    hasMore: offset + items.length < total
  };
}
async function installRegistryExtension(extensionIdOrSlug) {
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
  if (!slug) throw new Error("A valid extension id is required");
  const existingJob = extensionInstallJobs.get(slug);
  if (existingJob) return existingJob;
  const job = (async () => {
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
function uninstallRegistryExtension(extensionIdOrSlug) {
  const slug = slugFromRaymesExtensionId(extensionIdOrSlug);
  if (!slug) return false;
  let removed = false;
  for (const candidate of [
    getInstalledPath(slug),
    getLegacyInstalledPath(slug),
    getLegacyRegistryInstalledPath(slug)
  ]) {
    if (fs3.existsSync(candidate)) {
      fs3.rmSync(candidate, { recursive: true, force: true });
      removed = true;
    }
  }
  return removed || true;
}
function listInstalledExtensionSlugsFromDisk() {
  return getInstalledExtensionNames();
}
function getExtensionPreferences(extensionId, commandName) {
  const slug = slugFromRaymesExtensionId(extensionId);
  const pkg = readInstalledPackage(slug);
  const values = {};
  const applyDefaults = (preferences) => {
    for (const pref of preferences || []) {
      if (!pref?.name) continue;
      const resolvedDefault = resolvePlatformDefault2(pref.default);
      if (resolvedDefault !== void 0) {
        values[pref.name] = resolvedDefault;
      } else if (pref.type === "checkbox") {
        values[pref.name] = false;
      } else if (pref.type === "dropdown") {
        values[pref.name] = pref.data?.[0]?.value ?? "";
      } else if (pref.type === "appPicker") {
        values[pref.name] = resolveAppPickerDefault(pref);
      } else {
        values[pref.name] = "";
      }
    }
  };
  applyDefaults(Array.isArray(pkg.preferences) ? pkg.preferences : []);
  const command = Array.isArray(pkg.commands) ? pkg.commands.find((cmd) => cmd?.name === commandName) : null;
  applyDefaults(Array.isArray(command?.preferences) ? command.preferences : []);
  const extensionPath = resolveInstalledExtensionPathForRaymes(slug) || getInstalledPath(slug);
  const preferencesPath = path3.join(extensionPath, "preferences.json");
  if (fs3.existsSync(preferencesPath)) {
    try {
      const saved = JSON.parse(fs3.readFileSync(preferencesPath, "utf-8"));
      if (saved && typeof saved === "object") {
        Object.assign(values, saved);
        if (commandName && saved.commands?.[commandName]) {
          Object.assign(values, saved.commands[commandName]);
        }
      }
    } catch {
    }
  }
  return values;
}
function getExtensionPreferenceSetup(extensionId, commandName) {
  const slug = slugFromRaymesExtensionId(extensionId);
  const pkg = readInstalledPackage(slug);
  const extensionPath = resolveInstalledExtensionPathForRaymes(slug) || getInstalledPath(slug);
  const command = Array.isArray(pkg.commands) ? pkg.commands.find((cmd) => cmd?.name === commandName) : null;
  const extensionPreferences = Array.isArray(pkg.preferences) ? pkg.preferences.map((preference) => ({ ...preference })) : [];
  const commandPreferences = Array.isArray(command?.preferences) ? command.preferences.map((preference) => ({
    ...preference,
    commandName,
    commandTitle: String(command?.title || commandName)
  })) : [];
  const preferences = [...extensionPreferences, ...commandPreferences];
  const preferencesPath = path3.join(extensionPath, "preferences.json");
  return {
    extensionId: normalizeRaymesExtensionId(slug),
    commandName,
    title: String(pkg.title || extensionNameFromSlug(slug)),
    iconPath: resolveInstalledIconPath(extensionPath, pkg.icon || "icon.png") || void 0,
    preferences,
    values: getExtensionPreferences(extensionId, commandName),
    hasSavedPreferences: fs3.existsSync(preferencesPath)
  };
}
function shouldShowExtensionPreferenceSetup(extensionId, commandName) {
  const setup = getExtensionPreferenceSetup(extensionId, commandName);
  if (setup.preferences.length === 0) return false;
  const needsRequiredValue = setup.preferences.some((pref) => {
    if (!pref?.required || !pref?.name) return false;
    const value = setup.values[pref.name];
    return value === void 0 || value === null || String(value).trim() === "";
  });
  if (needsRequiredValue) return true;
  const needsCredentialValue = setup.preferences.some((pref) => {
    if (pref?.type !== "password" || !pref?.name) return false;
    const value = setup.values[pref.name];
    return value === void 0 || value === null || String(value).trim() === "";
  });
  if (needsCredentialValue) return true;
  if (!setup.hasSavedPreferences && setup.preferences.some((pref) => pref?.required)) {
    return true;
  }
  return !setup.hasSavedPreferences && extensionId === "raycast.google-translate";
}
function saveExtensionPreferences(extensionId, values, commandName) {
  const slug = slugFromRaymesExtensionId(extensionId);
  const extensionPath = resolveInstalledExtensionPathForRaymes(slug) || getInstalledPath(slug);
  fs3.mkdirSync(extensionPath, { recursive: true });
  const preferencesPath = path3.join(extensionPath, "preferences.json");
  let existing = {};
  if (fs3.existsSync(preferencesPath)) {
    try {
      const parsed = JSON.parse(fs3.readFileSync(preferencesPath, "utf-8"));
      if (parsed && typeof parsed === "object") existing = parsed;
    } catch {
    }
  }
  if (commandName) {
    existing.commands = existing.commands && typeof existing.commands === "object" ? existing.commands : {};
    existing.commands[commandName] = {
      ...existing.commands[commandName] || {},
      ...values
    };
  } else {
    existing = {
      ...existing,
      ...values
    };
  }
  fs3.writeFileSync(preferencesPath, JSON.stringify(existing, null, 2));
  return getExtensionPreferences(extensionId, commandName);
}
var import_child_process2, import_events, fs3, path3, zlib, extensionRegistryEvents, extensionInstallJobs, GITHUB_RAW, GITHUB_API, GITHUB_TREE_API, RAYMES_EXTENSIONS_GIT, REPO_TREE_TTL_MS, repoTreeCache, MAX_EXTENSION_ARCHIVE_BYTES, CATALOG_VERSION, CATALOG_TTL, catalogCache, catalogFetchPromise, _machineId;
var init_extension_registry = __esm({
  "src/main/extension-registry.ts"() {
    "use strict";
    init_desktop();
    import_child_process2 = require("child_process");
    import_events = require("events");
    fs3 = __toESM(require("fs"));
    path3 = __toESM(require("path"));
    zlib = __toESM(require("zlib"));
    init_extension_platform();
    init_extension_builder();
    init_extension_api();
    init_bun_manager();
    init_lovedExtensions();
    init_extensionRepository();
    init_services();
    extensionRegistryEvents = new import_events.EventEmitter();
    extensionInstallJobs = /* @__PURE__ */ new Map();
    GITHUB_RAW = "https://raw.githubusercontent.com/raycast/extensions/main";
    GITHUB_API = "https://api.github.com/repos/raycast/extensions/contents";
    GITHUB_TREE_API = "https://api.github.com/repos/raycast/extensions/git/trees/main?recursive=1";
    RAYMES_EXTENSIONS_GIT = "https://github.com/almatkai/raymes-extensions.git";
    REPO_TREE_TTL_MS = 10 * 60 * 1e3;
    repoTreeCache = null;
    MAX_EXTENSION_ARCHIVE_BYTES = 150 * 1024 * 1024;
    CATALOG_VERSION = 6;
    CATALOG_TTL = 24 * 60 * 60 * 1e3;
    catalogCache = null;
    catalogFetchPromise = null;
    _machineId = null;
  }
});

// src/main/extension-runner.ts
var extension_runner_exports = {};
__export(extension_runner_exports, {
  clearAllExtensionSessions: () => clearAllExtensionSessions,
  disposeExtensionSession: () => disposeExtensionSession,
  instrumentTimerNotificationCommand: () => instrumentTimerNotificationCommand,
  invokeExtensionAction: () => invokeExtensionAction,
  loadMoreExtensionSession: () => loadMoreExtensionSession,
  refreshExtensionSession: () => refreshExtensionSession,
  runExtensionCommand: () => runExtensionCommand,
  runExtensionCommandFromPackageJson: () => runExtensionCommandFromPackageJson,
  updateSearchText: () => updateSearchText
});
function instrumentTimerNotificationCommand(command) {
  const timerFile = /if \[ -f "([^"]+\.timer)" \]; then/.exec(command)?.[1];
  const notification = /osascript -e 'display notification "Timer \\"(.*?)\\" complete" with title "Ding!"'/.exec(
    command
  );
  if (!timerFile || !notification) return command;
  const payload = { timerFile, name: notification[1] };
  const marker = `${TIMER_NOTIFICATION_MARKER}${Buffer.from(JSON.stringify(payload)).toString("base64url")}`;
  return command.replace(notification[0], `printf '%s\\n' '${marker}'`);
}
function timerNotificationPayload(line) {
  const markerIndex = line.indexOf(TIMER_NOTIFICATION_MARKER);
  if (markerIndex < 0) return null;
  const encoded = line.slice(markerIndex + TIMER_NOTIFICATION_MARKER.length).trim();
  try {
    const parsed = JSON.parse(Buffer.from(encoded, "base64url").toString("utf8"));
    if (typeof parsed.timerFile !== "string" || typeof parsed.name !== "string") return null;
    return { timerFile: parsed.timerFile, name: parsed.name };
  } catch {
    return null;
  }
}
function stripTimerNotificationMarkers(value) {
  const stringValue = Buffer.isBuffer(value) ? value.toString("utf8") : value;
  const filtered = stringValue.split(/\r?\n/).filter((line) => !line.includes(TIMER_NOTIFICATION_MARKER)).join("\n");
  return Buffer.isBuffer(value) ? Buffer.from(filtered) : filtered;
}
function setPromiseResultMemoryCache(key, value) {
  if (promiseResultMemoryCache.size >= PROMISE_RESULT_MEMORY_CACHE_LIMIT && !promiseResultMemoryCache.has(key)) {
    const firstKey = promiseResultMemoryCache.keys().next().value;
    if (firstKey !== void 0) {
      promiseResultMemoryCache.delete(firstKey);
    }
  }
  promiseResultMemoryCache.delete(key);
  promiseResultMemoryCache.set(key, value);
}
function makeId(prefix) {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}
function delay(ms) {
  return new Promise((resolve3) => setTimeout(resolve3, ms));
}
function elapsedMs(startedAt) {
  return `${Date.now() - startedAt}ms`;
}
function hookDepsEqual(previous, next) {
  if (!previous || !next || previous.length !== next.length) return false;
  return next.every((value, index) => Object.is(value, previous[index]));
}
function flushPendingEffects(session) {
  const effects = session.pendingEffects.splice(0);
  for (const { idx, sideEffect, deps, label } of effects) {
    const previousCleanup = session.effectCleanups.get(idx);
    if (previousCleanup) {
      try {
        previousCleanup();
      } catch (error) {
        console.error(`[${label}] cleanup threw:`, error);
      }
      session.effectCleanups.delete(idx);
    }
    try {
      const cleanup = sideEffect();
      session.effectDeps.set(idx, deps);
      if (typeof cleanup === "function") session.effectCleanups.set(idx, cleanup);
    } catch (error) {
      console.error(`[${label}] side effect threw:`, error);
    }
  }
}
function promiseHookLabel(hookIdx, fn, args) {
  const source = typeof fn === "function" ? fn.toString().replace(/\s+/g, " ").slice(0, 90) : String(fn).slice(0, 90);
  let serializedArgs = "";
  try {
    serializedArgs = JSON.stringify(args);
  } catch {
    serializedArgs = "[unserializable]";
  }
  return `hook=${hookIdx} fn="${source}" args=${serializedArgs.slice(0, 160)}`;
}
function promiseResultCachePath(session, key) {
  const digest = (0, import_node_crypto2.createHash)("sha256").update(session.bundledCode).update("\0").update(session.extensionId).update("\0").update(session.commandName).update("\0").update(key).digest("hex");
  return (0, import_node_path4.join)(session.packageRoot, ".tezbar-runtime-cache", `${digest}.bin.gz`);
}
function readPromiseResultCache(session, key) {
  const memoryKey = `${session.extensionId}/${session.commandName}:${key}`;
  const memoryEntry = promiseResultMemoryCache.get(memoryKey);
  if (memoryEntry && Date.now() - memoryEntry.cachedAt <= PROMISE_RESULT_CACHE_TTL_MS) {
    return memoryEntry;
  }
  const cachePath = promiseResultCachePath(session, key);
  try {
    const stats = (0, import_node_fs3.statSync)(cachePath);
    if (Date.now() - stats.mtimeMs > PROMISE_RESULT_CACHE_TTL_MS) return null;
    const compressed = (0, import_node_fs3.readFileSync)(cachePath);
    const payload = (0, import_node_v8.deserialize)((0, import_node_zlib.gunzipSync)(compressed));
    setPromiseResultMemoryCache(memoryKey, payload);
    console.log(
      `[usePromise] Persistent cache hit ${session.extensionId}/${session.commandName}; bytes=${compressed.byteLength}`
    );
    return payload;
  } catch {
    return null;
  }
}
function writePromiseResultCache(session, key, data) {
  if (data === void 0 || session.disposed) return;
  const cachedAt = Date.now();
  const memoryKey = `${session.extensionId}/${session.commandName}:${key}`;
  setPromiseResultMemoryCache(memoryKey, { data, cachedAt });
  const cachePath = promiseResultCachePath(session, key);
  void (async () => {
    const startedAt = Date.now();
    try {
      const encoded = (0, import_node_v8.serialize)({ data, cachedAt });
      const compressed = await gzipAsync(encoded);
      (0, import_node_fs3.mkdirSync)((0, import_node_path4.dirname)(cachePath), { recursive: true });
      await (0, import_promises.writeFile)(cachePath, compressed);
      console.log(
        `[usePromise] Persistent cache write complete after ${elapsedMs(startedAt)}; raw=${encoded.byteLength}, compressed=${compressed.byteLength}`
      );
    } catch (error) {
      console.warn(
        "[usePromise] Persistent cache write failed:",
        error instanceof Error ? error.message : String(error)
      );
    }
  })();
}
function createLoggedFetch() {
  return async (input, init) => {
    const method = String(
      init?.method ?? (input instanceof Request ? input.method : "GET")
    ).toUpperCase();
    const url = input instanceof Request ? input.url : String(input);
    const startedAt = Date.now();
    console.log(`[ExtensionFetch] start ${method} ${url}`);
    try {
      const response = await fetch(input, init);
      const contentLength = response.headers.get("content-length") ?? "unknown";
      console.log(
        `[ExtensionFetch] headers ${method} ${url} after ${elapsedMs(startedAt)}; status=${response.status}, length=${contentLength}`
      );
      if (!response.body || method === "HEAD") {
        console.log(`[ExtensionFetch] complete ${method} ${url} after ${elapsedMs(startedAt)}`);
        return response;
      }
      let bytes = 0;
      let lastLoggedAt = Date.now();
      let lastLoggedBytes = 0;
      const monitor = new TransformStream({
        transform(chunk, controller) {
          bytes += chunk.byteLength;
          const now = Date.now();
          if (now - lastLoggedAt >= 2e3 || bytes - lastLoggedBytes >= 5 * 1024 * 1024) {
            console.log(
              `[ExtensionFetch] progress ${method} ${url}; bytes=${bytes}, elapsed=${elapsedMs(startedAt)}`
            );
            lastLoggedAt = now;
            lastLoggedBytes = bytes;
          }
          controller.enqueue(chunk);
        },
        flush() {
          console.log(
            `[ExtensionFetch] body complete ${method} ${url}; bytes=${bytes}, elapsed=${elapsedMs(startedAt)}`
          );
        }
      });
      return new Response(response.body.pipeThrough(monitor), {
        status: response.status,
        statusText: response.statusText,
        headers: response.headers
      });
    } catch (error) {
      console.error(
        `[ExtensionFetch] failed ${method} ${url} after ${elapsedMs(startedAt)}:`,
        error instanceof Error ? error.message : String(error)
      );
      throw error;
    }
  };
}
async function recoverIncompleteChunkedCache(session, error, promiseKey) {
  if (!(error instanceof Error) || session.cacheRecoveryKeys.has(promiseKey)) return false;
  const missingIndex = error.message.match(
    /ENOENT:.*open ['"]([^'"]+)[/\\]([^/\\]+)[/\\]index\.json['"]/
  );
  if (!missingIndex) return false;
  const indexPath = missingIndex[1];
  const cacheName = missingIndex[2];
  if (!indexPath || !cacheName) return false;
  const supportRoot = (0, import_node_path4.join)(session.packageRoot, ".tezbar-support");
  const chunkDirectory = (0, import_node_path4.join)(indexPath, cacheName);
  const sourcePath = (0, import_node_path4.join)(indexPath, `${cacheName}.json`);
  if ((0, import_node_path4.dirname)(sourcePath) !== supportRoot || (0, import_node_path4.dirname)(chunkDirectory) !== supportRoot) {
    return false;
  }
  let handle = null;
  try {
    handle = await (0, import_promises.open)(sourcePath, "r");
    const stats = await handle.stat();
    if (stats.size <= 0) return false;
    const tailSize = Math.min(stats.size, 4096);
    const tail = Buffer.alloc(tailSize);
    await handle.read(tail, 0, tailSize, stats.size - tailSize);
    const finalCharacter = tail.toString("utf8").trimEnd().at(-1);
    if (finalCharacter === "]" || finalCharacter === "}") return false;
  } catch {
    return false;
  } finally {
    await handle?.close().catch(() => {
    });
  }
  session.cacheRecoveryKeys.add(promiseKey);
  await Promise.all([
    (0, import_promises.rm)(sourcePath, { force: true }),
    (0, import_promises.rm)(chunkDirectory, { recursive: true, force: true })
  ]);
  console.warn(
    `[Runner] Removed incomplete extension cache "${cacheName}" and scheduled one rebuild.`
  );
  return true;
}
function createFetchModuleShim() {
  const boundFetch = fetch.bind(globalThis);
  boundFetch.default = boundFetch;
  boundFetch.fetch = boundFetch;
  boundFetch.Headers = globalThis.Headers;
  boundFetch.Request = globalThis.Request;
  boundFetch.Response = globalThis.Response;
  boundFetch.__esModule = true;
  return boundFetch;
}
async function runAppleScript(source) {
  if (process.platform !== "darwin") {
    throw new Error("AppleScript is only available on macOS");
  }
  if (typeof source !== "string" || source.trim().length === 0) {
    return "";
  }
  const { stdout } = await execFileAsync3("/usr/bin/osascript", ["-e", source], {
    encoding: "utf8",
    maxBuffer: 10 * 1024 * 1024
  });
  return String(stdout).replace(/\r?\n$/, "");
}
async function runAppleScriptForSession(session, source) {
  pushEffect(session, { kind: "apple-script", value: String(source ?? "").slice(0, 2e3) });
  if (session.effectMode === "record") return "";
  return runAppleScript(source);
}
async function runPowerShellScriptForSession(session, source, options) {
  pushEffect(session, { kind: "powershell-script", value: String(source ?? "").slice(0, 2e3) });
  if (session.effectMode === "record") return "";
  return runPowerShellScript(source, options);
}
function nativeColorPickerBundledHelperPath() {
  const envPath = process.env.COLOR_PICKER_HELPER_PATH;
  if (envPath && (0, import_node_fs3.existsSync)(envPath)) return envPath;
  const helperName = process.platform === "win32" ? "windows.ps1" : "color-picker-helper";
  const candidates = [
    (0, import_node_path4.join)(process.cwd(), "native", "color-picker", helperName),
    (0, import_node_path4.join)(app.getAppPath(), "native", "color-picker", helperName)
  ];
  if (app?.isPackaged) {
    const resourcesPath = process.resourcesPath;
    if (resourcesPath) {
      candidates.unshift(
        (0, import_node_path4.join)(resourcesPath, "app.asar.unpacked", "native", "color-picker", helperName),
        (0, import_node_path4.join)(resourcesPath, "native", "color-picker", helperName)
      );
    }
  }
  return candidates.find((candidate) => (0, import_node_fs3.existsSync)(candidate)) ?? null;
}
function nativeColorPickerCachedBinaryPath() {
  return (0, import_node_path4.join)(app.getPath("userData"), "native", "color-picker");
}
function nativeColorPickerSourcePath() {
  const candidates = [
    (0, import_node_path4.join)(process.cwd(), "native", "color-picker", "main.swift"),
    (0, import_node_path4.join)(process.cwd(), "src", "native", "color-picker.swift"),
    (0, import_node_path4.join)(app.getAppPath(), "native", "color-picker", "main.swift"),
    (0, import_node_path4.join)(app.getAppPath(), "src", "native", "color-picker.swift")
  ];
  return candidates.find((candidate) => (0, import_node_fs3.existsSync)(candidate)) ?? null;
}
async function ensureNativeColorPickerHelper() {
  const bundledPath = nativeColorPickerBundledHelperPath();
  if (bundledPath) return bundledPath;
  if (process.platform !== "darwin") return null;
  const sourcePath = nativeColorPickerSourcePath();
  if (!sourcePath) return null;
  const binaryPath = nativeColorPickerCachedBinaryPath();
  if ((0, import_node_fs3.existsSync)(binaryPath)) {
    try {
      if ((0, import_node_fs3.statSync)(binaryPath).mtimeMs >= (0, import_node_fs3.statSync)(sourcePath).mtimeMs) return binaryPath;
    } catch {
      return binaryPath;
    }
  }
  const moduleCachePath = (0, import_node_path4.join)((0, import_node_path4.dirname)(binaryPath), "swift-module-cache");
  (0, import_node_fs3.mkdirSync)((0, import_node_path4.dirname)(binaryPath), { recursive: true });
  (0, import_node_fs3.mkdirSync)(moduleCachePath, { recursive: true });
  try {
    await execFileAsync3("/usr/bin/swiftc", [
      "-module-cache-path",
      moduleCachePath,
      "-O",
      "-o",
      binaryPath,
      sourcePath,
      "-framework",
      "AppKit"
    ]);
    return (0, import_node_fs3.existsSync)(binaryPath) ? binaryPath : null;
  } catch (error) {
    console.error("[ColorPicker] Failed to compile native helper:", error);
    return null;
  }
}
async function runNativeColorPickerHelper(helperPath) {
  if (process.platform === "win32") {
    const { stdout: stdout2 } = await execFileAsync3(
      "powershell.exe",
      [
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Sta",
        "-File",
        helperPath
      ],
      {
        encoding: "utf8",
        windowsHide: true,
        maxBuffer: 1024 * 1024
      }
    );
    return stdout2;
  }
  const { stdout } = await execFileAsync3(helperPath, [], {
    encoding: "utf8",
    maxBuffer: 1024 * 1024
  });
  return stdout;
}
async function pickColorWithNativeSampler() {
  const visibleWindows = BrowserWindow.getAllWindows().filter((window2) => window2.isVisible());
  try {
    const helperPath = await ensureNativeColorPickerHelper();
    if (!helperPath) {
      console.error(`[ColorPicker] No native picker is available on ${process.platform}.`);
      return null;
    }
    setSuppressBlurHide(true);
    for (const window2 of visibleWindows) {
      window2.hide();
    }
    app.hide();
    await delay(process.platform === "win32" ? 160 : 80);
    const stdout = await runNativeColorPickerHelper(helperPath);
    const trimmed = stdout.trim();
    if (!trimmed || trimmed === "null") return null;
    const parsed = JSON.parse(trimmed);
    const toUnitRange = (value) => {
      const numeric = Number(value);
      if (!Number.isFinite(numeric)) return null;
      if (numeric > 1) return Math.max(0, Math.min(1, numeric / 255));
      return Math.max(0, Math.min(1, numeric));
    };
    const red = toUnitRange(parsed.red);
    const green = toUnitRange(parsed.green);
    const blue = toUnitRange(parsed.blue);
    const alpha = toUnitRange(parsed.alpha ?? 1);
    if (red === null || green === null || blue === null || alpha === null) return null;
    return {
      red,
      green,
      blue,
      alpha,
      colorSpace: typeof parsed.colorSpace === "string" && parsed.colorSpace.trim() ? parsed.colorSpace : "srgb"
    };
  } catch (error) {
    console.error("[ColorPicker] Native sampler failed:", error);
    return null;
  } finally {
    setSuppressBlurHide(false);
    app.show();
    for (const window2 of visibleWindows) {
      if (!window2.isDestroyed()) {
        window2.show();
        window2.focus();
      }
    }
  }
}
function imageColorsHelperPath() {
  const envPath = process.env.IMAGE_COLORS_HELPER_PATH;
  if (envPath && (0, import_node_fs3.existsSync)(envPath)) return envPath;
  const helperName = process.platform === "win32" ? "windows.ps1" : "image-colors-helper";
  const candidates = [
    (0, import_node_path4.join)(process.cwd(), "native", "image-colors", helperName),
    (0, import_node_path4.join)(app.getAppPath(), "native", "image-colors", helperName)
  ];
  if (app?.isPackaged) {
    const resourcesPath = process.resourcesPath;
    if (resourcesPath) {
      candidates.unshift(
        (0, import_node_path4.join)(resourcesPath, "app.asar.unpacked", "native", "image-colors", helperName),
        (0, import_node_path4.join)(resourcesPath, "native", "image-colors", helperName)
      );
    }
  }
  return candidates.find((candidate) => (0, import_node_fs3.existsSync)(candidate)) ?? null;
}
async function extractColorsFromImage(path4, colorCount = 40, dominantOnly = false) {
  const isWindows = process.platform === "win32";
  if (process.platform !== "darwin" && !isWindows) {
    throw new Error("Native image color extraction is not available on this platform");
  }
  if (!path4 || !(0, import_node_fs3.existsSync)(path4)) throw new Error("The selected image no longer exists");
  const helperPath = imageColorsHelperPath();
  if (!helperPath) throw new Error("The native image color helper is missing");
  const count = Math.max(1, Math.min(80, Math.round(Number(colorCount) || 40)));
  const usePowerShell = isWindows && (0, import_node_path4.extname)(helperPath).toLowerCase() === ".ps1";
  const command = usePowerShell ? "powershell.exe" : helperPath;
  const args = usePowerShell ? [
    "-NoLogo",
    "-NoProfile",
    "-NonInteractive",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
    helperPath,
    path4,
    String(count),
    dominantOnly ? "true" : "false"
  ] : [path4, String(count), dominantOnly ? "true" : "false"];
  const { stdout } = await execFileAsync3(command, args, {
    timeout: 6e4,
    maxBuffer: 10 * 1024 * 1024,
    ...isWindows ? { windowsHide: true } : {}
  });
  const parsed = JSON.parse(stdout.trim());
  if (!Array.isArray(parsed)) throw new Error("The native image color helper returned invalid data");
  return parsed;
}
function screenOcrHelperPath() {
  if (process.env.SCREENOCR_HELPER_PATH) return process.env.SCREENOCR_HELPER_PATH;
  if (app?.isPackaged) {
    const resourcesPath = process.resourcesPath;
    if (resourcesPath) {
      return (0, import_node_path4.join)(resourcesPath, "app.asar.unpacked", "native", "screenocr", "screenocr-helper");
    }
  }
  return (0, import_node_path4.join)(process.cwd(), "native", "screenocr", "screenocr-helper");
}
async function runScreenOcrHelper(command, values) {
  const helperPath = screenOcrHelperPath();
  if (!(0, import_node_fs3.existsSync)(helperPath)) {
    throw new Error(`ScreenOCR native helper is missing at ${helperPath}`);
  }
  const visibleWindows = BrowserWindow?.getAllWindows ? BrowserWindow.getAllWindows().filter((window2) => window2.isVisible()) : [];
  const shouldHideApp = command === "recognize-text" && values.fullscreen === true;
  try {
    if (shouldHideApp) {
      setSuppressBlurHide(true);
      for (const window2 of visibleWindows) window2.hide();
      app?.hide?.();
      await delay(120);
    }
    const { stdout } = await execFileAsync3(helperPath, [command, JSON.stringify(values)], {
      timeout: 18e4,
      maxBuffer: 10 * 1024 * 1024
    });
    const response = JSON.parse(stdout.trim());
    if (!response.ok) throw new Error(response.error || "ScreenOCR native helper failed");
    return response.value ?? "";
  } finally {
    if (shouldHideApp) {
      setSuppressBlurHide(false);
      app?.show?.();
      for (const window2 of visibleWindows) {
        if (!window2.isDestroyed()) window2.show();
      }
    }
  }
}
function colorWheelMarkdown() {
  return "![RGB Color Wheel](rgb-color-wheel.webp?&raycast-height=350)";
}
function attachRuntimeRootMetadata(root, session) {
  root.props = {
    ...root.props ?? {},
    assetsPath: (0, import_node_path4.join)(session.packageRoot, "assets")
  };
  if (typeof root.props.markdown === "string") {
    root.props.markdown = resolveExtensionMarkdownAssets(root.props.markdown, session.packageRoot);
  }
}
function buildPreferenceSetupRoot(extensionId, commandName) {
  const setup = getExtensionPreferenceSetup(extensionId, commandName);
  return {
    type: "Tezbar.PreferenceSetup",
    props: {
      extensionId: setup.extensionId,
      commandName,
      title: setup.title,
      iconPath: setup.iconPath,
      preferences: setup.preferences,
      values: setup.values,
      includeApiKey: extensionId === "raycast.google-translate"
    },
    children: []
  };
}
function parsePackageJson(path4) {
  if (!(0, import_node_fs3.existsSync)(path4)) {
    throw new Error(`Missing package.json at ${path4}`);
  }
  const raw = (0, import_node_fs3.readFileSync)(path4, "utf8");
  const parsed = JSON.parse(raw);
  return parsed && typeof parsed === "object" ? parsed : {};
}
function findCommandInManifest(pkg, commandName) {
  const command = (pkg.commands ?? []).find((entry) => entry.name === commandName);
  if (!command) {
    throw new Error(`Command not found: ${commandName}`);
  }
  return command;
}
function resolveCommandEntry(packageRoot, commandName, command) {
  const prebuilt = (0, import_node_path4.join)(packageRoot, ".sc-build", `${commandName}.js`);
  if ((0, import_node_fs3.existsSync)(prebuilt)) return prebuilt;
  const explicit = [command.path, command.entrypoint, command.entry, command.file, command.source].filter((entry) => typeof entry === "string" && entry.trim().length > 0).map((entry) => (0, import_node_path4.join)(packageRoot, entry));
  const src = (0, import_node_path4.join)(packageRoot, "src");
  const defaults = [
    (0, import_node_path4.join)(src, `${commandName}.tsx`),
    (0, import_node_path4.join)(src, `${commandName}.ts`),
    (0, import_node_path4.join)(src, `${commandName}.jsx`),
    (0, import_node_path4.join)(src, `${commandName}.js`),
    (0, import_node_path4.join)(src, commandName, "index.tsx"),
    (0, import_node_path4.join)(src, commandName, "index.ts"),
    (0, import_node_path4.join)(src, commandName, "index.jsx"),
    (0, import_node_path4.join)(src, commandName, "index.js"),
    (0, import_node_path4.join)(src, "commands", `${commandName}.tsx`),
    (0, import_node_path4.join)(src, "commands", `${commandName}.ts`),
    (0, import_node_path4.join)(src, "commands", `${commandName}.jsx`),
    (0, import_node_path4.join)(src, "commands", `${commandName}.js`)
  ];
  const candidate = [...explicit, ...defaults].find((entry) => (0, import_node_fs3.existsSync)(entry));
  if (!candidate) {
    throw new Error(`Could not resolve entry file for command ${commandName}`);
  }
  return candidate;
}
function patchExtensionScript(entryPath, code) {
  if (entryPath.toLowerCase().includes("amphetamine")) {
    return code.replace(
      /if\s*\(\s*isSessionActive\s*===\s*["']true["']\s*\)\s*\{[\s\S]*?toast\.title\s*=\s*["']A session is already running["'][\s\S]*?return\s+false;?\s*\}/g,
      `if (isSessionActive === "true") {
    await runAppleScript(\`
    tell application "Amphetamine"
        end session
    end tell
  \`);
  }`
    );
  }
  return code;
}
async function bundleCommand(entryPath, packageRoot) {
  if (entryPath.includes(`${(0, import_node_path4.join)(".sc-build", "")}`) || entryPath.includes("/.sc-build/")) {
    const prebuilt = (0, import_node_fs3.readFileSync)(entryPath, "utf8");
    if (!prebuilt.trim()) throw new Error(`Prebuilt extension bundle is empty: ${entryPath}`);
    return patchExtensionScript(entryPath, prebuilt);
  }
  configurePackagedEsbuildBinary();
  const esbuild = await import("esbuild");
  const legacyCheerioInterop = {
    name: "legacy-cheerio-default-interop",
    setup(build) {
      build.onLoad({ filter: /\.[cm]?[jt]sx?$/ }, (args) => {
        const source = (0, import_node_fs3.readFileSync)(args.path, "utf8");
        if (!/import\s+[A-Za-z_$][\w$]*\s+from\s+['"]cheerio['"]/.test(source)) return null;
        const extension = (0, import_node_path4.extname)(args.path).toLowerCase();
        const loader = extension.endsWith("x") ? extension.slice(1) : extension.slice(1) || "js";
        return {
          contents: source.replace(
            /import\s+([A-Za-z_$][\w$]*)\s+from\s+(['"])cheerio\2/g,
            "import * as $1 from $2cheerio$2"
          ),
          loader
        };
      });
    }
  };
  const result = await esbuild.build({
    entryPoints: [entryPath],
    absWorkingDir: packageRoot,
    bundle: true,
    format: "cjs",
    platform: "node",
    conditions: ["require", "node"],
    plugins: [legacyCheerioInterop],
    write: false,
    target: "node20",
    external: [
      "@raycast/api",
      "@raycast/utils",
      "react",
      "react/jsx-runtime",
      "react/jsx-dev-runtime"
    ],
    nodePaths: [(0, import_node_path4.join)(packageRoot, "node_modules")],
    logLevel: "silent"
  });
  const output = result.outputFiles?.[0]?.text;
  if (!output) {
    throw new Error("esbuild did not produce output");
  }
  return patchExtensionScript(entryPath, output);
}
function createJsxRuntimeShim() {
  const jsx = (type, props, key) => ({
    __jsx: true,
    type,
    props: props ?? {},
    key
  });
  return {
    Fragment: JSX_FRAGMENT,
    jsx,
    jsxs: jsx,
    jsxDEV: jsx
  };
}
function createReactShim(session) {
  const jsxRuntime = createJsxRuntimeShim();
  const jsx = jsxRuntime.jsx;
  const queueEffect = (idx, sideEffect, deps, label = "useEffect") => {
    const prevDeps = session.effectDeps.get(idx);
    if (deps && hookDepsEqual(prevDeps, deps)) return;
    session.pendingEffects.push({ idx, sideEffect, deps, label });
  };
  class Component {
    props;
    state;
    constructor(props) {
      this.props = props;
      this.state = {};
    }
    setState(next) {
      const resolved = typeof next === "function" ? next(this.state, this.props) : next;
      if (resolved == null) return;
      this.state = {
        ...this.state && typeof this.state === "object" ? this.state : {},
        ...resolved && typeof resolved === "object" ? resolved : {}
      };
      session.hasStateUpdates = true;
    }
    forceUpdate() {
      session.hasStateUpdates = true;
    }
  }
  const react = {
    Component,
    PureComponent: Component,
    Fragment: JSX_FRAGMENT,
    createElement: (type, props, ...children) => {
      const nextProps = { ...props ?? {} };
      if (children.length === 1) {
        nextProps.children = children[0];
      } else if (children.length > 1) {
        nextProps.children = children;
      }
      return jsx(type, nextProps);
    },
    createContext: (defaultValue) => {
      const context = {
        $$typeof: REACT_CONTEXT,
        _currentValue: defaultValue,
        _defaultValue: defaultValue
      };
      context.Provider = (props) => {
        context._currentValue = props.value;
        return props.children;
      };
      context.Consumer = (props) => {
        if (typeof props.children === "function") {
          return props.children(context._currentValue);
        }
        return props.children ?? null;
      };
      return context;
    },
    useState: (initial) => {
      const idx = session.hookIndex++;
      if (session.hookStates.length > idx) {
        return session.hookStates[idx];
      }
      let value = typeof initial === "function" ? initial() : initial;
      const setState = (next) => {
        value = typeof next === "function" ? next(value) : next;
        session.hookStates[idx] = [value, setState];
        session.hasStateUpdates = true;
      };
      const tuple = [value, setState];
      session.hookStates[idx] = tuple;
      return tuple;
    },
    useEffect: (sideEffect, deps) => {
      const idx = session.hookIndex++;
      queueEffect(idx, sideEffect, deps, "useEffect");
    },
    useLayoutEffect: (sideEffect, deps) => {
      const idx = session.hookIndex++;
      queueEffect(idx, sideEffect, deps, "useLayoutEffect");
    },
    useMemo: (factory, deps) => {
      const idx = session.hookIndex++;
      const existing = session.hookStates[idx];
      if (existing?.kind === "memo" && deps && hookDepsEqual(existing.deps, deps)) {
        return existing.value;
      }
      const value = factory();
      session.hookStates[idx] = { kind: "memo", value, deps };
      return value;
    },
    useCallback: (callback, deps) => {
      const idx = session.hookIndex++;
      const existing = session.hookStates[idx];
      if (existing?.kind === "callback" && deps && hookDepsEqual(existing.deps, deps)) {
        return existing.value;
      }
      session.hookStates[idx] = { kind: "callback", value: callback, deps };
      return callback;
    },
    useRef: (value) => {
      const idx = session.hookIndex++;
      const existing = session.hookStates[idx];
      if (existing && typeof existing === "object" && "current" in existing) {
        return existing;
      }
      const ref = { current: value };
      session.hookStates[idx] = ref;
      return ref;
    },
    useContext: (context) => {
      session.hookIndex++;
      return context && context.$$typeof === REACT_CONTEXT ? context._currentValue : null;
    },
    useDebugValue: () => {
      session.hookIndex++;
    },
    useSyncExternalStore: (subscribe, getSnapshot, getServerSnapshot) => {
      void getServerSnapshot;
      const idx = session.hookIndex++;
      const snapshot = getSnapshot();
      const existing = session.hookStates[idx];
      const state = existing?.kind === "external-store" ? existing : { kind: "external-store", snapshot };
      state.snapshot = snapshot;
      session.hookStates[idx] = state;
      queueEffect(
        idx,
        () => subscribe(() => {
          const nextSnapshot = getSnapshot();
          if (Object.is(state.snapshot, nextSnapshot)) return;
          state.snapshot = nextSnapshot;
          session.hasStateUpdates = true;
        }),
        [subscribe, getSnapshot],
        "useSyncExternalStore"
      );
      return snapshot;
    },
    useReducer: (reducer, initialArg) => {
      const idx = session.hookIndex++;
      if (session.hookStates.length > idx) {
        return session.hookStates[idx];
      }
      let current = initialArg;
      const dispatch = (action) => {
        current = reducer(current, action);
        session.hookStates[idx] = [current, dispatch];
        session.hasStateUpdates = true;
      };
      const tuple = [current, dispatch];
      session.hookStates[idx] = tuple;
      return tuple;
    },
    memo: (component) => component,
    forwardRef: (renderer) => renderer,
    isValidElement: (value) => isJsxNode(value)
  };
  return {
    ...react,
    default: react,
    __esModule: true
  };
}
function makeToken(name) {
  return { __raycastComponent: true, name };
}
function isToken(value) {
  return Boolean(
    value && typeof value === "object" && value.__raycastComponent === true && typeof value.name === "string"
  );
}
function normalizeActionTitle(typeName, props) {
  if (typeof props.title === "string" && props.title.trim().length > 0) {
    return props.title.trim();
  }
  switch (typeName) {
    case "Action.CopyToClipboard":
      return "Copy to Clipboard";
    case "Action.Paste":
      return "Paste";
    case "Action.OpenInBrowser":
      return "Open in Browser";
    case "Action.Push":
      return "Open";
    case "Action.Pop":
      return "Back";
    case "Action.ShowInFinder":
      return "Show in Finder";
    case "Action.SubmitForm":
      return "Submit";
    default:
      return "Action";
  }
}
function stableActionId(index, typeName, title) {
  const hash = (0, import_node_crypto2.createHash)("sha1").update(`${index}:${typeName}:${title}`).digest("hex").slice(0, 12);
  return `ext-action-${index}-${hash}`;
}
function parseShortcut(shortcut) {
  if (!shortcut || typeof shortcut !== "object") return void 0;
  const s = shortcut;
  const modifiers = Array.isArray(s.modifiers) ? s.modifiers.filter((m) => typeof m === "string") : void 0;
  const key = typeof s.key === "string" ? s.key : void 0;
  if (!modifiers && !key) return void 0;
  return { modifiers, key };
}
function pushFeedback(session, feedback) {
  session.feedback.push(feedback);
  if (session.feedback.length > 20) {
    session.feedback.splice(0, session.feedback.length - 20);
  }
}
function pushEffect(session, effect) {
  session.effects.push(effect);
  if (session.effects.length > 50) {
    session.effects.splice(0, session.effects.length - 50);
  }
}
function createLocalStorageShim(packageRoot) {
  const storagePath = (0, import_node_path4.join)(packageRoot, ".tezbar-local-storage.json");
  const readAll = () => {
    if (!(0, import_node_fs3.existsSync)(storagePath)) return {};
    try {
      const parsed = JSON.parse((0, import_node_fs3.readFileSync)(storagePath, "utf8"));
      return parsed && typeof parsed === "object" ? parsed : {};
    } catch {
      return {};
    }
  };
  const writeAll = (value) => {
    (0, import_node_fs3.mkdirSync)((0, import_node_path4.dirname)(storagePath), { recursive: true });
    (0, import_node_fs3.writeFileSync)(storagePath, JSON.stringify(value, null, 2), "utf8");
  };
  return {
    getItem: async (key) => readAll()[String(key)],
    setItem: async (key, value) => {
      const all = readAll();
      all[String(key)] = String(value);
      writeAll(all);
    },
    removeItem: async (key) => {
      const all = readAll();
      delete all[String(key)];
      writeAll(all);
    },
    clear: async () => writeAll({}),
    allItems: async () => readAll()
  };
}
function createCacheShim(packageRoot) {
  return class CacheShim {
    subscribers = /* @__PURE__ */ new Set();
    storagePath;
    constructor(options) {
      const rawNamespace = typeof options?.namespace === "string" && options.namespace.trim().length > 0 ? options.namespace.trim() : "shared";
      const safeNamespace = rawNamespace.replace(/[^a-z0-9._-]+/gi, "_");
      this.storagePath = (0, import_node_path4.join)(packageRoot, ".tezbar-support", "cache", `${safeNamespace}.json`);
    }
    readAll() {
      if (!(0, import_node_fs3.existsSync)(this.storagePath)) return {};
      try {
        const parsed = JSON.parse((0, import_node_fs3.readFileSync)(this.storagePath, "utf8"));
        return parsed && typeof parsed === "object" ? parsed : {};
      } catch {
        return {};
      }
    }
    writeAll(value) {
      (0, import_node_fs3.mkdirSync)((0, import_node_path4.dirname)(this.storagePath), { recursive: true });
      (0, import_node_fs3.writeFileSync)(this.storagePath, JSON.stringify(value, null, 2), "utf8");
    }
    notify(key, value) {
      for (const subscriber of this.subscribers) {
        try {
          subscriber(key, value);
        } catch {
        }
      }
    }
    get isEmpty() {
      return Object.keys(this.readAll()).length === 0;
    }
    get(key) {
      return this.readAll()[String(key)];
    }
    set(key, value) {
      const all = this.readAll();
      all[String(key)] = String(value);
      this.writeAll(all);
      this.notify(String(key), String(value));
    }
    has(key) {
      return Object.prototype.hasOwnProperty.call(this.readAll(), String(key));
    }
    remove(key) {
      const all = this.readAll();
      const normalizedKey = String(key);
      const existed = Object.prototype.hasOwnProperty.call(all, normalizedKey);
      if (!existed) return false;
      delete all[normalizedKey];
      this.writeAll(all);
      this.notify(normalizedKey, void 0);
      return true;
    }
    clear(options) {
      this.writeAll({});
      if (options?.notifySubscribers !== false) {
        this.notify(void 0, void 0);
      }
    }
    subscribe(subscriber) {
      this.subscribers.add(subscriber);
      return () => {
        this.subscribers.delete(subscriber);
      };
    }
  };
}
async function copyToSystemClipboard(session, value) {
  const effectValue = typeof value === "string" ? value : value && typeof value === "object" ? String(
    value.text ?? value.file ?? ""
  ) : String(value ?? "");
  pushEffect(session, { kind: "clipboard", value: effectValue.slice(0, 2e3) });
  if (session.effectMode === "record") return;
  await copyClipboardContent(value);
}
function avatarIcon(value) {
  const text = String(value ?? "?").trim() || "?";
  const initials = text.split(/\s+/).slice(0, 2).map((part) => part.charAt(0).toUpperCase()).join("").replace(/[<>&"']/g, "") || "?";
  let hash = 0;
  for (const char of text) hash = hash * 31 + char.charCodeAt(0) >>> 0;
  const hue = hash % 360;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64"><rect width="64" height="64" rx="32" fill="hsl(${hue} 58% 42%)"/><text x="32" y="39" text-anchor="middle" fill="white" font-family="-apple-system, sans-serif" font-size="22" font-weight="600">${initials}</text></svg>`;
  return { source: `data:image/svg+xml;base64,${Buffer.from(svg).toString("base64")}` };
}
function createRaycastApiShim(session) {
  class PKCEClientShim {
    tokenPath;
    constructor(options = {}) {
      const providerId = String(options.providerId ?? options.providerName ?? "oauth").replace(/[^a-z0-9._-]+/gi, "_").toLowerCase();
      this.tokenPath = (0, import_node_path4.join)(session.packageRoot, ".tezbar-support", "oauth", `${providerId}.json`);
    }
    async getTokens() {
      if (!(0, import_node_fs3.existsSync)(this.tokenPath)) return void 0;
      try {
        const stored = JSON.parse((0, import_node_fs3.readFileSync)(this.tokenPath, "utf8"));
        return {
          ...stored,
          isExpired: () => typeof stored.expiresIn === "number" && stored.expiresIn <= Date.now()
        };
      } catch {
        return void 0;
      }
    }
    async setTokens(response) {
      const expiresInSeconds = Number(response.expires_in);
      const tokens = {
        accessToken: String(response.access_token ?? response.accessToken ?? ""),
        refreshToken: String(response.refresh_token ?? response.refreshToken ?? ""),
        idToken: String(response.id_token ?? response.idToken ?? ""),
        scope: String(response.scope ?? ""),
        expiresIn: Number.isFinite(expiresInSeconds) ? Date.now() + expiresInSeconds * 1e3 : Number(response.expiresIn) || void 0
      };
      (0, import_node_fs3.mkdirSync)((0, import_node_path4.dirname)(this.tokenPath), { recursive: true });
      (0, import_node_fs3.writeFileSync)(this.tokenPath, JSON.stringify(tokens), "utf8");
    }
    async removeTokens() {
      await (0, import_promises.rm)(this.tokenPath, { force: true });
    }
    async authorizationRequest(options) {
      return {
        ...options,
        codeVerifier: makeId("pkce").replace(/[^a-z0-9]/gi, ""),
        redirectURI: `raycast://oauth?extension=${encodeURIComponent(session.extensionId)}`
      };
    }
    async authorize() {
      throw new Error("Interactive OAuth authorization is not yet available in Raymes");
    }
  }
  const ListItemDetailMetadata = Object.assign(makeToken("List.Item.Detail.Metadata"), {
    Label: makeToken("List.Item.Detail.Metadata.Label"),
    TagList: Object.assign(makeToken("List.Item.Detail.Metadata.TagList"), {
      Item: makeToken("List.Item.Detail.Metadata.TagList.Item")
    }),
    Separator: makeToken("List.Item.Detail.Metadata.Separator"),
    Link: makeToken("List.Item.Detail.Metadata.Link")
  });
  const ListItemDetail = Object.assign(makeToken("List.Item.Detail"), {
    Metadata: ListItemDetailMetadata
  });
  const List = Object.assign(makeToken("List"), {
    Item: Object.assign(makeToken("List.Item"), {
      Detail: ListItemDetail
    }),
    Section: makeToken("List.Section"),
    EmptyView: makeToken("List.EmptyView"),
    Dropdown: Object.assign(makeToken("List.Dropdown"), {
      Section: makeToken("List.Dropdown.Section"),
      Item: makeToken("List.Dropdown.Item")
    })
  });
  const FormDropdown = Object.assign(makeToken("Form.Dropdown"), {
    Item: makeToken("Form.Dropdown.Item"),
    Section: makeToken("Form.Dropdown.Section")
  });
  const FormTagPicker = Object.assign(makeToken("Form.TagPicker"), {
    Item: makeToken("Form.TagPicker.Item")
  });
  const Form = Object.assign(makeToken("Form"), {
    TextField: makeToken("Form.TextField"),
    TextArea: makeToken("Form.TextArea"),
    Checkbox: makeToken("Form.Checkbox"),
    Dropdown: FormDropdown,
    TagPicker: FormTagPicker,
    FilePicker: makeToken("Form.FilePicker"),
    DatePicker: makeToken("Form.DatePicker"),
    PasswordField: makeToken("Form.PasswordField"),
    Separator: makeToken("Form.Separator"),
    Description: makeToken("Form.Description")
  });
  const Grid = Object.assign(makeToken("Grid"), {
    Item: makeToken("Grid.Item"),
    Section: makeToken("Grid.Section"),
    EmptyView: makeToken("Grid.EmptyView"),
    Dropdown: List.Dropdown,
    Inset: {
      Small: "small",
      Medium: "medium",
      Large: "large"
    },
    Fit: {
      Fill: "fill",
      Contain: "contain"
    }
  });
  const Detail = Object.assign(makeToken("Detail"), {
    Metadata: Object.assign(makeToken("Detail.Metadata"), {
      Label: makeToken("Detail.Metadata.Label"),
      TagList: Object.assign(makeToken("Detail.Metadata.TagList"), {
        Item: makeToken("Detail.Metadata.TagList.Item")
      }),
      Separator: makeToken("Detail.Metadata.Separator"),
      Link: makeToken("Detail.Metadata.Link")
    })
  });
  const MenuBarExtra = Object.assign(makeToken("MenuBarExtra"), {
    Item: makeToken("MenuBarExtra.Item"),
    Section: makeToken("MenuBarExtra.Section"),
    Separator: makeToken("MenuBarExtra.Separator"),
    Submenu: makeToken("MenuBarExtra.Submenu")
  });
  const Action = Object.assign(makeToken("Action"), {
    CopyToClipboard: makeToken("Action.CopyToClipboard"),
    Paste: makeToken("Action.Paste"),
    OpenInBrowser: makeToken("Action.OpenInBrowser"),
    Push: makeToken("Action.Push"),
    Pop: makeToken("Action.Pop"),
    ShowInFinder: makeToken("Action.ShowInFinder"),
    SubmitForm: makeToken("Action.SubmitForm"),
    Style: {
      Regular: "regular",
      Destructive: "destructive"
    }
  });
  const ActionPanel = Object.assign(makeToken("ActionPanel"), {
    Section: makeToken("ActionPanel.Section")
  });
  class ToastShim {
    static Style = {
      Success: "success",
      Failure: "failure",
      Animated: "animated"
    };
    currentStyle;
    currentTitle;
    currentMessage;
    shownFeedback;
    shownEffect;
    constructor(options) {
      this.currentStyle = options.style;
      this.currentTitle = options.title;
      this.currentMessage = options.message;
    }
    get style() {
      return this.currentStyle;
    }
    set style(value) {
      this.currentStyle = value;
      if (this.shownFeedback) this.shownFeedback.style = value;
      if (this.shownEffect) this.shownEffect.style = value;
    }
    get title() {
      return this.currentTitle;
    }
    set title(value) {
      this.currentTitle = value;
      if (this.shownFeedback) this.shownFeedback.title = value;
      if (this.shownEffect) this.shownEffect.title = value;
    }
    get message() {
      return this.currentMessage;
    }
    set message(value) {
      this.currentMessage = value;
      if (this.shownFeedback) this.shownFeedback.message = value;
      if (this.shownEffect) this.shownEffect.message = value;
    }
    async show() {
      this.shownFeedback = {
        kind: "toast",
        style: this.currentStyle,
        title: this.currentTitle,
        message: this.currentMessage
      };
      this.shownEffect = {
        kind: "toast",
        style: this.currentStyle,
        title: this.currentTitle,
        message: this.currentMessage
      };
      pushFeedback(session, this.shownFeedback);
      pushEffect(session, this.shownEffect);
    }
    async hide() {
    }
  }
  return {
    List,
    Form,
    Grid,
    Image: {
      Mask: IMAGE_MASK
    },
    ImageMask: IMAGE_MASK,
    AI: {
      ask: async (prompt) => askExtensionAI(String(prompt ?? "")),
      Model: iconProxy,
      Creativity: {
        None: "none",
        Low: "low",
        Medium: "medium",
        High: "high",
        Maximum: "maximum"
      }
    },
    Detail,
    MenuBarExtra,
    OAuth: {
      PKCEClient: PKCEClientShim,
      RedirectMethod: {
        AppURI: "appURI",
        Web: "web"
      }
    },
    Action,
    ActionPanel,
    CopyToClipboardAction: Action.CopyToClipboard,
    OpenInBrowserAction: Action.OpenInBrowser,
    PushAction: Action.Push,
    PopToRootType: { Default: "default", Immediate: "immediate", Suspended: "suspended" },
    // Tezbar does not forward extension exceptions to Raycast telemetry.
    captureException: () => {
    },
    Icon: iconProxy,
    Color: iconProxy,
    Keyboard: {
      Shortcut: {
        Common: {
          Copy: { modifiers: ["cmd"], key: "c" },
          CopyPath: { modifiers: ["cmd", "shift"], key: "c" },
          Refresh: { modifiers: ["cmd"], key: "r" }
        }
      },
      Key: iconProxy
    },
    Toast: ToastShim,
    Alert: {
      ActionStyle: { Default: "default", Destructive: "destructive", Cancel: "cancel" }
    },
    LaunchType: {
      UserInitiated: "userInitiated",
      Background: "background"
    },
    environment: {
      raycastVersion: "1.80.0",
      extensionName: session.extensionId,
      commandName: session.commandName,
      isDevelopment: false,
      commandMode: session.commandMode,
      assetsPath: (0, import_node_path4.join)(session.packageRoot, "assets"),
      supportPath: (0, import_node_path4.join)(session.packageRoot, ".tezbar-support"),
      canAccess: () => false,
      get searchText() {
        return session.searchText;
      }
    },
    LocalStorage: createLocalStorageShim(session.packageRoot),
    Cache: createCacheShim(session.packageRoot),
    runAppleScript: (source) => runAppleScriptForSession(session, source),
    Clipboard: {
      copy: async (value) => {
        await copyToSystemClipboard(session, value);
      },
      paste: async (value) => {
        await copyToSystemClipboard(session, value);
        if (session.effectMode !== "record") await pasteClipboardContent();
      },
      clear: async () => {
        await copyToSystemClipboard(session, "");
      },
      read: async () => {
        const text = await clipboard.readText();
        return text ? { text } : {};
      },
      readText: async () => clipboard.readText()
    },
    getPreferenceValues: () => session.preferences,
    getSelectedFinderItems: async () => {
      if (session.effectMode === "record") return [];
      if (process.platform === "win32") {
        try {
          const { stdout } = await execFileAsync3(
            "powershell.exe",
            [
              "-NoLogo",
              "-NoProfile",
              "-NonInteractive",
              "-Command",
              "$window=(New-Object -ComObject Shell.Application).Windows() | Where-Object { $_.FullName -match 'explorer.exe$' } | Select-Object -First 1; if($null-ne $window){$window.Document.SelectedItems() | ForEach-Object {$_.Path}}"
            ],
            { timeout: 3e3, windowsHide: true }
          );
          return stdout.split(/\r?\n/).map((value) => value.trim()).filter(Boolean).map((path4) => ({ path: path4 }));
        } catch {
          return [];
        }
      }
      if (process.platform !== "darwin") return [];
      try {
        const output = await runAppleScript(`
          tell application "Finder"
            set selectedItems to selection as alias list
            set selectedPaths to {}
            repeat with selectedItem in selectedItems
              set end of selectedPaths to POSIX path of selectedItem
            end repeat
            set AppleScript's text item delimiters to linefeed
            return selectedPaths as text
          end tell
        `);
        return output.split(/\r?\n/).map((value) => value.trim()).filter(Boolean).map((path4) => ({ path: path4 }));
      } catch {
        return [];
      }
    },
    launchCommand: async (options) => {
      if (!options || typeof options !== "object") {
        throw new Error("launchCommand requires a command name");
      }
      const request2 = options;
      const targetName = typeof request2.name === "string" ? request2.name.trim() : "";
      if (!targetName) throw new Error("launchCommand requires a command name");
      const argumentValues = request2.arguments && typeof request2.arguments === "object" ? Object.fromEntries(
        Object.entries(request2.arguments).map(([key, value]) => [
          key,
          typeof value === "string" ? value : String(value ?? "")
        ])
      ) : {};
      const result = await runCommandFromPackagePath(
        (0, import_node_path4.join)(session.packageRoot, "package.json"),
        session.extensionId,
        targetName,
        argumentValues,
        void 0,
        { effectMode: session.effectMode }
      );
      if (!result.ok) throw new Error(result.message);
      for (const effect of result.effects ?? []) pushEffect(session, effect);
    },
    useNavigation: () => ({
      push: (next) => {
        session.stack.push(next);
      },
      pop: () => {
        if (session.stack.length > 1) {
          session.stack.pop();
        }
      }
    }),
    showToast: async (optionsOrStyle, title, message) => {
      let toast;
      if (typeof optionsOrStyle === "string") {
        toast = new ToastShim({
          style: optionsOrStyle,
          title: title ? String(title) : "",
          message: message ? String(message) : void 0
        });
      } else {
        const opts = optionsOrStyle && typeof optionsOrStyle === "object" ? optionsOrStyle : {};
        toast = new ToastShim({
          style: typeof opts.style === "string" ? opts.style : void 0,
          title: typeof opts.title === "string" ? opts.title : "",
          message: typeof opts.message === "string" ? opts.message : void 0
        });
      }
      await toast.show();
      return toast;
    },
    showHUD: async (title) => {
      const message = String(title || "");
      pushFeedback(session, { kind: "hud", message });
      pushEffect(session, { kind: "hud", message });
    },
    open: async (target) => {
      if (typeof target !== "string") return;
      if (/^[a-z][a-z0-9+.-]*:\/\//i.test(target) || target.startsWith("mailto:")) {
        pushEffect(session, { kind: "open", value: target });
        if (session.effectMode === "record") return;
        await shell.openExternal(target);
      }
    },
    showInFinder: async (path4) => {
      if (typeof path4 !== "string") return;
      pushEffect(session, { kind: "show-in-finder", value: path4 });
      if (session.effectMode === "record") return;
      shell.showItemInFolder(path4);
    },
    getApplications: async () => {
      const now = Date.now();
      if (applicationsCache && applicationsCache.expiresAt > now) {
        return applicationsCache.promise;
      }
      console.log("[getApplications] Starting Spotlight query for installed apps...");
      const promise = (async () => {
        if (process.env.TEZBAR_EXTENSION_WORKER === "1" || process.platform === "win32") {
          return listApplications().sort((a, b) => a.name.localeCompare(b.name));
        }
        try {
          const { stdout } = await execFileAsync3(
            "/usr/bin/mdfind",
            ["kMDItemKind == 'Application'"],
            {
              maxBuffer: 10 * 1024 * 1024,
              timeout: 3e3
            }
          );
          const apps = stdout.trim().split("\n").filter((p) => p.endsWith(".app")).map((appPath) => ({
            name: (0, import_node_path4.basename)(appPath, ".app"),
            path: appPath,
            bundleId: readAppBundleIdentifier(appPath)
          })).sort((a, b) => a.name.localeCompare(b.name));
          console.log(`[getApplications] mdfind returned ${apps.length} applications`);
          return apps;
        } catch (err) {
          console.warn("[getApplications] mdfind failed, falling back to directory scan:", err);
          const apps = [];
          const dirs = ["/Applications", "/System/Applications", (0, import_node_path4.join)((0, import_node_os2.homedir)(), "Applications")];
          for (const dir of dirs) {
            try {
              for (const entry of (0, import_node_fs3.readdirSync)(dir)) {
                if (entry.endsWith(".app")) {
                  const appPath = (0, import_node_path4.join)(dir, entry);
                  apps.push({
                    name: (0, import_node_path4.basename)(entry, ".app"),
                    path: appPath,
                    bundleId: readAppBundleIdentifier(appPath)
                  });
                }
              }
            } catch (dirErr) {
              console.warn(`[getApplications] Could not scan directory ${dir}:`, dirErr);
            }
          }
          console.log(`[getApplications] Directory scan found ${apps.length} applications`);
          return apps.sort((a, b) => a.name.localeCompare(b.name));
        }
      })();
      applicationsCache = {
        expiresAt: now + APPLICATIONS_CACHE_TTL_MS,
        promise
      };
      return promise;
    },
    getSelectedText: async () => {
      if (session.effectMode === "record") return "";
      return readSelectedText();
    },
    getFrontmostApplication: async () => {
      if (process.platform === "win32") {
        try {
          const { stdout } = await execFileAsync3(
            "powershell.exe",
            [
              "-NoLogo",
              "-NoProfile",
              "-NonInteractive",
              "-Command",
              `Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class TezbarForeground { [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow(); [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint p); }'; $pidValue=0; [void][TezbarForeground]::GetWindowThreadProcessId([TezbarForeground]::GetForegroundWindow(),[ref]$pidValue); $process=Get-Process -Id $pidValue -ErrorAction Stop; [pscustomobject]@{name=$process.ProcessName;path=$process.Path} | ConvertTo-Json -Compress`
            ],
            { timeout: 3e3, windowsHide: true }
          );
          const value = JSON.parse(stdout);
          if (typeof value.name === "string") {
            return {
              name: value.name,
              path: typeof value.path === "string" ? value.path : ""
            };
          }
        } catch {
        }
        return { name: "Tezbar", path: process.execPath };
      }
      try {
        const script = `tell application "System Events"
          set p to first process whose frontmost is true
          set appPath to ""
          try
            set appPath to POSIX path of (application file of p as alias)
          end try
          return (name of p) & linefeed & (bundle identifier of p) & linefeed & appPath
        end tell`;
        const { stdout } = await execFileAsync3("/usr/bin/osascript", ["-e", script], {
          timeout: 3e3
        });
        const lines = stdout.trim().split("\n");
        const name = lines[0]?.trim() || "";
        const bundleId = lines[1]?.trim() || void 0;
        const appPath = lines[2]?.trim() || (name ? `/Applications/${name}.app` : "");
        if (name) {
          return {
            name,
            path: appPath,
            bundleId: bundleId || (appPath ? readAppBundleIdentifier(appPath) : void 0)
          };
        }
        return { name: "Tezbar", path: process.execPath };
      } catch {
        return { name: "Tezbar", path: process.execPath };
      }
    },
    getDefaultApplication: async () => {
      return null;
    },
    confirmAlert: async (options) => {
      if (session.effectMode === "record") return false;
      return confirmExtensionAlert(options);
    },
    openExtensionPreferences: async () => {
    },
    openCommandPreferences: async () => {
    },
    updateCommandMetadata: async () => {
    },
    closeMainWindow: async () => {
    },
    popToRoot: async () => {
    },
    clearSearchBar: async () => {
    }
  };
}
function registerWindowsOAuthProtocols(port) {
  if (process.platform !== "win32") return () => {
  };
  try {
    const portFile = (0, import_node_path4.join)((0, import_node_os2.tmpdir)(), "tezbar-oauth.port");
    (0, import_node_fs3.writeFileSync)(portFile, String(port), "utf8");
    const bridgeScriptPath = (0, import_node_path4.join)((0, import_node_os2.tmpdir)(), "tezbar-oauth-bridge.ps1");
    const bridgeScript = `param([string]$url)
if (-not $url) { exit }
$portFile = Join-Path $env:TEMP 'tezbar-oauth.port'
if (-not (Test-Path $portFile)) { exit }
$port = (Get-Content -Raw $portFile).Trim()
if (-not $port) { exit }
try {
  $encoded = [System.Uri]::EscapeDataString($url)
  Invoke-RestMethod -Uri ("http://127.0.0.1:$port/oauth/callback?url=" + $encoded) -TimeoutSec 5 -UseBasicParsing | Out-Null
} catch {}
`;
    (0, import_node_fs3.writeFileSync)(bridgeScriptPath, bridgeScript, "utf8");
    const commandValue = `powershell.exe -WindowStyle Hidden -NoProfile -ExecutionPolicy Bypass -File "${bridgeScriptPath}" "%1"`;
    for (const scheme of ["com.raycast", "raycast"]) {
      try {
        (0, import_node_child_process2.execFileSync)("reg.exe", ["add", `HKCU\\Software\\Classes\\${scheme}`, "/ve", "/d", `URL:${scheme} Protocol`, "/f"], { stdio: "ignore" });
        (0, import_node_child_process2.execFileSync)("reg.exe", ["add", `HKCU\\Software\\Classes\\${scheme}`, "/v", "URL Protocol", "/d", "", "/f"], { stdio: "ignore" });
        (0, import_node_child_process2.execFileSync)("reg.exe", ["add", `HKCU\\Software\\Classes\\${scheme}\\shell\\open\\command`, "/ve", "/d", commandValue, "/f"], { stdio: "ignore" });
      } catch (err) {
        console.warn(`[OAuth] failed to register protocol ${scheme}:`, err);
      }
    }
    return () => {
      try {
        if ((0, import_node_fs3.existsSync)(portFile)) (0, import_node_fs3.unlinkSync)(portFile);
      } catch {
      }
    };
  } catch (error) {
    console.warn("[OAuth] failed to setup Windows protocol handler:", error);
    return () => {
    };
  }
}
function createRaycastUtilsShim(session) {
  const CacheShim = createCacheShim(session.packageRoot);
  const cache = new CacheShim();
  const functionCache = /* @__PURE__ */ new Map();
  const useState = createReactShim(session).useState;
  const useCachedState = (key, initialValue) => {
    const hookIdx = session.hookIndex++;
    const existing = session.hookStates[hookIdx];
    if (existing) return existing;
    const getInitialValue = () => {
      const raw = cache.get(String(key));
      if (typeof raw === "string") {
        try {
          return JSON.parse(raw);
        } catch {
          return raw;
        }
      }
      return typeof initialValue === "function" ? initialValue() : initialValue;
    };
    let current = getInitialValue();
    const setValue = (next) => {
      current = typeof next === "function" ? next(current) : next;
      cache.set(String(key), JSON.stringify(current));
      session.hookStates[hookIdx] = [current, setValue];
      session.hasStateUpdates = true;
    };
    const tuple = [current, setValue];
    session.hookStates[hookIdx] = tuple;
    return tuple;
  };
  const cacheKey = (fn, args) => {
    const fnSig = typeof fn === "function" ? fn.toString().slice(0, 120) : String(fn);
    let argsKey;
    try {
      argsKey = JSON.stringify(args);
    } catch {
      argsKey = String(args);
    }
    return `${fnSig}:${argsKey}`;
  };
  const makePromiseHook = (persistent = false) => {
    return (fn, args, options) => {
      const hookIdx = session.hookIndex++;
      const stableArgs = Array.isArray(args) ? args : [];
      const opts = options && typeof options === "object" ? options : {};
      const key = `${hookIdx}:${cacheKey(fn, stableArgs)}`;
      const label = promiseHookLabel(hookIdx, fn, stableArgs);
      const shouldExecute = opts?.execute !== false;
      const previousKey = session.promiseKeysByHook.get(hookIdx);
      const previousEntry = previousKey && previousKey !== key ? session.promiseCache.get(previousKey) : void 0;
      const previousData = opts.keepPreviousData ? previousEntry?.data : void 0;
      if (previousKey !== key) {
        opts.abortable?.current?.abort();
        if (previousKey) session.promiseCache.delete(previousKey);
        session.promisePaginationByHook.delete(hookIdx);
        session.promiseKeysByHook.set(hookIdx, key);
      }
      const schedule = (retainedData) => {
        if (!shouldExecute || typeof fn !== "function" || session.disposed) return null;
        const startedAt = Date.now();
        console.log(`[usePromise] Scheduled ${label}`);
        opts.abortable?.current?.abort();
        const controller = new AbortController();
        if (opts.abortable) opts.abortable.current = controller;
        session.abortControllers.add(controller);
        const tracked = delay(0).then(async () => {
          if (session.disposed || controller.signal.aborted) {
            const error = new Error("Aborted");
            error.name = "AbortError";
            throw error;
          }
          console.log(`[usePromise] Starting ${label}`);
          await Promise.resolve(opts.onWillExecute?.(stableArgs));
          return await Promise.resolve(fn(...stableArgs));
        }).then(async (data) => {
          if (session.promiseCache.get(key)?.promise !== tracked) return data;
          if (typeof data === "function") {
            const loader = data;
            const paginationState = {
              key,
              page: -1,
              hasMore: true,
              loader,
              loadingPromise: null
            };
            session.promisePaginationByHook.set(hookIdx, paginationState);
            const firstPage = await Promise.resolve(loader({ page: 0 }));
            const pageResult = firstPage && typeof firstPage === "object" ? firstPage : { data: firstPage, hasMore: false };
            paginationState.page = 0;
            paginationState.hasMore = pageResult.hasMore === true;
            data = pageResult.data;
          }
          console.log(
            `[usePromise] Resolved ${label} after ${elapsedMs(startedAt)}; data=${Array.isArray(data) ? `array(${data.length})` : typeof data}`
          );
          session.promiseCache.set(key, { data, error: void 0, label });
          if (persistent) writePromiseResultCache(session, key, data);
          await Promise.resolve(opts.onData?.(data));
          if (!session.disposed) session.hasStateUpdates = true;
          return data;
        }).catch(async (error) => {
          if (session.promiseCache.get(key)?.promise !== tracked) return void 0;
          const isAbort = controller.signal.aborted || error instanceof Error && error.name === "AbortError";
          const recovered = !isAbort && await recoverIncompleteChunkedCache(session, error, key);
          if (recovered) {
            console.warn(
              `[usePromise] Recovered ${label} after ${elapsedMs(startedAt)}; retrying on refresh.`
            );
            session.promiseCache.delete(key);
            if (!session.disposed) session.hasStateUpdates = true;
            return void 0;
          }
          session.promiseCache.set(key, {
            data: retainedData,
            error: isAbort ? void 0 : error
          });
          const paginationState = session.promisePaginationByHook.get(hookIdx);
          if (paginationState?.key === key && paginationState.page < 0) {
            session.promisePaginationByHook.delete(hookIdx);
          }
          if (isAbort) return void 0;
          console.error(
            `[usePromise] Rejected ${label} after ${elapsedMs(startedAt)}:`,
            error instanceof Error ? error.message : String(error)
          );
          if (!session.disposed) session.hasStateUpdates = true;
          if (typeof opts.onError === "function") {
            try {
              await Promise.resolve(opts.onError(error));
            } catch (onErrorErr) {
              console.error("[usePromise] onError callback threw:", onErrorErr);
            }
          }
          return void 0;
        }).finally(() => {
          console.log(`[usePromise] Finished ${label} after ${elapsedMs(startedAt)}`);
          session.abortControllers.delete(controller);
          if (opts.abortable?.current === controller) {
            opts.abortable.current = null;
          }
        });
        session.promiseCache.set(key, {
          promise: tracked,
          data: retainedData,
          error: void 0,
          label,
          startedAt
        });
        return tracked;
      };
      let cached = session.promiseCache.get(key);
      if (!cached && shouldExecute) {
        const persistentEntry = persistent ? readPromiseResultCache(session, key) : null;
        const retainedData = previousData ?? persistentEntry?.data ?? opts.initialData;
        schedule(retainedData);
        cached = session.promiseCache.get(key);
      }
      const pagination = () => {
        const paginationState = session.promisePaginationByHook.get(hookIdx);
        if (!paginationState || paginationState.key !== key) return void 0;
        return {
          hasMore: paginationState.hasMore,
          onLoadMore: async () => {
            if (!paginationState.hasMore || session.disposed) return;
            if (paginationState.loadingPromise) return paginationState.loadingPromise;
            const nextPage = paginationState.page + 1;
            const loadingPromise = Promise.resolve(paginationState.loader({ page: nextPage })).then(async (rawResult) => {
              const pageResult = rawResult && typeof rawResult === "object" ? rawResult : { data: rawResult, hasMore: false };
              const currentData = session.promiseCache.get(key)?.data;
              const mergedData = Array.isArray(currentData) && Array.isArray(pageResult.data) ? [...currentData, ...pageResult.data] : pageResult.data;
              paginationState.page = nextPage;
              paginationState.hasMore = pageResult.hasMore === true;
              session.promiseCache.set(key, { data: mergedData, error: void 0, label });
              await Promise.resolve(opts.onData?.(mergedData));
              if (!session.disposed) session.hasStateUpdates = true;
            }).catch(async (error) => {
              session.promiseCache.set(key, {
                data: session.promiseCache.get(key)?.data,
                error,
                label
              });
              paginationState.hasMore = false;
              if (typeof opts.onError === "function") {
                await Promise.resolve(opts.onError(error));
              }
              if (!session.disposed) session.hasStateUpdates = true;
              throw error;
            }).finally(() => {
              paginationState.loadingPromise = null;
            });
            paginationState.loadingPromise = loadingPromise;
            return loadingPromise;
          }
        };
      };
      if (cached) {
        if (cached.promise) {
          session.pendingPromises.push(cached.promise);
          return {
            data: cached.data,
            isLoading: true,
            error: void 0,
            revalidate: async () => {
              await schedule(cached?.data);
            },
            mutate: async (next) => {
              const value = typeof next === "function" ? next(cached?.data) : next;
              session.promiseCache.set(key, { data: value, error: void 0 });
              if (!session.disposed) session.hasStateUpdates = true;
              return value;
            },
            pagination: pagination()
          };
        }
        return {
          data: cached.data,
          isLoading: false,
          error: cached.error,
          revalidate: async () => {
            await schedule(cached?.data);
          },
          mutate: async (next) => {
            const value = typeof next === "function" ? next(cached?.data) : next;
            session.promiseCache.set(key, { data: value, error: void 0 });
            if (!session.disposed) session.hasStateUpdates = true;
            return value;
          },
          pagination: pagination()
        };
      }
      return {
        data: previousData ?? opts.initialData,
        isLoading: false,
        error: void 0,
        revalidate: async () => {
          await schedule(previousData ?? opts.initialData);
        },
        mutate: async (next) => next,
        pagination: pagination()
      };
    };
  };
  const useExecPromise = makePromiseHook();
  const useFetchPromise = makePromiseHook();
  const useAIPromise = makePromiseHook();
  const useSQLPromise = makePromiseHook();
  const useStoragePromise = makePromiseHook();
  const useExec = (command, args = [], options) => {
    const exec = async () => {
      const { stdout, stderr } = await execFileAsync3(command, args, {
        encoding: "utf8",
        maxBuffer: 10 * 1024 * 1024
      });
      const result = { stdout, stderr, exitCode: 0 };
      return options?.parseOutput ? options.parseOutput(result) : stdout;
    };
    return useExecPromise(exec, [command, ...args], options);
  };
  const useFetch = (input, options) => {
    const requestInit = {
      method: options?.method,
      headers: options?.headers,
      body: options?.body
    };
    const load = async () => {
      const response = await fetch(input, requestInit);
      if (!response.ok) throw new Error(`Request failed with status ${response.status}`);
      if (options?.parseResponse) return options.parseResponse(response);
      const contentType = response.headers.get("content-type") ?? "";
      const parsed = contentType.includes("json") ? await response.json() : await response.text();
      const mapped = options?.mapResult ? options.mapResult(parsed) : parsed;
      return mapped && typeof mapped === "object" && "data" in mapped ? mapped.data : mapped;
    };
    return useFetchPromise(load, [String(input), requestInit], options);
  };
  const useSQL = (databasePath, query, options) => {
    const result = useSQLPromise(executeExtensionSQL, [databasePath, query], options);
    return { ...result, permissionView: void 0 };
  };
  const FormValidation = { Required: "required" };
  const useForm = (options = {}) => {
    cache.remove(`form-values:${session.commandName}`);
    cache.remove(`form-errors:${session.commandName}`);
    const [values, setValues] = useState(options.initialValues ?? {});
    const [errors, setErrors] = useState({});
    const validate = (candidate) => {
      const nextErrors = {};
      for (const [key, rule] of Object.entries(options.validation ?? {})) {
        const value = candidate[key];
        const empty = value === void 0 || value === null || value === "" || Array.isArray(value) && value.length === 0;
        const error = rule === FormValidation.Required ? empty ? "This field is required" : void 0 : typeof rule === "function" ? rule(value) : void 0;
        if (error) nextErrors[key] = String(error);
      }
      setErrors(nextErrors);
      return Object.keys(nextErrors).length === 0;
    };
    const setValue = (key, value) => {
      setValues((current) => ({ ...current, [key]: value }));
      setErrors((current) => {
        const next = { ...current };
        delete next[key];
        return next;
      });
    };
    const keys = /* @__PURE__ */ new Set([
      ...Object.keys(options.initialValues ?? {}),
      ...Object.keys(options.validation ?? {}),
      ...Object.keys(values)
    ]);
    const itemProps = Object.fromEntries(
      [...keys].map((key) => [
        key,
        {
          id: key,
          value: values[key],
          error: errors[key],
          onChange: (value) => setValue(key, value)
        }
      ])
    );
    return {
      values,
      itemProps,
      setValue,
      setValidationError: (key, error) => setErrors((current) => ({ ...current, [key]: error })),
      reset: (next) => {
        setValues(next ?? options.initialValues ?? {});
        setErrors({});
      },
      focus: () => void 0,
      handleSubmit: async (submitted) => {
        const candidate = submitted ?? values;
        if (!validate(candidate)) return false;
        return await Promise.resolve(options.onSubmit?.(candidate)) !== false;
      }
    };
  };
  const useFrecencySorting = (input, options) => {
    const namespace = options?.namespace || "default";
    const [ranking, setRanking] = useCachedState(`frecency:${namespace}`, {});
    const keyFor = options?.key ?? ((item) => {
      const candidate = item;
      return candidate?.id === void 0 ? String(item) : String(candidate.id);
    });
    const source = Array.isArray(input) ? input : input && Array.isArray(input.data) ? input.data : [];
    const score = (entry) => {
      const ageHours = (Date.now() - entry.lastVisited) / 36e5;
      return entry.count * Math.pow(0.5, ageHours / 72);
    };
    const data = [...source].sort((a, b) => {
      const aEntry = ranking[keyFor(a)];
      const bEntry = ranking[keyFor(b)];
      if (aEntry && bEntry) return score(bEntry) - score(aEntry);
      if (aEntry) return -1;
      if (bEntry) return 1;
      return options?.sortUnvisited?.(a, b) ?? 0;
    });
    return {
      data,
      visitItem: async (item) => {
        const key = keyFor(item);
        setRanking((current) => ({
          ...current,
          [key]: {
            count: (current[key]?.count ?? 0) + 1,
            lastVisited: Date.now()
          }
        }));
      },
      resetRanking: async (item) => {
        const key = keyFor(item);
        setRanking((current) => {
          const next = { ...current };
          delete next[key];
          return next;
        });
      }
    };
  };
  let activeAccessToken;
  class OAuthServiceShim {
    static github(options) {
      return new OAuthServiceShim("github", options);
    }
    static slack(options) {
      return new OAuthServiceShim("slack", options);
    }
    static google(options) {
      return new OAuthServiceShim("google", options);
    }
    client = {
      removeTokens: async () => {
        activeAccessToken = void 0;
        await (0, import_promises.rm)(this.tokenPath, { force: true });
      }
    };
    provider;
    options;
    token;
    onAuthorize;
    tokenPath;
    constructor(provider, options = {}) {
      this.provider = provider;
      this.options = options;
      this.token = typeof options.personalAccessToken === "string" && options.personalAccessToken.trim() ? options.personalAccessToken.trim() : void 0;
      this.onAuthorize = options.onAuthorize;
      this.tokenPath = (0, import_node_path4.join)(
        session.packageRoot,
        ".tezbar-support",
        "oauth-service",
        `${provider}.json`
      );
      if (!this.applyPersonalToken()) this.applyStoredToken();
    }
    applyPersonalToken() {
      if (!this.token) return false;
      activeAccessToken = this.token;
      this.onAuthorize?.({ token: this.token, type: "personal" });
      return true;
    }
    readStoredTokens() {
      if (!(0, import_node_fs3.existsSync)(this.tokenPath)) return void 0;
      try {
        const value = JSON.parse((0, import_node_fs3.readFileSync)(this.tokenPath, "utf8"));
        return typeof value.accessToken === "string" && value.accessToken ? value : void 0;
      } catch {
        return void 0;
      }
    }
    applyStoredToken() {
      const stored = this.readStoredTokens();
      if (!stored || stored.expiresAt && stored.expiresAt <= Date.now()) return false;
      activeAccessToken = stored.accessToken;
      this.onAuthorize?.({ token: stored.accessToken, type: "oauth" });
      return true;
    }
    async persistTokenResponse(response, existingRefreshToken) {
      const accessToken = String(response.access_token ?? "");
      if (!accessToken)
        throw new Error(`${this.provider} OAuth response did not include an access token`);
      const expiresIn = Number(response.expires_in);
      const stored = {
        accessToken,
        refreshToken: String(response.refresh_token ?? existingRefreshToken ?? "") || void 0,
        expiresAt: Number.isFinite(expiresIn) ? Date.now() + expiresIn * 1e3 : void 0,
        scope: String(response.scope ?? "") || void 0
      };
      (0, import_node_fs3.mkdirSync)((0, import_node_path4.dirname)(this.tokenPath), { recursive: true });
      (0, import_node_fs3.writeFileSync)(this.tokenPath, JSON.stringify(stored), "utf8");
      activeAccessToken = accessToken;
      await Promise.resolve(this.onAuthorize?.({ token: accessToken, type: "oauth" }));
    }
    async exchangeGoogleToken(parameters) {
      const endpoint = typeof this.options.tokenEndpoint === "string" && this.options.tokenEndpoint ? this.options.tokenEndpoint : "https://oauth2.googleapis.com/token";
      const response = await fetch(endpoint, {
        method: "POST",
        headers: { "content-type": "application/x-www-form-urlencoded" },
        body: parameters
      });
      const text = await response.text();
      let payload = {};
      try {
        payload = text ? JSON.parse(text) : {};
      } catch {
        payload = { error_description: text };
      }
      if (!response.ok) {
        throw new Error(
          String(
            payload.error_description ?? payload.error ?? `OAuth token exchange failed (${response.status})`
          )
        );
      }
      return payload;
    }
    async refreshGoogleToken(stored) {
      const clientId = String(this.options.clientId ?? "");
      if (!clientId || !stored.refreshToken) return false;
      const response = await this.exchangeGoogleToken(
        new URLSearchParams({
          client_id: clientId,
          refresh_token: stored.refreshToken,
          grant_type: "refresh_token"
        })
      );
      await this.persistTokenResponse(response, stored.refreshToken);
      return true;
    }
    async authorizeGoogle() {
      const clientId = String(this.options.clientId ?? "");
      const scope = String(this.options.scope ?? "");
      if (!clientId || !scope) throw new Error("Google OAuth requires clientId and scope");
      const stored = this.readStoredTokens();
      if (stored?.refreshToken && await this.refreshGoogleToken(stored)) return;
      const state = (0, import_node_crypto2.randomBytes)(24).toString("base64url");
      const codeVerifier = (0, import_node_crypto2.randomBytes)(48).toString("base64url");
      const codeChallenge = (0, import_node_crypto2.createHash)("sha256").update(codeVerifier).digest("base64url");
      let settleCallback = null;
      let rejectCallback = null;
      const callbackPromise = new Promise(
        (resolve3, reject) => {
          settleCallback = resolve3;
          rejectCallback = reject;
        }
      );
      let cleanupProtocols = null;
      let chosenRedirectUri = "";
      const server = (0, import_node_http.createServer)((request2, response) => {
        const address2 = server.address();
        const currentPort = address2 && typeof address2 === "object" ? address2.port : 0;
        const reqUrl = new URL(request2.url ?? "/", `http://127.0.0.1:${currentPort}`);
        if (reqUrl.pathname === "/" || reqUrl.pathname === "/oauth/manual") {
          response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
          response.end(`<!doctype html>
<html>
<head><meta charset="utf-8"><title>Tezbar Google Authorization</title>
<style>
  body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; display: flex; align-items: center; justify-content: center; height: 100vh; margin: 0; background: #0f172a; color: #f8fafc; }
  .card { background: #1e293b; padding: 32px; border-radius: 12px; width: 450px; text-align: center; box-shadow: 0 4px 20px rgba(0,0,0,0.3); }
  h2 { font-size: 18px; margin-bottom: 12px; color: #38bdf8; }
  p { font-size: 13px; color: #94a3b8; line-height: 1.5; margin-bottom: 20px; }
  input { width: 100%; box-sizing: border-box; padding: 10px; border-radius: 6px; border: 1px solid #334155; background: #0f172a; color: #fff; font-size: 13px; margin-bottom: 12px; }
  button { background: #0284c7; color: #fff; border: none; padding: 10px 20px; border-radius: 6px; font-weight: 600; cursor: pointer; }
  button:hover { background: #0369a1; }
</style>
</head>
<body>
  <div class="card">
    <h2>Tezbar Authorization</h2>
    <p>If your browser did not redirect automatically, paste the redirect URL or code from the address bar below:</p>
    <form method="GET" action="/oauth/callback">
      <input type="text" name="url" placeholder="com.raycast:/oauth?code=... or authorization code" required autofocus />
      <button type="submit">Complete Authorization</button>
    </form>
  </div>
</body>
</html>`);
          return;
        }
        if (reqUrl.pathname !== "/oauth/callback") {
          response.writeHead(404).end("Not found");
          return;
        }
        let targetParams = reqUrl.searchParams;
        const forwardedUrl = reqUrl.searchParams.get("url");
        if (forwardedUrl) {
          const trimmed = forwardedUrl.trim();
          if (trimmed.startsWith("com.raycast:") || trimmed.startsWith("raycast:") || trimmed.startsWith("http")) {
            try {
              const normalized = trimmed.replace(/^com\.raycast:\/?\/?oauth\??/i, "http://localhost/oauth?").replace(/^raycast:\/?\/?oauth\??/i, "http://localhost/oauth?");
              targetParams = new URL(normalized).searchParams;
            } catch {
              const qIndex = trimmed.indexOf("?");
              if (qIndex !== -1) {
                targetParams = new URLSearchParams(trimmed.slice(qIndex + 1));
              }
            }
          } else if (trimmed.length > 10 && !trimmed.includes("=")) {
            targetParams = new URLSearchParams({ code: trimmed, state });
          }
        }
        const receivedState = targetParams.get("state");
        if (receivedState && receivedState !== state) {
          response.writeHead(400, { "content-type": "text/plain; charset=utf-8" });
          response.end("Invalid OAuth state");
          rejectCallback?.(new Error("OAuth callback state did not match"));
          return;
        }
        const providerError = targetParams.get("error");
        const code = targetParams.get("code");
        if (providerError || !code) {
          response.writeHead(400, { "content-type": "text/plain; charset=utf-8" });
          response.end("Authorization failed");
          rejectCallback?.(new Error(providerError || "OAuth callback did not include a code"));
          return;
        }
        response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
        response.end(
          '<!doctype html><title>Tezbar authorized</title><p style="font-family:sans-serif;text-align:center;padding:40px;">Successfully authorized! You can close this window and return to Tezbar.</p>'
        );
        settleCallback?.({ code, redirectUri: chosenRedirectUri });
      });
      await new Promise((resolve3, reject) => {
        server.once("error", reject);
        server.listen(0, "127.0.0.1", resolve3);
      });
      const address = server.address();
      const port = address && typeof address === "object" ? address.port : 0;
      cleanupProtocols = registerWindowsOAuthProtocols(port);
      const authorizationEndpoint = typeof this.options.authorizationEndpoint === "string" && this.options.authorizationEndpoint ? this.options.authorizationEndpoint : "https://accounts.google.com/o/oauth2/v2/auth";
      const isCustomGoogleScheme = !authorizationEndpoint.includes("127.0.0.1") && (clientId.startsWith("690234628480-") || Boolean(this.options.redirectUri) || authorizationEndpoint.includes("accounts.google.com"));
      chosenRedirectUri = typeof this.options.redirectUri === "string" && this.options.redirectUri ? this.options.redirectUri : isCustomGoogleScheme ? "com.raycast:/oauth" : `http://127.0.0.1:${port}/oauth/callback`;
      const authorizationUrl = new URL(authorizationEndpoint);
      authorizationUrl.search = new URLSearchParams({
        client_id: clientId,
        redirect_uri: chosenRedirectUri,
        response_type: "code",
        scope,
        state,
        code_challenge: codeChallenge,
        code_challenge_method: "S256",
        access_type: "offline",
        prompt: "consent"
      }).toString();
      pushEffect(session, { kind: "open", value: authorizationUrl.toString() });
      const timeoutMs = Math.max(1e3, Number(this.options.timeoutMs) || 12e4);
      const timeout = setTimeout(
        () => rejectCallback?.(new Error("OAuth authorization timed out")),
        timeoutMs
      );
      try {
        if (this.options.openAuthorizationUrl) {
          await Promise.resolve(this.options.openAuthorizationUrl(authorizationUrl.toString()));
        } else if (session.effectMode === "record") {
          throw new Error("Interactive OAuth requires system effect mode");
        } else {
          await shell.openExternal(authorizationUrl.toString());
        }
        const callback = await callbackPromise;
        const tokenResponse = await this.exchangeGoogleToken(
          new URLSearchParams({
            client_id: clientId,
            code: callback.code,
            code_verifier: codeVerifier,
            redirect_uri: callback.redirectUri,
            grant_type: "authorization_code"
          })
        );
        await this.persistTokenResponse(tokenResponse);
      } finally {
        clearTimeout(timeout);
        cleanupProtocols?.();
        await new Promise((resolve3) => server.close(() => resolve3()));
      }
    }
    async authorize() {
      if (this.applyPersonalToken() || this.applyStoredToken()) return;
      if (this.provider === "google") return this.authorizeGoogle();
      throw new Error("Add a personal access token in this extension's preferences");
    }
    hasAccessToken() {
      return Boolean(this.token || this.applyStoredToken());
    }
    requiresInteractiveAuthorization() {
      return this.provider === "google";
    }
  }
  return {
    useCachedState,
    FormValidation,
    useForm,
    useFrecencySorting,
    usePromise: makePromiseHook(),
    useFetch,
    useAI: (prompt, options) => useAIPromise(() => askExtensionAI(String(prompt ?? "")), [String(prompt ?? "")], options),
    useCachedPromise: makePromiseHook(true),
    useExec,
    useSQL,
    executeSQL: executeExtensionSQL,
    useLocalStorage: (key, initialValue) => {
      const storage = createLocalStorageShim(session.packageRoot);
      const load = async (storageKey) => {
        const raw = await storage.getItem(storageKey);
        return raw === void 0 ? initialValue : JSON.parse(raw);
      };
      const state = useStoragePromise(load, [String(key)], { initialData: initialValue });
      return {
        value: state.data ?? initialValue,
        setValue: async (next) => {
          await storage.setItem(String(key), JSON.stringify(next));
          await state.mutate(next);
        },
        removeValue: async () => {
          await storage.removeItem(String(key));
          await state.mutate(initialValue);
        },
        isLoading: state.isLoading
      };
    },
    getFavicon: (baseUrl, options) => {
      const size = Math.max(16, Math.min(256, Number(options?.size) || 64));
      const hostMatch = /^[a-z][a-z0-9+.-]*:\/\/([^/?#]+)/i.exec(String(baseUrl ?? "").trim());
      const host = hostMatch?.[1];
      if (host) {
        return {
          source: `https://www.google.com/s2/favicons?domain=${encodeURIComponent(host)}&sz=${size}`
        };
      }
      return { source: String(options?.fallback ?? "Icon.Globe") };
    },
    getAvatarIcon: avatarIcon,
    withCache: (fn, options) => {
      return async (...args) => {
        const key = cacheKey(fn, args);
        const cached = functionCache.get(key);
        if (cached && cached.expiresAt > Date.now()) return cached.value;
        const value = await Promise.resolve(fn(...args));
        functionCache.set(key, {
          expiresAt: Date.now() + Math.max(0, options?.maxAge ?? 5 * 6e4),
          value
        });
        return value;
      };
    },
    getProgressIcon: (progress, color = "#ff6363", options) => {
      const value = Math.max(0, Math.min(1, Number(progress) || 0));
      const radius = 10;
      const circumference = 2 * Math.PI * radius;
      const offset = circumference * (1 - value);
      const background = options?.background || "#ffffff";
      const opacity = options?.backgroundOpacity ?? 0.16;
      const svg = [
        '<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 32 32">',
        `<circle cx="16" cy="16" r="${radius}" fill="none" stroke="${background}" stroke-width="4" opacity="${opacity}"/>`,
        `<circle cx="16" cy="16" r="${radius}" fill="none" stroke="${color}" stroke-width="4" stroke-linecap="round" stroke-dasharray="${circumference}" stroke-dashoffset="${offset}" transform="rotate(-90 16 16)"/>`,
        "</svg>"
      ].join("");
      return `data:image/svg+xml;base64,${Buffer.from(svg).toString("base64")}`;
    },
    runAppleScript: (source) => runAppleScriptForSession(session, source),
    runPowerShellScript: (source, options) => runPowerShellScriptForSession(session, source, options),
    OAuthService: OAuthServiceShim,
    getAccessToken: () => {
      if (!activeAccessToken) throw new Error("No extension access token is configured");
      return { token: activeAccessToken };
    },
    withAccessToken: (service) => (Component) => async (props) => {
      if (!service?.hasAccessToken()) {
        if (service?.requiresInteractiveAuthorization()) {
          await service.authorize();
          return Component(props);
        }
        return {
          __jsx: true,
          type: makeToken("Detail"),
          props: {
            markdown: "# Authentication Required\n\nAdd a personal access token in this extension's preferences."
          }
        };
      }
      return Component(props);
    },
    showFailureToast: (error) => {
      const feedback = {
        kind: "toast",
        style: "failure",
        title: error instanceof Error ? error.message : String(error)
      };
      pushFeedback(session, feedback);
      pushEffect(session, feedback);
    }
  };
}
function isJsxNode(value) {
  return Boolean(
    value && typeof value === "object" && value.__jsx === true && "type" in value && "props" in value
  );
}
function sanitizeValue(value) {
  if (value == null) return value;
  if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
    return value;
  }
  if (Array.isArray(value)) {
    return value.map((entry) => sanitizeValue(entry)).filter((entry) => entry !== void 0);
  }
  if (typeof value === "object") {
    const out = {};
    for (const [key, entry] of Object.entries(value)) {
      if (typeof entry === "function") continue;
      if (key === "children") continue;
      const sanitized = sanitizeValue(entry);
      if (sanitized !== void 0) {
        out[key] = sanitized;
      }
    }
    return out;
  }
  return void 0;
}
function mimeTypeForAsset(path4) {
  switch ((0, import_node_path4.extname)(path4).toLowerCase()) {
    case ".svg":
      return "image/svg+xml";
    case ".png":
      return "image/png";
    case ".jpg":
    case ".jpeg":
      return "image/jpeg";
    case ".gif":
      return "image/gif";
    case ".webp":
      return "image/webp";
    case ".ico":
      return "image/x-icon";
    default:
      return "application/octet-stream";
  }
}
function resolveExtensionMarkdownAssets(markdown, packageRoot) {
  return markdown.replace(
    /!\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g,
    (match, alt, rawSrc) => {
      const src = String(rawSrc || "").trim();
      if (!src || /^(?:https?:|data:|file:)/i.test(src)) return match;
      const cleanSrc = src.split(/[?#]/)[0]?.replace(/^\.?\//, "") ?? "";
      if (!cleanSrc || cleanSrc.startsWith("/") || cleanSrc.includes("..")) return match;
      const assetPath = (0, import_node_path4.join)(packageRoot, "assets", cleanSrc);
      if (!(0, import_node_fs3.existsSync)(assetPath)) {
        console.warn(`[ExtensionAssets] Missing markdown asset: ${assetPath}`);
        return match;
      }
      try {
        const encoded = (0, import_node_fs3.readFileSync)(assetPath).toString("base64");
        console.log(`[ExtensionAssets] Inlined markdown asset: ${assetPath}`);
        return `![${alt}](data:${mimeTypeForAsset(assetPath)};base64,${encoded})`;
      } catch {
        return match;
      }
    }
  );
}
function registerAction(typeName, props, session) {
  const index = session.currentActions.length;
  const title = normalizeActionTitle(typeName, props);
  const id = stableActionId(index, typeName, title);
  const kind = typeName === "Action.CopyToClipboard" || typeName === "Action.Paste" ? "copy" : typeName === "Action.OpenInBrowser" ? "open" : typeName === "Action.Push" ? "push" : typeName === "Action.Pop" ? "pop" : typeName === "Action.SubmitForm" ? "submit-form" : typeName === "Action.ShowInFinder" ? "show-in-finder" : "action";
  const style = typeof props.style === "string" && props.style.toLowerCase() === "destructive" ? "destructive" : "default";
  const action = {
    id,
    title,
    style,
    shortcut: parseShortcut(props.shortcut),
    kind
  };
  session.currentActions.push(action);
  const handler = async (formValues) => {
    if (kind === "copy") {
      const content = props.content ?? props.title ?? "";
      await copyToSystemClipboard(session, content);
      if (typeName === "Action.Paste" && session.effectMode !== "record") await pasteClipboardContent();
      const callback = typeName === "Action.Paste" ? props.onPaste : props.onCopy;
      if (typeof callback === "function") await Promise.resolve(callback());
    }
    if (kind === "open") {
      const url = typeof props.url === "string" ? props.url : "";
      if (url) {
        pushEffect(session, { kind: "open", value: url });
        if (session.effectMode !== "record") await shell.openExternal(url);
        if (typeof props.onOpen === "function") await Promise.resolve(props.onOpen());
      }
    }
    if (kind === "show-in-finder") {
      const path4 = typeof props.path === "string" ? props.path : "";
      if (path4) {
        pushEffect(session, { kind: "show-in-finder", value: path4 });
        if (session.effectMode !== "record") shell.showItemInFolder(path4);
      }
    }
    if (kind === "push" && props.target !== void 0) {
      session.stack.push(props.target);
    }
    if (kind === "pop") {
      if (session.stack.length > 1) {
        session.stack.pop();
      }
    }
    if (kind === "submit-form" && typeof props.onSubmit === "function") {
      await Promise.resolve(
        props.onSubmit(formValues ?? {})
      );
      return;
    }
    if (typeof props.onAction === "function") {
      const event = typeName === "MenuBarExtra.Item" ? { type: "left-click" } : void 0;
      await Promise.resolve(props.onAction(event));
    }
  };
  session.actionHandlers.set(id, handler);
}
function walkRuntimeNodes(input, session, depth, budget, options = {}) {
  if (budget.remaining <= 0 || depth > RUNTIME_RECURSION_LIMIT) {
    return [];
  }
  if (input == null || typeof input === "boolean") {
    return [];
  }
  if (Array.isArray(input)) {
    const nodes = [];
    for (const entry of input) {
      if (options.listItemsSeen && options.listItemLimit && options.listItemsSeen.count >= options.listItemLimit) {
        if (options.listItemsTruncated) options.listItemsTruncated.value = true;
        break;
      }
      nodes.push(...walkRuntimeNodes(entry, session, depth, budget, options));
    }
    return nodes;
  }
  if (!isJsxNode(input)) {
    return [];
  }
  const type = input.type;
  const props = input.props ?? {};
  if (type === JSX_FRAGMENT) {
    return walkRuntimeNodes(props.children, session, depth + 1, budget, options);
  }
  if (typeof type === "function") {
    let rendered;
    try {
      const component = type;
      if (typeof component.prototype?.render === "function") {
        const instance = new component(props);
        const derived = component.getDerivedStateFromProps?.(props, instance.state);
        if (derived && typeof derived === "object") {
          instance.state = {
            ...instance.state && typeof instance.state === "object" ? instance.state : {},
            ...derived
          };
        }
        rendered = instance.render();
      } else {
        rendered = component(props);
      }
    } catch (error) {
      console.error("[ExtensionRuntime] Component render failed:", error);
      const message = error && typeof error === "object" && typeof error.message === "string" ? String(error.message) : String(error);
      session.renderErrors.push(message);
      return [];
    }
    return walkRuntimeNodes(rendered, session, depth + 1, budget, options);
  }
  const typeName = isToken(type) ? type.name : typeof type === "string" ? type : "";
  if (!typeName) return [];
  if (typeName.startsWith("Action")) {
    if (typeName === "ActionPanel" || typeName.startsWith("ActionPanel.")) {
      return walkRuntimeNodes(props.children, session, depth + 1, budget, options);
    }
    registerAction(typeName, props, session);
    return [];
  }
  if ((typeName === "List.Item" || typeName === "Grid.Item") && options.listItemsSeen && options.listItemLimit) {
    if (options.listItemsSeen.count >= options.listItemLimit) {
      return [];
    }
    options.listItemsSeen.count += 1;
  }
  const hasSearchTextChangeHandler = (typeName === "List" || typeName === "Grid") && typeof props.onSearchTextChange === "function";
  if (hasSearchTextChangeHandler) {
    session.searchTextChangeHandler = props.onSearchTextChange;
  }
  const actionStart = session.currentActions.length;
  if (typeName === "MenuBarExtra.Item" && typeof props.onAction === "function") {
    registerAction(typeName, props, session);
  }
  const nestedOptions = {
    ...options,
    listItemsSeen: void 0,
    listItemLimit: void 0,
    listItemsTruncated: void 0
  };
  if (props.actions !== void 0) {
    walkRuntimeNodes(props.actions, session, depth + 1, budget, nestedOptions);
  }
  const actionIds = session.currentActions.slice(actionStart).map((action) => action.id);
  const metadataNodes = props.metadata !== void 0 ? walkRuntimeNodes(props.metadata, session, depth + 1, budget, nestedOptions) : [];
  const detailNodes = props.detail !== void 0 ? walkRuntimeNodes(props.detail, session, depth + 1, budget, nestedOptions) : [];
  const searchBarAccessoryNodes = props.searchBarAccessory !== void 0 ? walkRuntimeNodes(props.searchBarAccessory, session, depth + 1, budget, nestedOptions) : [];
  budget.remaining -= 1;
  const sanitizedProps = sanitizeValue(props);
  if (actionIds.length > 0) {
    if (sanitizedProps) {
      sanitizedProps.actionIds = actionIds;
    }
  }
  if (detailNodes[0] && sanitizedProps) {
    sanitizedProps.detail = detailNodes[0];
  }
  if (searchBarAccessoryNodes[0] && sanitizedProps) {
    sanitizedProps.searchBarAccessory = searchBarAccessoryNodes[0];
  }
  if (metadataNodes[0] && sanitizedProps) {
    sanitizedProps.metadata = metadataNodes[0];
  }
  if (typeName === "List.Dropdown" && typeof props.onChange === "function" && sanitizedProps) {
    const actionId = makeId("list-dropdown");
    sanitizedProps.actionId = actionId;
    session.actionHandlers.set(actionId, async (formValues) => {
      await Promise.resolve(
        props.onChange(String(formValues?.value ?? ""))
      );
    });
  }
  if (typeName.startsWith("Form.") && typeof props.onChange === "function" && sanitizedProps) {
    const actionId = makeId("form-change");
    sanitizedProps.actionId = actionId;
    session.actionHandlers.set(actionId, async (formValues) => {
      const rawValue = formValues?.value;
      const value = typeName === "Form.Checkbox" ? rawValue === true : typeName === "Form.FilePicker" || typeName === "Form.TagPicker" ? Array.isArray(rawValue) ? rawValue.filter((entry) => typeof entry === "string") : [] : typeName === "Form.DatePicker" ? typeof rawValue === "string" && rawValue ? /* @__PURE__ */ new Date(`${rawValue}T00:00:00`) : null : String(rawValue ?? "");
      await Promise.resolve(props.onChange(value));
    });
  }
  if (hasSearchTextChangeHandler && sanitizedProps) {
    sanitizedProps.__hasServerSearch = true;
  }
  if ((typeName === "List" || typeName === "Grid") && props.pagination && typeof props.pagination === "object") {
    const pagination = props.pagination;
    if (typeof pagination.onLoadMore === "function") {
      session.serverLoadMoreHandler = pagination.onLoadMore;
      session.serverHasMore = pagination.hasMore === true;
    }
  }
  const isSearchCollection = typeName === "List" || typeName === "Grid";
  const listItemsTruncated = isSearchCollection ? { value: false } : options.listItemsTruncated;
  const childOptions = isSearchCollection ? {
    ...options,
    listItemsSeen: { count: 0 },
    listItemLimit: session.listItemLimit,
    listItemsTruncated
  } : typeName === "List.Section" || typeName === "Grid.Section" ? {
    ...options,
    listItemsSeen: options.listItemsSeen ?? { count: 0 },
    listItemLimit: session.listItemLimit
  } : options;
  const children = walkRuntimeNodes(props.children, session, depth + 1, budget, childOptions);
  if (isSearchCollection && sanitizedProps && listItemsTruncated) {
    sanitizedProps.__hasMore = session.serverHasMore || listItemsTruncated.value && session.listItemLimit < RUNTIME_COMPONENT_LIMIT;
    sanitizedProps.__pageSize = session.listItemLimit;
  }
  const node = {
    type: typeName,
    props: sanitizedProps,
    children,
    metadata: metadataNodes[0]
  };
  return [node];
}
function formatFeedback(feedback) {
  if (!feedback) return void 0;
  if (feedback.kind === "hud") {
    return feedback.message || void 0;
  }
  const title = feedback.title?.trim() || "";
  const message = feedback.message?.trim() || "";
  if (title && message) return `${title}: ${message}`;
  return title || message || void 0;
}
function renderCurrentView(session) {
  const startedAt = Date.now();
  console.log(
    `[Runner] renderCurrentView start ${session.extensionId}/${session.commandName}; stack=${session.stack.length}, limit=${session.listItemLimit}`
  );
  const top = session.stack.at(-1);
  if (!top) {
    return {
      ok: false,
      message: "No view is available for this extension session."
    };
  }
  session.actionHandlers.clear();
  session.currentActions = [];
  session.renderErrors = [];
  session.serverLoadMoreHandler = null;
  session.serverHasMore = false;
  const budget = { remaining: RUNTIME_COMPONENT_LIMIT };
  const internalTop = top;
  const nodes = typeof internalTop.type === "string" && internalTop.type.startsWith("Tezbar.") ? [top] : walkRuntimeNodes(top, session, 0, budget);
  console.log(
    `[Runner] walkRuntimeNodes complete after ${elapsedMs(startedAt)}; nodes=${nodes.length}, budgetUsed=${RUNTIME_COMPONENT_LIMIT - budget.remaining}, actions=${session.currentActions.length}`
  );
  if (session.renderErrors.length > 0) {
    session.pendingEffects = [];
    return {
      ok: false,
      message: `Extension render failed: ${session.renderErrors.join("; ")}`
    };
  }
  flushPendingEffects(session);
  const root = nodes[0] ?? {
    type: "Detail",
    props: { markdown: "This extension returned an empty view." },
    children: []
  };
  attachRuntimeRootMetadata(root, session);
  console.log(
    `[Runner] renderCurrentView complete after ${elapsedMs(startedAt)}; root=${root.type}, children=${root.children?.length ?? 0}`
  );
  return {
    ok: true,
    mode: "view",
    message: formatFeedback(session.feedback.at(-1)),
    sessionId: session.id,
    extensionId: session.extensionId,
    commandName: session.commandName,
    title: session.title,
    root,
    actions: [...session.currentActions],
    effects: [...session.effects]
  };
}
async function rerenderSessionCommand(session, label) {
  if (!session.commandFn) {
    return renderCurrentView(session);
  }
  console.log(`[Runner] ${label}: rerendering ${session.extensionId}/${session.commandName}`);
  const startedAt = Date.now();
  session.hookIndex = 0;
  session.pendingPromises = [];
  session.actionHandlers.clear();
  session.currentActions = [];
  session.feedback = [];
  session.hasStateUpdates = false;
  console.log(`[Runner] ${label}: command function start`);
  const result = await Promise.resolve(session.commandFn({ arguments: session.commandArgs }));
  console.log(
    `[Runner] ${label}: command function complete after ${elapsedMs(startedAt)}; jsx=${isJsxNode(result)}`
  );
  session.stack = isJsxNode(result) ? [result] : [];
  const view = renderCurrentView(session);
  console.log(`[Runner] ${label}: rerender complete after ${elapsedMs(startedAt)}`);
  return view;
}
async function refreshExtensionSession(request2) {
  const sessionId = String(request2.sessionId || "").trim();
  if (!sessionId) {
    return { ok: false, message: "sessionId is required." };
  }
  const session = sessions.get(sessionId);
  if (!session) {
    return { ok: false, message: "Extension session not found." };
  }
  const inFlight = [...session.promiseCache.values()].filter((entry) => entry.promise).map(
    (entry) => `${entry.label ?? "unknown"} age=${entry.startedAt ? elapsedMs(entry.startedAt) : "?"}`
  );
  console.log(
    `[Runner] Refresh request ${session.extensionId}/${session.commandName}; stateUpdates=${session.hasStateUpdates}, inFlight=${inFlight.length}${inFlight.length ? `
  ${inFlight.join("\n  ")}` : ""}`
  );
  if (!session.hasStateUpdates) return { ok: true, mode: "unchanged" };
  return rerenderSessionCommand(session, "Refresh");
}
async function loadMoreExtensionSession(request2) {
  const sessionId = String(request2.sessionId || "").trim();
  const session = sessions.get(sessionId);
  if (!session) return { ok: false, message: "Extension session not found." };
  if (session.serverLoadMoreHandler) {
    if (!session.serverHasMore) return { ok: true, mode: "unchanged" };
    if (session.serverLoadMoreRequest) return session.serverLoadMoreRequest;
    const request3 = (async () => {
      try {
        await session.serverLoadMoreHandler?.();
        return await rerenderSessionCommand(session, "Load more");
      } catch (error) {
        return {
          ok: false,
          message: error instanceof Error ? error.message : String(error)
        };
      } finally {
        session.serverLoadMoreRequest = null;
      }
    })();
    session.serverLoadMoreRequest = request3;
    return request3;
  }
  const nextLimit = Math.min(session.listItemLimit + LIST_ITEM_PAGE_SIZE, RUNTIME_COMPONENT_LIMIT);
  if (nextLimit === session.listItemLimit) return { ok: true, mode: "unchanged" };
  session.listItemLimit = nextLimit;
  return rerenderSessionCommand(session, "Load more");
}
async function updateSearchText(request2) {
  const sessionId = String(request2.sessionId || "").trim();
  const searchText = String(request2.searchText ?? "");
  if (!sessionId) {
    return { ok: false, message: "sessionId is required." };
  }
  const session = sessions.get(sessionId);
  if (!session) {
    return { ok: false, message: "Extension session not found." };
  }
  session.searchText = searchText;
  session.listItemLimit = searchText.trim() && !session.searchTextChangeHandler ? RUNTIME_COMPONENT_LIMIT : LIST_ITEM_PAGE_SIZE;
  if (session.searchTextChangeHandler) {
    try {
      session.searchTextChangeHandler(searchText);
    } catch (error) {
      return {
        ok: false,
        message: error instanceof Error ? error.message : String(error)
      };
    }
  }
  if (session.commandFn) {
    try {
      const execArgs = { arguments: session.commandArgs };
      let result;
      const searchPass = async (label) => {
        console.log(
          `[Runner] ${label}: executing ${session.extensionId}/${session.commandName} search="${searchText}"`
        );
        session.hookIndex = 0;
        session.pendingPromises = [];
        session.actionHandlers.clear();
        session.currentActions = [];
        session.feedback = [];
        session.hasStateUpdates = false;
        const r = await Promise.resolve(session.commandFn(execArgs));
        console.log(
          `[Runner] ${label} complete: ${session.pendingPromises.length} promises, ${session.hookStates.length} states, stateUpdates=${session.hasStateUpdates}`
        );
        return r;
      };
      for (let p = 1; p <= SEARCH_TEXT_RENDER_PASSES; p += 1) {
        result = await searchPass(`Search Pass ${p}`);
        if (!session.hasStateUpdates) break;
      }
      const searchInFlight = [...session.promiseCache.values()].filter(
        (entry) => entry.promise
      ).length;
      if (searchInFlight > 0) {
        console.log(
          `[Runner] Search render returned with ${searchInFlight} in-flight promises; refresh will continue.`
        );
      }
      session.stack = isJsxNode(result) ? [result] : [];
      if (session.stack.length > 0) {
        return renderCurrentView(session);
      }
    } catch (error) {
      return {
        ok: false,
        message: error instanceof Error ? error.message : String(error)
      };
    }
  }
  return {
    ok: true,
    mode: "no-view",
    message: formatFeedback(session.feedback.at(-1)) || ""
  };
}
function cleanupRuntimeSession(session) {
  if (session.disposed) return;
  session.disposed = true;
  for (const controller of session.abortControllers) {
    controller.abort();
  }
  session.abortControllers.clear();
  for (const cleanup of session.effectCleanups.values()) {
    try {
      cleanup();
    } catch (error) {
      console.warn("[Runner] Extension effect cleanup failed:", error);
    }
  }
  session.effectCleanups.clear();
  session.effectDeps.clear();
  session.promiseCache.clear();
  session.promiseKeysByHook.clear();
  session.promisePaginationByHook.clear();
  session.cacheRecoveryKeys.clear();
}
function deleteRuntimeSession(sessionId) {
  const session = sessions.get(sessionId);
  if (!session) return false;
  cleanupRuntimeSession(session);
  return sessions.delete(sessionId);
}
function pruneSessions() {
  if (sessions.size <= SESSIONS_SOFT_LIMIT) return;
  const ids = [...sessions.keys()];
  const overflow = sessions.size - SESSIONS_SOFT_LIMIT;
  for (let i = 0; i < overflow; i += 1) {
    const id = ids[i];
    if (id) deleteRuntimeSession(id);
  }
}
function runBundle(code, packageRoot, session) {
  const fileRequire = (0, import_node_module.createRequire)((0, import_node_path4.join)(packageRoot, "package.json"));
  const jsxRuntimeShim = createJsxRuntimeShim();
  const reactShim = createReactShim(session);
  const raycastApiShim = createRaycastApiShim(session);
  const raycastUtilsShim = createRaycastUtilsShim(session);
  const customRequire = (specifier) => {
    if (specifier === "@raycast/api") return raycastApiShim;
    if (specifier === "@raycast/utils") return raycastUtilsShim;
    if (specifier === "react") return reactShim;
    if (specifier === "react/jsx-runtime" || specifier === "react/jsx-dev-runtime") {
      return jsxRuntimeShim;
    }
    if (specifier === "child_process" || specifier === "node:child_process") {
      const childProcessModule = fileRequire(specifier);
      const originalExec = childProcessModule.exec;
      return {
        ...childProcessModule,
        exec: ((command, ...args) => {
          const instrumentedCommand = instrumentTimerNotificationCommand(command);
          if (instrumentedCommand === command) {
            return Reflect.apply(originalExec, childProcessModule, [command, ...args]);
          }
          let callbackIndex = -1;
          for (let index = args.length - 1; index >= 0; index -= 1) {
            if (typeof args[index] === "function") {
              callbackIndex = index;
              break;
            }
          }
          if (callbackIndex >= 0) {
            const callback = args[callbackIndex];
            args[callbackIndex] = (error, stdout, stderr) => callback(
              error,
              stripTimerNotificationMarkers(stdout),
              stripTimerNotificationMarkers(stderr)
            );
          }
          const child = Reflect.apply(originalExec, childProcessModule, [
            instrumentedCommand,
            ...args
          ]);
          let markerBuffer = "";
          child.stdout?.on("data", (chunk) => {
            markerBuffer += Buffer.isBuffer(chunk) ? chunk.toString("utf8") : String(chunk);
            let newlineIndex = markerBuffer.indexOf("\n");
            while (newlineIndex >= 0) {
              const line = markerBuffer.slice(0, newlineIndex);
              markerBuffer = markerBuffer.slice(newlineIndex + 1);
              const payload = timerNotificationPayload(line);
              if (payload) {
                process.stdout.write(
                  `${JSON.stringify({ type: "timer_notification", ...payload })}
`
                );
              }
              newlineIndex = markerBuffer.indexOf("\n");
            }
          });
          return child;
        })
        // Preserve native spawn stream semantics. Trimming/splitting stdout
        // corrupts binary output and line-oriented lsof/netstat parsers.
      };
    }
    if (specifier === "raycast-cross-extension") {
      return {
        callbackLaunchCommand: async () => {
        },
        launchCommand: async () => {
        }
      };
    }
    if (specifier === "sha256-file") {
      return (filename, callback) => {
        try {
          const sum = (0, import_node_crypto2.createHash)("sha256").update((0, import_node_fs3.readFileSync)(filename)).digest("hex");
          callback(null, sum);
        } catch (error) {
          callback(error instanceof Error ? error : new Error(String(error)));
        }
      };
    }
    if (specifier === "axios") {
      const createAxiosShim = (instanceConfig = {}) => {
        const commonHeaders = {};
        const defaults = {
          ...instanceConfig,
          headers: {
            common: commonHeaders,
            ...instanceConfig.headers ?? {}
          }
        };
        const execute = async (requestConfig) => {
          const merged = { ...instanceConfig, ...requestConfig };
          const rawUrl = String(merged.url ?? "");
          const url = new URL(rawUrl, merged.baseURL || instanceConfig.baseURL);
          for (const [key, value] of Object.entries(merged.params ?? {})) {
            if (value !== void 0 && value !== null) url.searchParams.set(key, String(value));
          }
          const headers = new Headers({
            ...instanceConfig.headers ?? {},
            ...commonHeaders,
            ...requestConfig.headers ?? {}
          });
          const method = String(merged.method ?? "GET").toUpperCase();
          let body;
          if (merged.data !== void 0 && method !== "GET" && method !== "HEAD") {
            if (typeof merged.data === "string" || merged.data instanceof ArrayBuffer || ArrayBuffer.isView(merged.data) || typeof FormData !== "undefined" && merged.data instanceof FormData) {
              body = merged.data;
            } else {
              body = JSON.stringify(merged.data);
              if (!headers.has("content-type")) headers.set("content-type", "application/json");
            }
          }
          const response = await fetch(url, {
            method,
            headers,
            body
          });
          const responseHeaders = {};
          response.headers.forEach((value, key) => {
            responseHeaders[key] = value;
          });
          let data;
          if (merged.responseType === "stream") {
            data = response.body ? import_node_stream.Readable.fromWeb(response.body) : import_node_stream.Readable.from([]);
          } else {
            const text = await response.text();
            data = text;
            try {
              data = text ? JSON.parse(text) : null;
            } catch {
            }
          }
          const result = {
            data,
            status: response.status,
            statusText: response.statusText,
            headers: responseHeaders,
            config: merged
          };
          if (!response.ok) {
            const error = new Error(
              `Request failed with status code ${response.status}`
            );
            error.response = result;
            error.config = merged;
            throw error;
          }
          return result;
        };
        const instance = ((config) => execute(config));
        Object.assign(instance, {
          get: (url, config) => execute({ ...config, url, method: "GET" }),
          delete: (url, config) => execute({ ...config, url, method: "DELETE" }),
          post: (url, data, config) => execute({ ...config, url, data, method: "POST" }),
          put: (url, data, config) => execute({ ...config, url, data, method: "PUT" }),
          patch: (url, data, config) => execute({ ...config, url, data, method: "PATCH" }),
          request: execute,
          defaults,
          interceptors: {
            request: { use: () => 0, eject: () => {
            } },
            response: { use: () => 0, eject: () => {
            } }
          },
          create: createAxiosShim,
          __esModule: true
        });
        instance.default = instance;
        return instance;
      };
      return createAxiosShim();
    }
    if (specifier === "node-fetch" || specifier === "cross-fetch") {
      return createFetchModuleShim();
    }
    if (specifier === "undici") {
      class ProxyAgent {
        uri;
        constructor(uri) {
          this.uri = uri;
        }
      }
      const request2 = async (url, options) => {
        const response = await fetch(url, {
          method: options?.method,
          body: options?.body,
          headers: options?.headers
        });
        const headers = Object.fromEntries(response.headers.entries());
        return {
          statusCode: response.status,
          headers,
          body: {
            text: () => response.clone().text(),
            json: () => response.clone().json(),
            arrayBuffer: () => response.clone().arrayBuffer()
          }
        };
      };
      return {
        request: request2,
        fetch,
        ProxyAgent,
        default: { request: request2, fetch, ProxyAgent },
        __esModule: true
      };
    }
    if (specifier === "tar") {
      const extract = async (options) => {
        if (!options?.file || !options.cwd) throw new Error("tar.extract requires file and cwd");
        (0, import_node_fs3.mkdirSync)(options.cwd, { recursive: true });
        const args = ["-xzf", options.file, "-C", options.cwd];
        try {
          if (typeof options.filter === "function") {
            if (options.filter("speedtest")) args.push("speedtest");
          } else if (options.filter === "speedtest") {
            args.push("speedtest");
          }
        } catch {
        }
        await execFileAsync3(process.platform === "win32" ? "tar.exe" : "/usr/bin/tar", args);
      };
      return {
        extract,
        x: extract,
        default: { extract, x: extract },
        __esModule: true
      };
    }
    if (specifier === "extract-zip") {
      const extractZip = async (file, options) => {
        const dir = options?.dir;
        if (!dir) throw new Error("extract-zip requires dir");
        (0, import_node_fs3.mkdirSync)(dir, { recursive: true });
        if (process.platform === "win32") {
          await execFileAsync3(
            "powershell.exe",
            [
              "-NoLogo",
              "-NoProfile",
              "-NonInteractive",
              "-Command",
              "Expand-Archive -LiteralPath $env:TEZBAR_ZIP_FILE -DestinationPath $env:TEZBAR_ZIP_DIR -Force"
            ],
            {
              windowsHide: true,
              env: { ...process.env, TEZBAR_ZIP_FILE: file, TEZBAR_ZIP_DIR: dir }
            }
          );
        } else {
          await execFileAsync3("/usr/bin/unzip", ["-o", file, "-d", dir]);
        }
      };
      return {
        default: extractZip,
        __esModule: true
      };
    }
    if (specifier.startsWith("swift:") || specifier.startsWith("rust:")) {
      const pickColor = async () => {
        if (session.commandName === "color-wheel") return null;
        const picked = await pickColorWithNativeSampler();
        session.pickedColor = picked;
        return picked;
      };
      const recognizeText = async (fullscreen = false, keepImage = false, fast = false, languageCorrection = false, ignoreLineBreaks = false, customWordsList = [], languages = [], playSound = false) => runScreenOcrHelper("recognize-text", {
        fullscreen,
        keepImage,
        fast,
        languageCorrection,
        ignoreLineBreaks,
        customWordsList,
        languages,
        playSound
      });
      const detectBarcode = async (keepImage = false, playSound = false) => runScreenOcrHelper("detect-barcode", { keepImage, playSound });
      return {
        pickColor,
        pick_color: pickColor,
        extractColor: extractColorsFromImage,
        extract_color: extractColorsFromImage,
        recognizeText,
        recognize_text: recognizeText,
        detectBarcode,
        detect_barcode: detectBarcode
      };
    }
    if (specifier.startsWith("./") || specifier.startsWith("../") || specifier.startsWith("/")) {
      return fileRequire(specifier);
    }
    if (specifier.startsWith("node:") || BUILTIN_SET.has(specifier)) {
      return fileRequire(specifier);
    }
    return fileRequire(specifier);
  };
  const webGlobals = globalThis;
  const loggedFetch = createLoggedFetch();
  const context = import_node_vm.default.createContext({
    console,
    Buffer,
    process,
    fetch: loggedFetch,
    AbortController: webGlobals.AbortController,
    AbortSignal: webGlobals.AbortSignal,
    Headers: webGlobals.Headers,
    Request: webGlobals.Request,
    Response: webGlobals.Response,
    ReadableStream: webGlobals.ReadableStream ?? import_web.ReadableStream,
    TransformStream: webGlobals.TransformStream ?? import_web.TransformStream,
    WritableStream: webGlobals.WritableStream ?? import_web.WritableStream,
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
    setImmediate,
    clearImmediate,
    TextEncoder,
    TextDecoder,
    Blob,
    File,
    FormData,
    Event,
    EventTarget,
    DOMException,
    MessageChannel,
    MessagePort,
    BroadcastChannel,
    crypto: globalThis.crypto,
    performance: globalThis.performance,
    structuredClone,
    atob,
    btoa,
    URL,
    URLSearchParams
  });
  context.global = context;
  context.globalThis = context;
  context.window = context;
  const runtimeCode = code.replace(
    /\bimport\(\s*(["'])(swift:[^"']+|rust:[^"']+)\1\s*\)/g,
    (_match, quote, specifier) => `Promise.resolve(require(${quote}${specifier}${quote}))`
  );
  const wrapped = `(function(exports, require, module, __filename, __dirname) {
${runtimeCode}
})`;
  const script = new import_node_vm.default.Script(wrapped, {
    filename: (0, import_node_path4.join)(packageRoot, ".tezbar-runtime-bundle.cjs")
  });
  const fn = script.runInContext(context);
  const mod = { exports: {} };
  fn(mod.exports, customRequire, mod, (0, import_node_path4.join)(packageRoot, ".tezbar-runtime-bundle.cjs"), packageRoot);
  return mod.exports;
}
function getCommandExport(moduleExports) {
  if (typeof moduleExports === "function") {
    return moduleExports;
  }
  if (moduleExports && typeof moduleExports === "object") {
    const exp = moduleExports;
    if (typeof exp.default === "function") {
      return exp.default;
    }
  }
  return null;
}
function manifestPreferenceDefaults(pkg, command) {
  const values = {};
  const apply = (preferences) => {
    for (const preference of preferences ?? []) {
      if (!preference?.name) continue;
      if (preference.default !== void 0) {
        const value = preference.default;
        const platform = process.platform === "win32" ? "Windows" : "macOS";
        values[preference.name] = value && typeof value === "object" && !Array.isArray(value) && ("macOS" in value || "Windows" in value) ? value[platform] ?? value.macOS ?? value.Windows : value;
      } else if (preference.type === "checkbox") values[preference.name] = false;
      else if (preference.type === "dropdown")
        values[preference.name] = preference.data?.[0]?.value ?? "";
      else values[preference.name] = "";
    }
  };
  apply(Array.isArray(pkg.preferences) ? pkg.preferences : []);
  apply(Array.isArray(command?.preferences) ? command.preferences : []);
  return values;
}
async function runCommandFromPackagePath(packageJsonPath, extensionId, commandName, argumentValues, preferenceValues, options) {
  const packageRoot = (0, import_node_path4.dirname)(packageJsonPath);
  (0, import_node_fs3.mkdirSync)((0, import_node_path4.join)(packageRoot, ".tezbar-support"), { recursive: true });
  const pkg = parsePackageJson(packageJsonPath);
  const command = findCommandInManifest(pkg, commandName);
  const mode = String(command.mode || "").toLowerCase();
  const title = String(command.title || commandName);
  const entryPath = resolveCommandEntry(packageRoot, commandName, command);
  console.log(`[Runner] Mode=${mode}, title="${title}", entry=${entryPath}`);
  const bundled = await bundleCommand(entryPath, packageRoot);
  console.log(`[Runner] Bundle size: ${bundled.length} chars`);
  const session = {
    id: makeId("ext-session"),
    extensionId,
    commandName,
    commandMode: mode || "view",
    title,
    packageRoot,
    actionHandlers: /* @__PURE__ */ new Map(),
    currentActions: [],
    feedback: [],
    effects: [],
    effectMode: options?.effectMode ?? "system",
    stack: [],
    preferences: preferenceValues ?? {
      ...manifestPreferenceDefaults(pkg, command),
      ...getExtensionPreferences(extensionId, commandName)
    },
    searchTextChangeHandler: null,
    commandFn: null,
    commandArgs: argumentValues,
    bundledCode: bundled,
    searchText: "",
    hookStates: [],
    hookIndex: 0,
    pendingPromises: [],
    promiseCache: /* @__PURE__ */ new Map(),
    promiseKeysByHook: /* @__PURE__ */ new Map(),
    promisePaginationByHook: /* @__PURE__ */ new Map(),
    serverLoadMoreHandler: null,
    serverHasMore: false,
    serverLoadMoreRequest: null,
    cacheRecoveryKeys: /* @__PURE__ */ new Set(),
    abortControllers: /* @__PURE__ */ new Set(),
    effectCleanups: /* @__PURE__ */ new Map(),
    effectDeps: /* @__PURE__ */ new Map(),
    pendingEffects: [],
    hasStateUpdates: false,
    disposed: false,
    listItemLimit: LIST_ITEM_PAGE_SIZE,
    hookStateSnapshot: null,
    pickedColor: null,
    renderErrors: []
  };
  if (preferenceValues === void 0 && shouldShowExtensionPreferenceSetup(extensionId, commandName)) {
    session.stack = [buildPreferenceSetupRoot(extensionId, commandName)];
    sessions.set(session.id, session);
    pruneSessions();
    return renderCurrentView(session);
  }
  const moduleExports = runBundle(bundled, packageRoot, session);
  const commandFn = getCommandExport(moduleExports);
  if (!commandFn) {
    return { ok: false, message: "Extension command entry is not executable." };
  }
  session.commandFn = commandFn;
  const execArgs = { arguments: argumentValues };
  let result;
  const executePass = async (passLabel) => {
    console.log(`[Runner] ${passLabel}: executing ${extensionId}/${commandName}`);
    session.hookIndex = 0;
    session.pendingPromises = [];
    session.actionHandlers.clear();
    session.currentActions = [];
    session.feedback = [];
    session.hasStateUpdates = false;
    result = await Promise.resolve(commandFn(execArgs));
    console.log(
      `[Runner] ${passLabel} complete: ${session.pendingPromises.length} promises, ${session.hookStates.length} hook states, stateUpdates=${session.hasStateUpdates}`
    );
  };
  for (let p = 1; p <= INITIAL_RENDER_PASSES; p += 1) {
    await executePass(`Pass ${p}`);
    if (!session.hasStateUpdates) break;
  }
  const remainingInFlight = [...session.promiseCache.values()].filter(
    (entry) => entry.promise
  ).length;
  if (remainingInFlight > 0) {
    console.log(
      `[Runner] Initial multi-pass exited with ${remainingInFlight} in-flight promises; polling refresh will pick them up.`
    );
  }
  if (commandName === "pick-color" && session.pickedColor) {
    session.title = "Color Wheel";
    session.stack = [
      {
        __jsx: true,
        type: makeToken("Detail"),
        props: {
          markdown: colorWheelMarkdown(),
          initialColor: session.pickedColor
        }
      }
    ];
    sessions.set(session.id, session);
    pruneSessions();
    return renderCurrentView(session);
  }
  if (mode === "no-view" || !isJsxNode(result)) {
    flushPendingEffects(session);
    const message = formatFeedback(session.feedback.at(-1)) || "";
    return {
      ok: true,
      mode: "no-view",
      message,
      effects: [...session.effects]
    };
  }
  session.stack = [result];
  sessions.set(session.id, session);
  pruneSessions();
  return renderCurrentView(session);
}
async function runExtensionCommand(request2) {
  const extensionId = String(request2.extensionId || "").trim();
  const commandName = String(request2.commandName || "").trim();
  console.log(`[Runner] runExtensionCommand called: ${extensionId}/${commandName}`);
  if (!extensionId || !commandName) {
    return { ok: false, message: "Extension id and command name are required." };
  }
  const packagePath = resolveInstalledPackageJsonPath(extensionId);
  if (!packagePath) {
    console.error(`[Runner] Extension not installed: ${extensionId}`);
    return { ok: false, message: `Extension is not installed: ${extensionId}` };
  }
  console.log(`[Runner] Found package.json at ${packagePath}`);
  for (const [sessionId, session] of sessions) {
    if (session.extensionId === extensionId && session.commandName === commandName) {
      deleteRuntimeSession(sessionId);
    }
  }
  try {
    return await runCommandFromPackagePath(
      packagePath,
      extensionId,
      commandName,
      request2.argumentValues ?? {}
    );
  } catch (error) {
    return {
      ok: false,
      message: error instanceof Error ? error.message : String(error)
    };
  }
}
async function runExtensionCommandFromPackageJson(packageJsonPath, commandName, argumentValues, preferenceValues, options) {
  const normalizedPath = String(packageJsonPath || "").trim();
  const normalizedCommandName = String(commandName || "").trim();
  if (!normalizedPath || !normalizedCommandName) {
    return { ok: false, message: "packageJsonPath and commandName are required." };
  }
  const extensionId = `raycast.${(0, import_node_path4.basename)((0, import_node_path4.dirname)(normalizedPath)) || "external"}`;
  try {
    return await runCommandFromPackagePath(
      normalizedPath,
      extensionId,
      normalizedCommandName,
      argumentValues ?? {},
      preferenceValues,
      options
    );
  } catch (error) {
    return {
      ok: false,
      message: error instanceof Error ? error.message : String(error)
    };
  }
}
async function invokeExtensionAction(request2) {
  const sessionId = String(request2.sessionId || "").trim();
  const actionId = String(request2.actionId || "").trim();
  if (!sessionId || !actionId) {
    return { ok: false, message: "sessionId and actionId are required." };
  }
  const session = sessions.get(sessionId);
  if (!session) {
    return { ok: false, message: "Extension session not found." };
  }
  if (actionId === "__nav_pop__") {
    if (session.stack.length > 1) {
      session.stack.pop();
    }
    return renderCurrentView(session);
  }
  const handler = session.actionHandlers.get(actionId);
  if (!handler) {
    return { ok: false, message: "Action is no longer available in this session." };
  }
  try {
    const stackDepthBefore = session.stack.length;
    await Promise.resolve(handler(request2.formValues ?? {}));
    if (session.stack.length !== stackDepthBefore) {
      return renderCurrentView(session);
    }
    if (session.commandFn && session.hasStateUpdates) {
      return rerenderSessionCommand(session, "Action");
    }
    if (session.stack.length > 0) {
      return renderCurrentView(session);
    }
    return {
      ok: true,
      mode: "no-view",
      message: formatFeedback(session.feedback.at(-1)) || "",
      effects: [...session.effects]
    };
  } catch (error) {
    return {
      ok: false,
      message: error instanceof Error ? error.message : String(error)
    };
  }
}
function disposeExtensionSession(sessionId) {
  return deleteRuntimeSession(sessionId);
}
function clearAllExtensionSessions() {
  for (const session of sessions.values()) {
    cleanupRuntimeSession(session);
  }
  sessions.clear();
}
var import_node_fs3, import_promises, import_node_child_process2, import_node_crypto2, import_node_http, import_node_os2, import_node_module, import_node_path4, import_node_stream, import_web, import_node_util2, import_node_v8, import_node_zlib, import_node_vm, TIMER_NOTIFICATION_MARKER, RUNTIME_COMPONENT_LIMIT, RUNTIME_RECURSION_LIMIT, SESSIONS_SOFT_LIMIT, INITIAL_RENDER_PASSES, SEARCH_TEXT_RENDER_PASSES, LIST_ITEM_PAGE_SIZE, APPLICATIONS_CACHE_TTL_MS, PROMISE_RESULT_CACHE_TTL_MS, PROMISE_RESULT_MEMORY_CACHE_LIMIT, BUILTIN_SET, JSX_FRAGMENT, REACT_CONTEXT, execFileAsync3, gzipAsync, IMAGE_MASK, sessions, promiseResultMemoryCache, applicationsCache, iconProxy;
var init_extension_runner = __esm({
  "src/main/extension-runner.ts"() {
    "use strict";
    init_desktop();
    import_node_fs3 = require("node:fs");
    import_promises = require("node:fs/promises");
    import_node_child_process2 = require("node:child_process");
    init_extension_sql();
    init_clipboard();
    init_extension_confirm_alert();
    init_powershell_script();
    init_services();
    import_node_crypto2 = require("node:crypto");
    import_node_http = require("node:http");
    import_node_os2 = require("node:os");
    import_node_module = require("node:module");
    import_node_path4 = require("node:path");
    import_node_stream = require("node:stream");
    import_web = require("node:stream/web");
    import_node_util2 = require("node:util");
    import_node_v8 = require("node:v8");
    import_node_zlib = require("node:zlib");
    import_node_vm = __toESM(require("node:vm"));
    init_esbuild_runtime();
    init_services();
    init_services();
    init_windowState();
    init_extension_registry();
    TIMER_NOTIFICATION_MARKER = "__TEZBAR_TIMER_NOTIFICATION__:";
    RUNTIME_COMPONENT_LIMIT = 1e4;
    RUNTIME_RECURSION_LIMIT = 80;
    SESSIONS_SOFT_LIMIT = 30;
    INITIAL_RENDER_PASSES = 1;
    SEARCH_TEXT_RENDER_PASSES = 1;
    LIST_ITEM_PAGE_SIZE = 30;
    APPLICATIONS_CACHE_TTL_MS = 3e4;
    PROMISE_RESULT_CACHE_TTL_MS = 24 * 60 * 60 * 1e3;
    PROMISE_RESULT_MEMORY_CACHE_LIMIT = 200;
    BUILTIN_SET = new Set(import_node_module.builtinModules);
    JSX_FRAGMENT = /* @__PURE__ */ Symbol.for("tezbar.jsx.fragment");
    REACT_CONTEXT = /* @__PURE__ */ Symbol.for("react.context");
    execFileAsync3 = (0, import_node_util2.promisify)(import_node_child_process2.execFile);
    gzipAsync = (0, import_node_util2.promisify)(import_node_zlib.gzip);
    IMAGE_MASK = {
      Circle: "circle",
      RoundedRectangle: "roundedRectangle"
    };
    sessions = /* @__PURE__ */ new Map();
    promiseResultMemoryCache = /* @__PURE__ */ new Map();
    applicationsCache = null;
    iconProxy = new Proxy(
      {},
      {
        get: (_target, prop) => String(prop)
      }
    );
  }
});

// node_modules/.pnpm/fuse.js@7.3.0/node_modules/fuse.js/dist/fuse.mjs
function isArray(value) {
  return !Array.isArray ? getTag(value) === "[object Array]" : Array.isArray(value);
}
function baseToString(value) {
  if (typeof value == "string") {
    return value;
  }
  if (typeof value === "bigint") {
    return value.toString();
  }
  const result = value + "";
  return result == "0" && 1 / value == -Infinity ? "-0" : result;
}
function toString(value) {
  return value == null ? "" : baseToString(value);
}
function isString(value) {
  return typeof value === "string";
}
function isNumber(value) {
  return typeof value === "number";
}
function isBoolean(value) {
  return value === true || value === false || isObjectLike(value) && getTag(value) == "[object Boolean]";
}
function isObject(value) {
  return typeof value === "object";
}
function isObjectLike(value) {
  return isObject(value) && value !== null;
}
function isDefined(value) {
  return value !== void 0 && value !== null;
}
function isBlank(value) {
  return !value.trim().length;
}
function getTag(value) {
  return value == null ? value === void 0 ? "[object Undefined]" : "[object Null]" : Object.prototype.toString.call(value);
}
function createKey(key) {
  let path4 = null;
  let id = null;
  let src = null;
  let weight = 1;
  let getFn = null;
  if (isString(key) || isArray(key)) {
    src = key;
    path4 = createKeyPath(key);
    id = createKeyId(key);
  } else {
    if (!hasOwn.call(key, "name")) {
      throw new Error(MISSING_KEY_PROPERTY("name"));
    }
    const name = key.name;
    src = name;
    if (hasOwn.call(key, "weight")) {
      weight = key.weight;
      if (weight <= 0) {
        throw new Error(INVALID_KEY_WEIGHT_VALUE(name));
      }
    }
    path4 = createKeyPath(name);
    id = createKeyId(name);
    getFn = key.getFn;
  }
  return {
    path: path4,
    id,
    weight,
    src,
    getFn
  };
}
function createKeyPath(key) {
  return isArray(key) ? key : key.split(".");
}
function createKeyId(key) {
  return isArray(key) ? key.join(".") : key;
}
function get(obj, path4) {
  const list = [];
  let arr = false;
  const deepGet = (obj2, path5, index, arrayIndex) => {
    if (!isDefined(obj2)) {
      return;
    }
    if (!path5[index]) {
      list.push(arrayIndex !== void 0 ? {
        v: obj2,
        i: arrayIndex
      } : obj2);
    } else {
      const key = path5[index];
      const value = obj2[key];
      if (!isDefined(value)) {
        return;
      }
      if (index === path5.length - 1 && (isString(value) || isNumber(value) || isBoolean(value) || typeof value === "bigint")) {
        list.push(arrayIndex !== void 0 ? {
          v: toString(value),
          i: arrayIndex
        } : toString(value));
      } else if (isArray(value)) {
        arr = true;
        for (let i = 0, len = value.length; i < len; i += 1) {
          deepGet(value[i], path5, index + 1, i);
        }
      } else if (path5.length) {
        deepGet(value, path5, index + 1, arrayIndex);
      }
    }
  };
  deepGet(obj, isString(path4) ? path4.split(".") : path4, 0);
  return arr ? list : list[0];
}
function norm(weight = 1, mantissa = 3) {
  const cache = /* @__PURE__ */ new Map();
  const m = Math.pow(10, mantissa);
  return {
    get(value) {
      const numTokens = value.match(SPACE).length;
      if (cache.has(numTokens)) {
        return cache.get(numTokens);
      }
      const norm2 = 1 / Math.pow(numTokens, 0.5 * weight);
      const n = parseFloat(Math.round(norm2 * m) / m);
      cache.set(numTokens, n);
      return n;
    },
    clear() {
      cache.clear();
    }
  };
}
function createIndex(keys, docs, {
  getFn = Config.getFn,
  fieldNormWeight = Config.fieldNormWeight
} = {}) {
  const myIndex = new FuseIndex({
    getFn,
    fieldNormWeight
  });
  myIndex.setKeys(keys.map(createKey));
  myIndex.setSources(docs);
  myIndex.create();
  return myIndex;
}
function parseIndex(data, {
  getFn = Config.getFn,
  fieldNormWeight = Config.fieldNormWeight
} = {}) {
  const {
    keys,
    records
  } = data;
  const myIndex = new FuseIndex({
    getFn,
    fieldNormWeight
  });
  myIndex.setKeys(keys);
  myIndex.setIndexRecords(records);
  return myIndex;
}
function convertMaskToIndices(matchmask = [], minMatchCharLength = Config.minMatchCharLength) {
  const indices = [];
  let start = -1;
  let end = -1;
  let i = 0;
  for (let len = matchmask.length; i < len; i += 1) {
    const match = matchmask[i];
    if (match && start === -1) {
      start = i;
    } else if (!match && start !== -1) {
      end = i - 1;
      if (end - start + 1 >= minMatchCharLength) {
        indices.push([start, end]);
      }
      start = -1;
    }
  }
  if (matchmask[i - 1] && i - start >= minMatchCharLength) {
    indices.push([start, i - 1]);
  }
  return indices;
}
function search(text, pattern, patternAlphabet, {
  location = Config.location,
  distance = Config.distance,
  threshold = Config.threshold,
  findAllMatches = Config.findAllMatches,
  minMatchCharLength = Config.minMatchCharLength,
  includeMatches = Config.includeMatches,
  ignoreLocation = Config.ignoreLocation
} = {}) {
  if (pattern.length > MAX_BITS) {
    throw new Error(PATTERN_LENGTH_TOO_LARGE(MAX_BITS));
  }
  const patternLen = pattern.length;
  const textLen = text.length;
  const expectedLocation = Math.max(0, Math.min(location, textLen));
  let currentThreshold = threshold;
  let bestLocation = expectedLocation;
  const calcScore = (errors, currentLocation) => {
    const accuracy = errors / patternLen;
    if (ignoreLocation) return accuracy;
    const proximity = Math.abs(expectedLocation - currentLocation);
    if (!distance) return proximity ? 1 : accuracy;
    return accuracy + proximity / distance;
  };
  const computeMatches = minMatchCharLength > 1 || includeMatches;
  const matchMask = computeMatches ? Array(textLen) : [];
  let index;
  while ((index = text.indexOf(pattern, bestLocation)) > -1) {
    const score = calcScore(0, index);
    currentThreshold = Math.min(score, currentThreshold);
    bestLocation = index + patternLen;
    if (computeMatches) {
      let i = 0;
      while (i < patternLen) {
        matchMask[index + i] = 1;
        i += 1;
      }
    }
  }
  bestLocation = -1;
  let lastBitArr = [];
  let finalScore = 1;
  let binMax = patternLen + textLen;
  const mask = 1 << patternLen - 1;
  for (let i = 0; i < patternLen; i += 1) {
    let binMin = 0;
    let binMid = binMax;
    while (binMin < binMid) {
      const score2 = calcScore(i, expectedLocation + binMid);
      if (score2 <= currentThreshold) {
        binMin = binMid;
      } else {
        binMax = binMid;
      }
      binMid = Math.floor((binMax - binMin) / 2 + binMin);
    }
    binMax = binMid;
    let start = Math.max(1, expectedLocation - binMid + 1);
    const finish = findAllMatches ? textLen : Math.min(expectedLocation + binMid, textLen) + patternLen;
    const bitArr = Array(finish + 2);
    bitArr[finish + 1] = (1 << i) - 1;
    for (let j = finish; j >= start; j -= 1) {
      const currentLocation = j - 1;
      const charMatch = patternAlphabet[text[currentLocation]];
      if (computeMatches) {
        matchMask[currentLocation] = +!!charMatch;
      }
      bitArr[j] = (bitArr[j + 1] << 1 | 1) & charMatch;
      if (i) {
        bitArr[j] |= (lastBitArr[j + 1] | lastBitArr[j]) << 1 | 1 | lastBitArr[j + 1];
      }
      if (bitArr[j] & mask) {
        finalScore = calcScore(i, currentLocation);
        if (finalScore <= currentThreshold) {
          currentThreshold = finalScore;
          bestLocation = currentLocation;
          if (bestLocation <= expectedLocation) {
            break;
          }
          start = Math.max(1, 2 * expectedLocation - bestLocation);
        }
      }
    }
    const score = calcScore(i + 1, expectedLocation);
    if (score > currentThreshold) {
      break;
    }
    lastBitArr = bitArr;
  }
  const result = {
    isMatch: bestLocation >= 0,
    // Count exact matches (those with a score of 0) to be "almost" exact
    score: Math.max(1e-3, finalScore)
  };
  if (computeMatches) {
    const indices = convertMaskToIndices(matchMask, minMatchCharLength);
    if (!indices.length) {
      result.isMatch = false;
    } else if (includeMatches) {
      result.indices = indices;
    }
  }
  return result;
}
function createPatternAlphabet(pattern) {
  const mask = {};
  for (let i = 0, len = pattern.length; i < len; i += 1) {
    const char = pattern.charAt(i);
    mask[char] = (mask[char] || 0) | 1 << len - i - 1;
  }
  return mask;
}
function mergeIndices(indices) {
  if (indices.length <= 1) return indices;
  indices.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  const merged = [indices[0]];
  for (let i = 1, len = indices.length; i < len; i += 1) {
    const last = merged[merged.length - 1];
    const curr = indices[i];
    if (curr[0] <= last[1] + 1) {
      last[1] = Math.max(last[1], curr[1]);
    } else {
      merged.push(curr);
    }
  }
  return merged;
}
function getMatch(pattern, exp) {
  const matches = pattern.match(exp);
  return matches ? matches[1] : null;
}
function tokenize(pattern) {
  const tokens = [];
  const len = pattern.length;
  let i = 0;
  while (i < len) {
    while (i < len && pattern[i] === " ") i++;
    if (i >= len) break;
    let j = i;
    while (j < len && pattern[j] !== " " && pattern[j] !== '"') j++;
    if (j < len && pattern[j] === '"') {
      j++;
      while (j < len) {
        if (pattern[j] === '"') {
          const next = j + 1;
          if (next >= len || pattern[next] === " ") {
            j++;
            break;
          }
          if (pattern[next] === "$" && (next + 1 >= len || pattern[next + 1] === " ")) {
            j += 2;
            break;
          }
        }
        j++;
      }
      tokens.push(pattern.substring(i, j));
      i = j;
    } else {
      while (j < len && pattern[j] !== " ") j++;
      tokens.push(pattern.substring(i, j));
      i = j;
    }
  }
  return tokens;
}
function parseQuery(pattern, options = {}) {
  const escaped = pattern.replace(/\\\|/g, ESCAPED_PIPE);
  return escaped.split(OR_TOKEN).map((item) => {
    const restored = item.replace(/\u0000/g, "|");
    const query = tokenize(restored.trim()).filter((item2) => item2 && !!item2.trim());
    const results = [];
    for (let i = 0, len = query.length; i < len; i += 1) {
      const queryItem = query[i];
      let found = false;
      let idx = -1;
      while (!found && ++idx < searchersLen) {
        const searcher = searchers[idx];
        const token = searcher.isMultiMatch(queryItem);
        if (token) {
          results.push(new searcher(token, options));
          found = true;
        }
      }
      if (found) {
        continue;
      }
      idx = -1;
      while (++idx < searchersLen) {
        const searcher = searchers[idx];
        const token = searcher.isSingleMatch(queryItem);
        if (token) {
          results.push(new searcher(token, options));
          break;
        }
      }
    }
    return results;
  });
}
function register(...args) {
  registeredSearchers.push(...args);
}
function createSearcher(pattern, options) {
  for (let i = 0, len = registeredSearchers.length; i < len; i += 1) {
    const searcherClass = registeredSearchers[i];
    if (searcherClass.condition(pattern, options)) {
      return new searcherClass(pattern, options);
    }
  }
  return new BitapSearch(pattern, options);
}
function parse(query, options, {
  auto = true
} = {}) {
  const next = (query2) => {
    if (isString(query2)) {
      const obj = {
        keyId: null,
        pattern: query2
      };
      if (auto) {
        obj.searcher = createSearcher(query2, options);
      }
      return obj;
    }
    const keys = Object.keys(query2);
    const isQueryPath = isPath(query2);
    if (!isQueryPath && keys.length > 1 && !isExpression(query2)) {
      return next(convertToExplicit(query2));
    }
    if (isLeaf(query2)) {
      const key = isQueryPath ? query2[KeyType.PATH] : keys[0];
      const pattern = isQueryPath ? query2[KeyType.PATTERN] : query2[key];
      if (!isString(pattern)) {
        throw new Error(LOGICAL_SEARCH_INVALID_QUERY_FOR_KEY(key));
      }
      const obj = {
        keyId: createKeyId(key),
        pattern
      };
      if (auto) {
        obj.searcher = createSearcher(pattern, options);
      }
      return obj;
    }
    const node = {
      children: [],
      operator: keys[0]
    };
    keys.forEach((key) => {
      const value = query2[key];
      if (isArray(value)) {
        value.forEach((item) => {
          node.children.push(next(item));
        });
      }
    });
    return node;
  };
  if (!isExpression(query)) {
    query = convertToExplicit(query);
  }
  return next(query);
}
function computeScoreSingle(matches, {
  ignoreFieldNorm = Config.ignoreFieldNorm
}) {
  let totalScore = 1;
  matches.forEach(({
    key,
    norm: norm2,
    score
  }) => {
    const weight = key ? key.weight : null;
    totalScore *= Math.pow(score === 0 && weight ? Number.EPSILON : score, (weight || 1) * (ignoreFieldNorm ? 1 : norm2));
  });
  return totalScore;
}
function computeScore(results, {
  ignoreFieldNorm = Config.ignoreFieldNorm
}) {
  results.forEach((result) => {
    result.score = computeScoreSingle(result.matches, {
      ignoreFieldNorm
    });
  });
}
function transformMatches(result, data) {
  const matches = result.matches;
  data.matches = [];
  if (!isDefined(matches)) {
    return;
  }
  matches.forEach((match) => {
    if (!isDefined(match.indices) || !match.indices.length) {
      return;
    }
    const {
      indices,
      value
    } = match;
    const obj = {
      indices,
      value
    };
    if (match.key) {
      obj.key = match.key.src;
    }
    if (match.idx > -1) {
      obj.refIndex = match.idx;
    }
    data.matches.push(obj);
  });
}
function transformScore(result, data) {
  data.score = result.score;
}
function format(results, docs, {
  includeMatches = Config.includeMatches,
  includeScore = Config.includeScore
} = {}) {
  const transformers = [];
  if (includeMatches) transformers.push(transformMatches);
  if (includeScore) transformers.push(transformScore);
  return results.map((result) => {
    const {
      idx
    } = result;
    const data = {
      item: docs[idx],
      refIndex: idx
    };
    if (transformers.length) {
      transformers.forEach((transformer) => {
        transformer(result, data);
      });
    }
    return data;
  });
}
function createAnalyzer({
  isCaseSensitive = false,
  ignoreDiacritics = false
} = {}) {
  return {
    tokenize(text) {
      if (!isCaseSensitive) {
        text = text.toLowerCase();
      }
      if (ignoreDiacritics) {
        text = stripDiacritics(text);
      }
      return text.match(WORD) || [];
    }
  };
}
function buildInvertedIndex(records, keyCount, analyzer) {
  const terms = /* @__PURE__ */ new Map();
  const df = /* @__PURE__ */ new Map();
  let fieldCount = 0;
  function addField(text, docIdx, keyIdx, subIdx) {
    const tokens = analyzer.tokenize(text);
    if (!tokens.length) return;
    fieldCount++;
    const termFreqs = /* @__PURE__ */ new Map();
    for (const token of tokens) {
      termFreqs.set(token, (termFreqs.get(token) || 0) + 1);
    }
    for (const [term, tf] of termFreqs) {
      const posting = {
        docIdx,
        keyIdx,
        subIdx,
        tf
      };
      let postings = terms.get(term);
      if (!postings) {
        postings = [];
        terms.set(term, postings);
      }
      postings.push(posting);
      df.set(term, (df.get(term) || 0) + 1);
    }
  }
  for (const record of records) {
    const {
      i: docIdx,
      v,
      $: fields
    } = record;
    if (v !== void 0) {
      addField(v, docIdx, -1, -1);
      continue;
    }
    if (fields) {
      for (let keyIdx = 0; keyIdx < keyCount; keyIdx++) {
        const value = fields[keyIdx];
        if (!value) continue;
        if (Array.isArray(value)) {
          for (const sub of value) {
            addField(sub.v, docIdx, keyIdx, sub.i ?? -1);
          }
        } else {
          addField(value.v, docIdx, keyIdx, -1);
        }
      }
    }
  }
  return {
    terms,
    fieldCount,
    df
  };
}
function addToInvertedIndex(index, record, keyCount, analyzer) {
  const {
    i: docIdx,
    v,
    $: fields
  } = record;
  function addField(text, keyIdx, subIdx) {
    const tokens = analyzer.tokenize(text);
    if (!tokens.length) return;
    index.fieldCount++;
    const termFreqs = /* @__PURE__ */ new Map();
    for (const token of tokens) {
      termFreqs.set(token, (termFreqs.get(token) || 0) + 1);
    }
    for (const [term, tf] of termFreqs) {
      const posting = {
        docIdx,
        keyIdx,
        subIdx,
        tf
      };
      let postings = index.terms.get(term);
      if (!postings) {
        postings = [];
        index.terms.set(term, postings);
      }
      postings.push(posting);
      index.df.set(term, (index.df.get(term) || 0) + 1);
    }
  }
  if (v !== void 0) {
    addField(v, -1, -1);
    return;
  }
  if (fields) {
    for (let keyIdx = 0; keyIdx < keyCount; keyIdx++) {
      const value = fields[keyIdx];
      if (!value) continue;
      if (Array.isArray(value)) {
        for (const sub of value) {
          addField(sub.v, keyIdx, sub.i ?? -1);
        }
      } else {
        addField(value.v, keyIdx, -1);
      }
    }
  }
}
function removeFromInvertedIndex(index, docIdx) {
  for (const [term, postings] of index.terms) {
    const filtered = postings.filter((p) => p.docIdx !== docIdx);
    const removed = postings.length - filtered.length;
    if (removed > 0) {
      index.fieldCount -= removed;
      index.df.set(term, (index.df.get(term) || 0) - removed);
      if (filtered.length === 0) {
        index.terms.delete(term);
        index.df.delete(term);
      } else {
        index.terms.set(term, filtered);
      }
    }
  }
}
var INCORRECT_INDEX_TYPE, LOGICAL_SEARCH_INVALID_QUERY_FOR_KEY, PATTERN_LENGTH_TOO_LARGE, MISSING_KEY_PROPERTY, INVALID_KEY_WEIGHT_VALUE, hasOwn, KeyStore, MatchOptions, BasicOptions, FuzzyOptions, AdvancedOptions, Config, SPACE, FuseIndex, MAX_BITS, NON_DECOMPOSABLE_MAP, NON_DECOMPOSABLE_RE, stripDiacritics, BitapSearch, BaseMatch, ExactMatch, InverseExactMatch, PrefixExactMatch, InversePrefixExactMatch, SuffixExactMatch, InverseSuffixExactMatch, FuzzyMatch, IncludeMatch, searchers, searchersLen, ESCAPED_PIPE, OR_TOKEN, MultiMatchSet, ExtendedSearch, registeredSearchers, LogicalOperator, KeyType, isExpression, isPath, isLeaf, convertToExplicit, MaxHeap, WORD, Fuse, TokenSearch;
var init_fuse = __esm({
  "node_modules/.pnpm/fuse.js@7.3.0/node_modules/fuse.js/dist/fuse.mjs"() {
    INCORRECT_INDEX_TYPE = "Incorrect 'index' type";
    LOGICAL_SEARCH_INVALID_QUERY_FOR_KEY = (key) => `Invalid value for key ${key}`;
    PATTERN_LENGTH_TOO_LARGE = (max) => `Pattern length exceeds max of ${max}.`;
    MISSING_KEY_PROPERTY = (name) => `Missing ${name} property in key`;
    INVALID_KEY_WEIGHT_VALUE = (key) => `Property 'weight' in key '${key}' must be a positive integer`;
    hasOwn = Object.prototype.hasOwnProperty;
    KeyStore = class {
      constructor(keys) {
        this._keys = [];
        this._keyMap = {};
        let totalWeight = 0;
        keys.forEach((key) => {
          const obj = createKey(key);
          this._keys.push(obj);
          this._keyMap[obj.id] = obj;
          totalWeight += obj.weight;
        });
        this._keys.forEach((key) => {
          key.weight /= totalWeight;
        });
      }
      get(keyId) {
        return this._keyMap[keyId];
      }
      keys() {
        return this._keys;
      }
      toJSON() {
        return JSON.stringify(this._keys);
      }
    };
    MatchOptions = {
      includeMatches: false,
      findAllMatches: false,
      minMatchCharLength: 1
    };
    BasicOptions = {
      isCaseSensitive: false,
      ignoreDiacritics: false,
      includeScore: false,
      keys: [],
      shouldSort: true,
      sortFn: (a, b) => a.score === b.score ? a.idx < b.idx ? -1 : 1 : a.score < b.score ? -1 : 1
    };
    FuzzyOptions = {
      location: 0,
      threshold: 0.6,
      distance: 100
    };
    AdvancedOptions = {
      useExtendedSearch: false,
      useTokenSearch: false,
      getFn: get,
      ignoreLocation: false,
      ignoreFieldNorm: false,
      fieldNormWeight: 1
    };
    Config = Object.freeze({
      ...BasicOptions,
      ...MatchOptions,
      ...FuzzyOptions,
      ...AdvancedOptions
    });
    SPACE = /[^ ]+/g;
    FuseIndex = class {
      constructor({
        getFn = Config.getFn,
        fieldNormWeight = Config.fieldNormWeight
      } = {}) {
        this.norm = norm(fieldNormWeight, 3);
        this.getFn = getFn;
        this.isCreated = false;
        this.docs = [];
        this.keys = [];
        this._keysMap = {};
        this.setIndexRecords();
      }
      setSources(docs = []) {
        this.docs = docs;
      }
      setIndexRecords(records = []) {
        this.records = records;
      }
      setKeys(keys = []) {
        this.keys = keys;
        this._keysMap = {};
        keys.forEach((key, idx) => {
          this._keysMap[key.id] = idx;
        });
      }
      create() {
        if (this.isCreated || !this.docs.length) {
          return;
        }
        this.isCreated = true;
        if (isString(this.docs[0])) {
          this.docs.forEach((doc, docIndex) => {
            this._addString(doc, docIndex);
          });
        } else {
          this.docs.forEach((doc, docIndex) => {
            this._addObject(doc, docIndex);
          });
        }
        this.norm.clear();
      }
      // Adds a doc to the end of the index
      add(doc) {
        const idx = this.size();
        if (isString(doc)) {
          this._addString(doc, idx);
        } else {
          this._addObject(doc, idx);
        }
      }
      // Removes the doc at the specified index of the index
      removeAt(idx) {
        this.records.splice(idx, 1);
        for (let i = idx, len = this.size(); i < len; i += 1) {
          this.records[i].i -= 1;
        }
      }
      // Removes docs at the specified indices (must be sorted ascending)
      removeAll(indices) {
        for (let i = indices.length - 1; i >= 0; i -= 1) {
          this.records.splice(indices[i], 1);
        }
        for (let i = 0, len = this.records.length; i < len; i += 1) {
          this.records[i].i = i;
        }
      }
      getValueForItemAtKeyId(item, keyId) {
        return item[this._keysMap[keyId]];
      }
      size() {
        return this.records.length;
      }
      _addString(doc, docIndex) {
        if (!isDefined(doc) || isBlank(doc)) {
          return;
        }
        const record = {
          v: doc,
          i: docIndex,
          n: this.norm.get(doc)
        };
        this.records.push(record);
      }
      _addObject(doc, docIndex) {
        const record = {
          i: docIndex,
          $: {}
        };
        this.keys.forEach((key, keyIndex) => {
          const value = key.getFn ? key.getFn(doc) : this.getFn(doc, key.path);
          if (!isDefined(value)) {
            return;
          }
          if (isArray(value)) {
            const subRecords = [];
            for (let i = 0, len = value.length; i < len; i += 1) {
              const item = value[i];
              if (!isDefined(item)) {
                continue;
              }
              if (isString(item)) {
                if (!isBlank(item)) {
                  const subRecord = {
                    v: item,
                    i,
                    n: this.norm.get(item)
                  };
                  subRecords.push(subRecord);
                }
              } else if (isDefined(item.v)) {
                const text = isString(item.v) ? item.v : toString(item.v);
                if (!isBlank(text)) {
                  const subRecord = {
                    v: text,
                    i: item.i,
                    n: this.norm.get(text)
                  };
                  subRecords.push(subRecord);
                }
              }
            }
            record.$[keyIndex] = subRecords;
          } else if (isString(value) && !isBlank(value)) {
            const subRecord = {
              v: value,
              n: this.norm.get(value)
            };
            record.$[keyIndex] = subRecord;
          }
        });
        this.records.push(record);
      }
      toJSON() {
        return {
          // eslint-disable-next-line @typescript-eslint/no-unused-vars
          keys: this.keys.map(({
            getFn,
            ...key
          }) => key),
          records: this.records
        };
      }
    };
    MAX_BITS = 32;
    NON_DECOMPOSABLE_MAP = {
      "\u0142": "l",
      // ł
      "\u0141": "L",
      // Ł
      "\u0111": "d",
      // đ
      "\u0110": "D",
      // Đ
      "\xF8": "o",
      // ø
      "\xD8": "O",
      // Ø
      "\u0127": "h",
      // ħ
      "\u0126": "H",
      // Ħ
      "\u0167": "t",
      // ŧ
      "\u0166": "T",
      // Ŧ
      "\u0131": "i",
      // ı
      "\xDF": "ss"
      // ß
    };
    NON_DECOMPOSABLE_RE = new RegExp("[" + Object.keys(NON_DECOMPOSABLE_MAP).join("") + "]", "g");
    stripDiacritics = String.prototype.normalize ? (str) => str.normalize("NFD").replace(/[\u0300-\u036F\u0483-\u0489\u0591-\u05BD\u05BF\u05C1\u05C2\u05C4\u05C5\u05C7\u0610-\u061A\u064B-\u065F\u0670\u06D6-\u06DC\u06DF-\u06E4\u06E7\u06E8\u06EA-\u06ED\u0711\u0730-\u074A\u07A6-\u07B0\u07EB-\u07F3\u07FD\u0816-\u0819\u081B-\u0823\u0825-\u0827\u0829-\u082D\u0859-\u085B\u08D3-\u08E1\u08E3-\u0903\u093A-\u093C\u093E-\u094F\u0951-\u0957\u0962\u0963\u0981-\u0983\u09BC\u09BE-\u09C4\u09C7\u09C8\u09CB-\u09CD\u09D7\u09E2\u09E3\u09FE\u0A01-\u0A03\u0A3C\u0A3E-\u0A42\u0A47\u0A48\u0A4B-\u0A4D\u0A51\u0A70\u0A71\u0A75\u0A81-\u0A83\u0ABC\u0ABE-\u0AC5\u0AC7-\u0AC9\u0ACB-\u0ACD\u0AE2\u0AE3\u0AFA-\u0AFF\u0B01-\u0B03\u0B3C\u0B3E-\u0B44\u0B47\u0B48\u0B4B-\u0B4D\u0B56\u0B57\u0B62\u0B63\u0B82\u0BBE-\u0BC2\u0BC6-\u0BC8\u0BCA-\u0BCD\u0BD7\u0C00-\u0C04\u0C3E-\u0C44\u0C46-\u0C48\u0C4A-\u0C4D\u0C55\u0C56\u0C62\u0C63\u0C81-\u0C83\u0CBC\u0CBE-\u0CC4\u0CC6-\u0CC8\u0CCA-\u0CCD\u0CD5\u0CD6\u0CE2\u0CE3\u0D00-\u0D03\u0D3B\u0D3C\u0D3E-\u0D44\u0D46-\u0D48\u0D4A-\u0D4D\u0D57\u0D62\u0D63\u0D82\u0D83\u0DCA\u0DCF-\u0DD4\u0DD6\u0DD8-\u0DDF\u0DF2\u0DF3\u0E31\u0E34-\u0E3A\u0E47-\u0E4E\u0EB1\u0EB4-\u0EB9\u0EBB\u0EBC\u0EC8-\u0ECD\u0F18\u0F19\u0F35\u0F37\u0F39\u0F3E\u0F3F\u0F71-\u0F84\u0F86\u0F87\u0F8D-\u0F97\u0F99-\u0FBC\u0FC6\u102B-\u103E\u1056-\u1059\u105E-\u1060\u1062-\u1064\u1067-\u106D\u1071-\u1074\u1082-\u108D\u108F\u109A-\u109D\u135D-\u135F\u1712-\u1714\u1732-\u1734\u1752\u1753\u1772\u1773\u17B4-\u17D3\u17DD\u180B-\u180D\u1885\u1886\u18A9\u1920-\u192B\u1930-\u193B\u1A17-\u1A1B\u1A55-\u1A5E\u1A60-\u1A7C\u1A7F\u1AB0-\u1ABE\u1B00-\u1B04\u1B34-\u1B44\u1B6B-\u1B73\u1B80-\u1B82\u1BA1-\u1BAD\u1BE6-\u1BF3\u1C24-\u1C37\u1CD0-\u1CD2\u1CD4-\u1CE8\u1CED\u1CF2-\u1CF4\u1CF7-\u1CF9\u1DC0-\u1DF9\u1DFB-\u1DFF\u20D0-\u20F0\u2CEF-\u2CF1\u2D7F\u2DE0-\u2DFF\u302A-\u302F\u3099\u309A\uA66F-\uA672\uA674-\uA67D\uA69E\uA69F\uA6F0\uA6F1\uA802\uA806\uA80B\uA823-\uA827\uA880\uA881\uA8B4-\uA8C5\uA8E0-\uA8F1\uA8FF\uA926-\uA92D\uA947-\uA953\uA980-\uA983\uA9B3-\uA9C0\uA9E5\uAA29-\uAA36\uAA43\uAA4C\uAA4D\uAA7B-\uAA7D\uAAB0\uAAB2-\uAAB4\uAAB7\uAAB8\uAABE\uAABF\uAAC1\uAAEB-\uAAEF\uAAF5\uAAF6\uABE3-\uABEA\uABEC\uABED\uFB1E\uFE00-\uFE0F\uFE20-\uFE2F]/g, "").replace(NON_DECOMPOSABLE_RE, (ch) => NON_DECOMPOSABLE_MAP[ch]) : (str) => str;
    BitapSearch = class {
      constructor(pattern, {
        location = Config.location,
        threshold = Config.threshold,
        distance = Config.distance,
        includeMatches = Config.includeMatches,
        findAllMatches = Config.findAllMatches,
        minMatchCharLength = Config.minMatchCharLength,
        isCaseSensitive = Config.isCaseSensitive,
        ignoreDiacritics = Config.ignoreDiacritics,
        ignoreLocation = Config.ignoreLocation
      } = {}) {
        this.options = {
          location,
          threshold,
          distance,
          includeMatches,
          findAllMatches,
          minMatchCharLength,
          isCaseSensitive,
          ignoreDiacritics,
          ignoreLocation
        };
        pattern = isCaseSensitive ? pattern : pattern.toLowerCase();
        pattern = ignoreDiacritics ? stripDiacritics(pattern) : pattern;
        this.pattern = pattern;
        this.chunks = [];
        if (!this.pattern.length) {
          return;
        }
        const addChunk = (pattern2, startIndex) => {
          this.chunks.push({
            pattern: pattern2,
            alphabet: createPatternAlphabet(pattern2),
            startIndex
          });
        };
        const len = this.pattern.length;
        if (len > MAX_BITS) {
          let i = 0;
          const remainder = len % MAX_BITS;
          const end = len - remainder;
          while (i < end) {
            addChunk(this.pattern.substr(i, MAX_BITS), i);
            i += MAX_BITS;
          }
          if (remainder) {
            const startIndex = len - MAX_BITS;
            addChunk(this.pattern.substr(startIndex), startIndex);
          }
        } else {
          addChunk(this.pattern, 0);
        }
      }
      searchIn(text) {
        const {
          isCaseSensitive,
          ignoreDiacritics,
          includeMatches
        } = this.options;
        text = isCaseSensitive ? text : text.toLowerCase();
        text = ignoreDiacritics ? stripDiacritics(text) : text;
        if (this.pattern === text) {
          const result2 = {
            isMatch: true,
            score: 0
          };
          if (includeMatches) {
            result2.indices = [[0, text.length - 1]];
          }
          return result2;
        }
        const {
          location,
          distance,
          threshold,
          findAllMatches,
          minMatchCharLength,
          ignoreLocation
        } = this.options;
        const allIndices = [];
        let totalScore = 0;
        let hasMatches = false;
        this.chunks.forEach(({
          pattern,
          alphabet,
          startIndex
        }) => {
          const {
            isMatch,
            score,
            indices
          } = search(text, pattern, alphabet, {
            location: location + startIndex,
            distance,
            threshold,
            findAllMatches,
            minMatchCharLength,
            includeMatches,
            ignoreLocation
          });
          if (isMatch) {
            hasMatches = true;
          }
          totalScore += score;
          if (isMatch && indices) {
            allIndices.push(...indices);
          }
        });
        const result = {
          isMatch: hasMatches,
          score: hasMatches ? totalScore / this.chunks.length : 1
        };
        if (hasMatches && includeMatches) {
          result.indices = mergeIndices(allIndices);
        }
        return result;
      }
    };
    BaseMatch = class {
      constructor(pattern) {
        this.pattern = pattern;
      }
      static isMultiMatch(pattern) {
        return getMatch(pattern, this.multiRegex);
      }
      static isSingleMatch(pattern) {
        return getMatch(pattern, this.singleRegex);
      }
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      search(_text) {
        return {
          isMatch: false,
          score: 1
        };
      }
    };
    ExactMatch = class extends BaseMatch {
      constructor(pattern) {
        super(pattern);
      }
      static get type() {
        return "exact";
      }
      static get multiRegex() {
        return /^="(.*)"$/;
      }
      static get singleRegex() {
        return /^=(.*)$/;
      }
      search(text) {
        const isMatch = text === this.pattern;
        return {
          isMatch,
          score: isMatch ? 0 : 1,
          indices: [0, this.pattern.length - 1]
        };
      }
    };
    InverseExactMatch = class extends BaseMatch {
      constructor(pattern) {
        super(pattern);
      }
      static get type() {
        return "inverse-exact";
      }
      static get multiRegex() {
        return /^!"(.*)"$/;
      }
      static get singleRegex() {
        return /^!(.*)$/;
      }
      search(text) {
        const index = text.indexOf(this.pattern);
        const isMatch = index === -1;
        return {
          isMatch,
          score: isMatch ? 0 : 1,
          indices: [0, text.length - 1]
        };
      }
    };
    PrefixExactMatch = class extends BaseMatch {
      constructor(pattern) {
        super(pattern);
      }
      static get type() {
        return "prefix-exact";
      }
      static get multiRegex() {
        return /^\^"(.*)"$/;
      }
      static get singleRegex() {
        return /^\^(.*)$/;
      }
      search(text) {
        const isMatch = text.startsWith(this.pattern);
        return {
          isMatch,
          score: isMatch ? 0 : 1,
          indices: [0, this.pattern.length - 1]
        };
      }
    };
    InversePrefixExactMatch = class extends BaseMatch {
      constructor(pattern) {
        super(pattern);
      }
      static get type() {
        return "inverse-prefix-exact";
      }
      static get multiRegex() {
        return /^!\^"(.*)"$/;
      }
      static get singleRegex() {
        return /^!\^(.*)$/;
      }
      search(text) {
        const isMatch = !text.startsWith(this.pattern);
        return {
          isMatch,
          score: isMatch ? 0 : 1,
          indices: [0, text.length - 1]
        };
      }
    };
    SuffixExactMatch = class extends BaseMatch {
      constructor(pattern) {
        super(pattern);
      }
      static get type() {
        return "suffix-exact";
      }
      static get multiRegex() {
        return /^"(.*)"\$$/;
      }
      static get singleRegex() {
        return /^(.*)\$$/;
      }
      search(text) {
        const isMatch = text.endsWith(this.pattern);
        return {
          isMatch,
          score: isMatch ? 0 : 1,
          indices: [text.length - this.pattern.length, text.length - 1]
        };
      }
    };
    InverseSuffixExactMatch = class extends BaseMatch {
      constructor(pattern) {
        super(pattern);
      }
      static get type() {
        return "inverse-suffix-exact";
      }
      static get multiRegex() {
        return /^!"(.*)"\$$/;
      }
      static get singleRegex() {
        return /^!(.*)\$$/;
      }
      search(text) {
        const isMatch = !text.endsWith(this.pattern);
        return {
          isMatch,
          score: isMatch ? 0 : 1,
          indices: [0, text.length - 1]
        };
      }
    };
    FuzzyMatch = class extends BaseMatch {
      constructor(pattern, {
        location = Config.location,
        threshold = Config.threshold,
        distance = Config.distance,
        includeMatches = Config.includeMatches,
        findAllMatches = Config.findAllMatches,
        minMatchCharLength = Config.minMatchCharLength,
        isCaseSensitive = Config.isCaseSensitive,
        ignoreDiacritics = Config.ignoreDiacritics,
        ignoreLocation = Config.ignoreLocation
      } = {}) {
        super(pattern);
        this._bitapSearch = new BitapSearch(pattern, {
          location,
          threshold,
          distance,
          includeMatches,
          findAllMatches,
          minMatchCharLength,
          isCaseSensitive,
          ignoreDiacritics,
          ignoreLocation
        });
      }
      static get type() {
        return "fuzzy";
      }
      static get multiRegex() {
        return /^"(.*)"$/;
      }
      static get singleRegex() {
        return /^(.*)$/;
      }
      search(text) {
        return this._bitapSearch.searchIn(text);
      }
    };
    IncludeMatch = class extends BaseMatch {
      constructor(pattern) {
        super(pattern);
      }
      static get type() {
        return "include";
      }
      static get multiRegex() {
        return /^'"(.*)"$/;
      }
      static get singleRegex() {
        return /^'(.*)$/;
      }
      search(text) {
        let location = 0;
        let index;
        const indices = [];
        const patternLen = this.pattern.length;
        while ((index = text.indexOf(this.pattern, location)) > -1) {
          location = index + patternLen;
          indices.push([index, location - 1]);
        }
        const isMatch = !!indices.length;
        return {
          isMatch,
          score: isMatch ? 0 : 1,
          indices
        };
      }
    };
    searchers = [ExactMatch, IncludeMatch, PrefixExactMatch, InversePrefixExactMatch, InverseSuffixExactMatch, SuffixExactMatch, InverseExactMatch, FuzzyMatch];
    searchersLen = searchers.length;
    ESCAPED_PIPE = "\0";
    OR_TOKEN = "|";
    MultiMatchSet = /* @__PURE__ */ new Set([FuzzyMatch.type, IncludeMatch.type]);
    ExtendedSearch = class {
      constructor(pattern, {
        isCaseSensitive = Config.isCaseSensitive,
        ignoreDiacritics = Config.ignoreDiacritics,
        includeMatches = Config.includeMatches,
        minMatchCharLength = Config.minMatchCharLength,
        ignoreLocation = Config.ignoreLocation,
        findAllMatches = Config.findAllMatches,
        location = Config.location,
        threshold = Config.threshold,
        distance = Config.distance
      } = {}) {
        this.query = null;
        this.options = {
          isCaseSensitive,
          ignoreDiacritics,
          includeMatches,
          minMatchCharLength,
          findAllMatches,
          ignoreLocation,
          location,
          threshold,
          distance
        };
        pattern = isCaseSensitive ? pattern : pattern.toLowerCase();
        pattern = ignoreDiacritics ? stripDiacritics(pattern) : pattern;
        this.pattern = pattern;
        this.query = parseQuery(this.pattern, this.options);
      }
      static condition(_, options) {
        return options.useExtendedSearch;
      }
      // Note: searchIn operates on a single text value and sets hasInverse on the
      // result when inverse patterns are involved. _searchObjectList uses this to
      // switch from "ANY key" to "ALL keys" aggregation. See #712.
      searchIn(text) {
        const query = this.query;
        if (!query) {
          return {
            isMatch: false,
            score: 1
          };
        }
        const {
          includeMatches,
          isCaseSensitive,
          ignoreDiacritics
        } = this.options;
        text = isCaseSensitive ? text : text.toLowerCase();
        text = ignoreDiacritics ? stripDiacritics(text) : text;
        let numMatches = 0;
        const allIndices = [];
        let totalScore = 0;
        let hasInverse = false;
        for (let i = 0, qLen = query.length; i < qLen; i += 1) {
          const searchers2 = query[i];
          allIndices.length = 0;
          numMatches = 0;
          hasInverse = false;
          for (let j = 0, pLen = searchers2.length; j < pLen; j += 1) {
            const searcher = searchers2[j];
            const {
              isMatch,
              indices,
              score
            } = searcher.search(text);
            if (isMatch) {
              numMatches += 1;
              totalScore += score;
              const type = searcher.constructor.type;
              if (type.startsWith("inverse")) {
                hasInverse = true;
              }
              if (includeMatches) {
                if (MultiMatchSet.has(type)) {
                  allIndices.push(...indices);
                } else {
                  allIndices.push(indices);
                }
              }
            } else {
              totalScore = 0;
              numMatches = 0;
              allIndices.length = 0;
              hasInverse = false;
              break;
            }
          }
          if (numMatches) {
            const result = {
              isMatch: true,
              score: totalScore / numMatches
            };
            if (hasInverse) {
              result.hasInverse = true;
            }
            if (includeMatches) {
              result.indices = mergeIndices(allIndices);
            }
            return result;
          }
        }
        return {
          isMatch: false,
          score: 1
        };
      }
    };
    registeredSearchers = [];
    LogicalOperator = {
      AND: "$and",
      OR: "$or"
    };
    KeyType = {
      PATH: "$path",
      PATTERN: "$val"
    };
    isExpression = (query) => !!(query[LogicalOperator.AND] || query[LogicalOperator.OR]);
    isPath = (query) => !!query[KeyType.PATH];
    isLeaf = (query) => !isArray(query) && isObject(query) && !isExpression(query);
    convertToExplicit = (query) => ({
      [LogicalOperator.AND]: Object.keys(query).map((key) => ({
        [key]: query[key]
      }))
    });
    MaxHeap = class {
      constructor(limit) {
        this.limit = limit;
        this.heap = [];
      }
      get size() {
        return this.heap.length;
      }
      shouldInsert(score) {
        return this.size < this.limit || score < this.heap[0].score;
      }
      insert(item) {
        if (this.size < this.limit) {
          this.heap.push(item);
          this._bubbleUp(this.size - 1);
        } else if (item.score < this.heap[0].score) {
          this.heap[0] = item;
          this._sinkDown(0);
        }
      }
      extractSorted(sortFn) {
        return this.heap.sort(sortFn);
      }
      _bubbleUp(i) {
        const heap = this.heap;
        while (i > 0) {
          const parent = i - 1 >> 1;
          if (heap[i].score <= heap[parent].score) break;
          const tmp = heap[i];
          heap[i] = heap[parent];
          heap[parent] = tmp;
          i = parent;
        }
      }
      _sinkDown(i) {
        const heap = this.heap;
        const len = heap.length;
        let largest = i;
        do {
          i = largest;
          const left = 2 * i + 1;
          const right = 2 * i + 2;
          if (left < len && heap[left].score > heap[largest].score) {
            largest = left;
          }
          if (right < len && heap[right].score > heap[largest].score) {
            largest = right;
          }
          if (largest !== i) {
            const tmp = heap[i];
            heap[i] = heap[largest];
            heap[largest] = tmp;
          }
        } while (largest !== i);
      }
    };
    WORD = /\b\w+\b/g;
    Fuse = class {
      // Statics are assigned in entry.ts
      constructor(docs, options, index) {
        this.options = {
          ...Config,
          ...options
        };
        if (this.options.useExtendedSearch && false) ;
        if (this.options.useTokenSearch && false) ;
        this._keyStore = new KeyStore(this.options.keys);
        this._docs = docs;
        this._myIndex = null;
        this._invertedIndex = null;
        this.setCollection(docs, index);
        this._lastQuery = null;
        this._lastSearcher = null;
      }
      _getSearcher(query) {
        if (this._lastQuery === query) {
          return this._lastSearcher;
        }
        const opts = this._invertedIndex ? {
          ...this.options,
          _invertedIndex: this._invertedIndex
        } : this.options;
        const searcher = createSearcher(query, opts);
        this._lastQuery = query;
        this._lastSearcher = searcher;
        return searcher;
      }
      setCollection(docs, index) {
        this._docs = docs;
        if (index && !(index instanceof FuseIndex)) {
          throw new Error(INCORRECT_INDEX_TYPE);
        }
        this._myIndex = index || createIndex(this.options.keys, this._docs, {
          getFn: this.options.getFn,
          fieldNormWeight: this.options.fieldNormWeight
        });
        if (this.options.useTokenSearch) {
          const analyzer = createAnalyzer({
            isCaseSensitive: this.options.isCaseSensitive,
            ignoreDiacritics: this.options.ignoreDiacritics
          });
          this._invertedIndex = buildInvertedIndex(this._myIndex.records, this._myIndex.keys.length, analyzer);
        }
      }
      add(doc) {
        if (!isDefined(doc)) {
          return;
        }
        this._docs.push(doc);
        this._myIndex.add(doc);
        if (this._invertedIndex) {
          const record = this._myIndex.records[this._myIndex.records.length - 1];
          const analyzer = createAnalyzer({
            isCaseSensitive: this.options.isCaseSensitive,
            ignoreDiacritics: this.options.ignoreDiacritics
          });
          addToInvertedIndex(this._invertedIndex, record, this._myIndex.keys.length, analyzer);
        }
      }
      remove(predicate = () => false) {
        const results = [];
        const indicesToRemove = [];
        for (let i = 0, len = this._docs.length; i < len; i += 1) {
          if (predicate(this._docs[i], i)) {
            results.push(this._docs[i]);
            indicesToRemove.push(i);
          }
        }
        if (indicesToRemove.length) {
          if (this._invertedIndex) {
            for (const idx of indicesToRemove) {
              removeFromInvertedIndex(this._invertedIndex, idx);
            }
          }
          for (let i = indicesToRemove.length - 1; i >= 0; i -= 1) {
            this._docs.splice(indicesToRemove[i], 1);
          }
          this._myIndex.removeAll(indicesToRemove);
        }
        return results;
      }
      removeAt(idx) {
        if (this._invertedIndex) {
          removeFromInvertedIndex(this._invertedIndex, idx);
        }
        const doc = this._docs.splice(idx, 1)[0];
        this._myIndex.removeAt(idx);
        return doc;
      }
      getIndex() {
        return this._myIndex;
      }
      search(query, options) {
        const {
          limit = -1
        } = options || {};
        const {
          includeMatches,
          includeScore,
          shouldSort,
          sortFn,
          ignoreFieldNorm
        } = this.options;
        if (isString(query) && !query.trim()) {
          let docs = this._docs.map((item, idx) => ({
            item,
            refIndex: idx
          }));
          if (isNumber(limit) && limit > -1) {
            docs = docs.slice(0, limit);
          }
          return docs;
        }
        const useHeap = isNumber(limit) && limit > 0 && isString(query);
        let results;
        if (useHeap) {
          const heap = new MaxHeap(limit);
          if (isString(this._docs[0])) {
            this._searchStringList(query, {
              heap,
              ignoreFieldNorm
            });
          } else {
            this._searchObjectList(query, {
              heap,
              ignoreFieldNorm
            });
          }
          results = heap.extractSorted(sortFn);
        } else {
          results = isString(query) ? isString(this._docs[0]) ? this._searchStringList(query) : this._searchObjectList(query) : this._searchLogical(query);
          computeScore(results, {
            ignoreFieldNorm
          });
          if (shouldSort) {
            results.sort(sortFn);
          }
          if (isNumber(limit) && limit > -1) {
            results = results.slice(0, limit);
          }
        }
        return format(results, this._docs, {
          includeMatches,
          includeScore
        });
      }
      _searchStringList(query, {
        heap,
        ignoreFieldNorm
      } = {}) {
        const searcher = this._getSearcher(query);
        const {
          records
        } = this._myIndex;
        const results = heap ? null : [];
        records.forEach(({
          v: text,
          i: idx,
          n: norm2
        }) => {
          if (!isDefined(text)) {
            return;
          }
          const {
            isMatch,
            score,
            indices
          } = searcher.searchIn(text);
          if (isMatch) {
            const result = {
              item: text,
              idx,
              matches: [{
                score,
                value: text,
                norm: norm2,
                indices
              }]
            };
            if (heap) {
              result.score = computeScoreSingle(result.matches, {
                ignoreFieldNorm
              });
              if (heap.shouldInsert(result.score)) {
                heap.insert(result);
              }
            } else {
              results.push(result);
            }
          }
        });
        return results;
      }
      _searchLogical(query) {
        const expression = parse(query, this.options);
        const evaluate = (node, item, idx) => {
          if (!("children" in node)) {
            const {
              keyId,
              searcher
            } = node;
            let matches;
            if (keyId === null) {
              matches = [];
              this._myIndex.keys.forEach((key, keyIndex) => {
                matches.push(...this._findMatches({
                  key,
                  value: item[keyIndex],
                  searcher
                }));
              });
            } else {
              matches = this._findMatches({
                key: this._keyStore.get(keyId),
                value: this._myIndex.getValueForItemAtKeyId(item, keyId),
                searcher
              });
            }
            if (matches && matches.length) {
              return [{
                idx,
                item,
                matches
              }];
            }
            return [];
          }
          const {
            children,
            operator
          } = node;
          const res = [];
          for (let i = 0, len = children.length; i < len; i += 1) {
            const child = children[i];
            const result = evaluate(child, item, idx);
            if (result.length) {
              res.push(...result);
            } else if (operator === LogicalOperator.AND) {
              return [];
            }
          }
          return res;
        };
        const records = this._myIndex.records;
        const resultMap = /* @__PURE__ */ new Map();
        const results = [];
        records.forEach(({
          $: item,
          i: idx
        }) => {
          if (isDefined(item)) {
            const expResults = evaluate(expression, item, idx);
            if (expResults.length) {
              if (!resultMap.has(idx)) {
                resultMap.set(idx, {
                  idx,
                  item,
                  matches: []
                });
                results.push(resultMap.get(idx));
              }
              expResults.forEach(({
                matches
              }) => {
                resultMap.get(idx).matches.push(...matches);
              });
            }
          }
        });
        return results;
      }
      // When a search involves inverse patterns (e.g. !Syrup), the aggregation
      // across keys switches from "ANY key matches" to "ALL keys must match."
      // This is signaled by hasInverse on the SearchResult from ExtendedSearch.
      //
      // For mixed patterns like "^hello !Syrup", a key failure is ambiguous —
      // it could be the positive or inverse term that failed. In that case we
      // conservatively exclude the item, which is strictly better than the old
      // behavior of including it. See: https://github.com/krisk/Fuse/issues/712
      _searchObjectList(query, {
        heap,
        ignoreFieldNorm
      } = {}) {
        const searcher = this._getSearcher(query);
        const {
          keys,
          records
        } = this._myIndex;
        const results = heap ? null : [];
        records.forEach(({
          $: item,
          i: idx
        }) => {
          if (!isDefined(item)) {
            return;
          }
          const matches = [];
          let anyKeyFailed = false;
          let hasInverse = false;
          keys.forEach((key, keyIndex) => {
            const keyMatches = this._findMatches({
              key,
              value: item[keyIndex],
              searcher
            });
            if (keyMatches.length) {
              matches.push(...keyMatches);
              if (keyMatches[0].hasInverse) {
                hasInverse = true;
              }
            } else {
              anyKeyFailed = true;
            }
          });
          if (hasInverse && anyKeyFailed) {
            return;
          }
          if (matches.length) {
            const result = {
              idx,
              item,
              matches
            };
            if (heap) {
              result.score = computeScoreSingle(result.matches, {
                ignoreFieldNorm
              });
              if (heap.shouldInsert(result.score)) {
                heap.insert(result);
              }
            } else {
              results.push(result);
            }
          }
        });
        return results;
      }
      _findMatches({
        key,
        value,
        searcher
      }) {
        if (!isDefined(value)) {
          return [];
        }
        const matches = [];
        if (isArray(value)) {
          value.forEach(({
            v: text,
            i: idx,
            n: norm2
          }) => {
            if (!isDefined(text)) {
              return;
            }
            const {
              isMatch,
              score,
              indices,
              hasInverse
            } = searcher.searchIn(text);
            if (isMatch) {
              matches.push({
                score,
                key,
                value: text,
                idx,
                norm: norm2,
                indices,
                hasInverse
              });
            }
          });
        } else {
          const {
            v: text,
            n: norm2
          } = value;
          const {
            isMatch,
            score,
            indices,
            hasInverse
          } = searcher.searchIn(text);
          if (isMatch) {
            matches.push({
              score,
              key,
              value: text,
              norm: norm2,
              indices,
              hasInverse
            });
          }
        }
        return matches;
      }
    };
    TokenSearch = class {
      static condition(_, options) {
        return options.useTokenSearch;
      }
      constructor(pattern, options) {
        this.options = options;
        this.analyzer = createAnalyzer({
          isCaseSensitive: options.isCaseSensitive,
          ignoreDiacritics: options.ignoreDiacritics
        });
        const queryTerms = this.analyzer.tokenize(pattern);
        const invertedIndex = options._invertedIndex;
        const {
          df,
          fieldCount
        } = invertedIndex;
        this.termSearchers = [];
        this.idfWeights = [];
        for (const term of queryTerms) {
          this.termSearchers.push(new BitapSearch(term, {
            location: options.location,
            threshold: options.threshold,
            distance: options.distance,
            includeMatches: options.includeMatches,
            findAllMatches: options.findAllMatches,
            minMatchCharLength: options.minMatchCharLength,
            isCaseSensitive: options.isCaseSensitive,
            ignoreDiacritics: options.ignoreDiacritics,
            ignoreLocation: true
          }));
          const docFreq = df.get(term) || 0;
          const idf = Math.log(1 + (fieldCount - docFreq + 0.5) / (docFreq + 0.5));
          this.idfWeights.push(idf);
        }
      }
      searchIn(text) {
        if (!this.termSearchers.length) {
          return {
            isMatch: false,
            score: 1
          };
        }
        const allIndices = [];
        let weightedScore = 0;
        let maxPossibleScore = 0;
        let matchedCount = 0;
        for (let i = 0; i < this.termSearchers.length; i++) {
          const result = this.termSearchers[i].searchIn(text);
          const idf = this.idfWeights[i];
          maxPossibleScore += idf;
          if (result.isMatch) {
            matchedCount++;
            weightedScore += idf * (1 - result.score);
            if (result.indices) {
              allIndices.push(...result.indices);
            }
          }
        }
        if (matchedCount === 0) {
          return {
            isMatch: false,
            score: 1
          };
        }
        const normalized = maxPossibleScore > 0 ? 1 - weightedScore / maxPossibleScore : 0;
        const searchResult = {
          isMatch: true,
          score: Math.max(1e-3, normalized)
        };
        if (this.options.includeMatches && allIndices.length) {
          searchResult.indices = mergeIndices(allIndices);
        }
        return searchResult;
      }
    };
    Fuse.version = "7.3.0";
    Fuse.createIndex = createIndex;
    Fuse.parseIndex = parseIndex;
    Fuse.config = Config;
    Fuse.match = function(pattern, text, options) {
      const searcher = createSearcher(pattern, {
        ...Config,
        ...options
      });
      return searcher.searchIn(text);
    };
    {
      Fuse.parseQuery = parse;
    }
    {
      register(ExtendedSearch);
    }
    {
      register(TokenSearch);
    }
    Fuse.use = function(...plugins) {
      plugins.forEach((plugin) => register(plugin));
    };
  }
});

// src/main/extensions/raycastShim.ts
async function runAppleScript2(source) {
  if (process.platform !== "darwin") {
    throw new Error("AppleScript is only available on macOS");
  }
  if (typeof source !== "string" || source.trim().length === 0) {
    return "";
  }
  const { stdout } = await execFileAsync4("/usr/bin/osascript", ["-e", source], {
    encoding: "utf8",
    maxBuffer: 10 * 1024 * 1024
  });
  return String(stdout).replace(/\r?\n$/, "");
}
function createRenderProxy(name) {
  const target = function() {
    return void 0;
  };
  Object.defineProperty(target, "name", { value: name });
  return new Proxy(target, {
    get(t, prop) {
      if (prop === Symbol.toPrimitive) return () => `[Raycast:${name}]`;
      if (prop === "displayName") return name;
      if (prop === "prototype") return {};
      if (typeof prop === "symbol") return void 0;
      if (prop in t) return Reflect.get(t, prop);
      return createRenderProxy(`${name}.${String(prop)}`);
    },
    apply() {
      return void 0;
    },
    construct() {
      return {};
    }
  });
}
function createLocalStorage(packageRoot) {
  const file = (0, import_node_path5.join)(packageRoot, "localStorage.json");
  const readAll = () => {
    try {
      const raw = (0, import_node_fs4.readFileSync)(file, "utf8");
      const parsed = JSON.parse(raw);
      return parsed && typeof parsed === "object" ? parsed : {};
    } catch {
      return {};
    }
  };
  const writeAll = (value) => {
    (0, import_node_fs4.mkdirSync)(packageRoot, { recursive: true });
    (0, import_node_fs4.writeFileSync)(file, JSON.stringify(value, null, 2), "utf8");
  };
  return {
    getItem: async (key) => readAll()[key],
    setItem: async (key, value) => {
      const all = readAll();
      all[key] = value;
      writeAll(all);
    },
    removeItem: async (key) => {
      const all = readAll();
      delete all[key];
      writeAll(all);
    },
    clear: async () => {
      writeAll({});
    },
    allItems: async () => readAll()
  };
}
function readPreferences(packageRoot) {
  const file = (0, import_node_path5.join)(packageRoot, "preferences.json");
  try {
    const raw = (0, import_node_fs4.readFileSync)(file, "utf8");
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}
function createClipboardShim() {
  return {
    copy: copyClipboardContent,
    paste: async (value) => {
      await copyClipboardContent(value);
      await pasteClipboardContent();
    },
    readText: async () => clipboard.readText(),
    read: async () => ({
      text: await clipboard.readText() || void 0
    }),
    clear: async () => clipboard.clear()
  };
}
function createEnvironment(ctx) {
  const supportPath = (0, import_node_path5.join)(ctx.packageRoot, "support");
  try {
    (0, import_node_fs4.mkdirSync)(supportPath, { recursive: true });
  } catch {
  }
  return {
    appearance: "dark",
    commandName: ctx.commandName,
    commandMode: "no-view",
    extensionName: ctx.extensionId,
    raycastVersion: "1.77.0",
    isDevelopment: !app.isPackaged,
    supportPath,
    assetsPath: (0, import_node_path5.join)(ctx.packageRoot, "assets"),
    launchType: "userInitiated",
    textSize: "medium"
  };
}
function createRaycastApi(ctx) {
  const Image = Object.assign(createRenderProxy("Image"), {
    Mask: IMAGE_MASK2
  });
  return {
    Toast: { Style: TOAST_STYLE },
    Icon: createRenderProxy("Icon"),
    Color: createRenderProxy("Color"),
    Image,
    ImageMask: IMAGE_MASK2,
    List: createRenderProxy("List"),
    Form: createRenderProxy("Form"),
    Detail: createRenderProxy("Detail"),
    Grid: createRenderProxy("Grid"),
    Action: createRenderProxy("Action"),
    ActionPanel: createRenderProxy("ActionPanel"),
    MenuBarExtra: createRenderProxy("MenuBarExtra"),
    Alert: {
      ActionStyle: { Destructive: "destructive", Cancel: "cancel", Default: "default" }
    },
    Keyboard: {
      Shortcut: { Common: {} }
    },
    OAuth: createRenderProxy("OAuth"),
    BrowserExtension: createRenderProxy("BrowserExtension"),
    AI: {
      Model: createRenderProxy("AI.Model"),
      ask: async (prompt) => {
        return prompt;
      }
    },
    environment: createEnvironment(ctx),
    LocalStorage: createLocalStorage(ctx.packageRoot),
    Cache: class {
      store = /* @__PURE__ */ new Map();
      get(key) {
        return this.store.get(key);
      }
      set(key, value) {
        this.store.set(key, value);
      }
      has(key) {
        return this.store.has(key);
      }
      remove(key) {
        this.store.delete(key);
      }
      clear() {
        this.store.clear();
      }
    },
    Clipboard: createClipboardShim(),
    getPreferenceValues: () => readPreferences(ctx.packageRoot),
    getSelectedText: readSelectedText,
    getApplications: async () => listApplications(),
    getFrontmostApplication: async () => {
      const apps = listApplications();
      return apps[0] ?? { name: "Tezbar", path: process.execPath };
    },
    getDefaultApplication: async () => null,
    runAppleScript: runAppleScript2,
    open: async (target) => {
      if (typeof target !== "string") return;
      if (/^[a-z][a-z0-9+.-]*:\/\//i.test(target) || target.startsWith("mailto:")) {
        await shell.openExternal(target);
      } else {
        const resolved = target.startsWith("~") ? target.replace(/^~/, (0, import_node_os3.homedir)()) : target;
        await shell.openPath(resolved);
      }
    },
    openExtensionPreferences: async () => {
    },
    openCommandPreferences: async () => {
    },
    showToast: (opts) => {
      const obj = opts && typeof opts === "object" ? opts : {};
      ctx.feedback.push({
        kind: "toast",
        style: typeof obj.style === "string" ? obj.style : void 0,
        title: typeof obj.title === "string" ? obj.title : void 0,
        message: typeof obj.message === "string" ? obj.message : void 0
      });
      return {
        hide: async () => {
        },
        set title(_v) {
        },
        set message(_v) {
        },
        set style(_v) {
        }
      };
    },
    showHUD: async (message) => {
      ctx.feedback.push({ kind: "hud", message: String(message ?? "") });
    },
    showInFinder: async (path4) => {
      if (typeof path4 !== "string") return;
      shell.showItemInFolder(path4);
    },
    confirmAlert: (options) => confirmExtensionAlert(options),
    closeMainWindow: async () => {
    },
    popToRoot: async () => {
    },
    updateCommandMetadata: async () => {
    },
    captureException: () => {
    },
    useNavigation: () => ({ push: () => {
    }, pop: () => {
    } }),
    /** Image helper. Electron has its own `nativeImage`; extensions mainly
     *  use this for sizing/base64 conversion. */
    createImage: (buffer) => nativeImage.createFromBuffer(buffer)
  };
}
function createRaycastUtils(ctx) {
  const localStorage = createLocalStorage(ctx.packageRoot);
  return {
    runPowerShellScript,
    executeSQL: executeExtensionSQL,
    useCachedState: (_, initialValue) => {
      let state = initialValue;
      const setState = (next) => {
        state = typeof next === "function" ? next(state) : next;
      };
      return [state, setState];
    },
    useCachedPromise: () => ({
      data: void 0,
      revalidate: async () => {
      },
      isLoading: false,
      mutate: async () => {
      },
      error: void 0,
      pagination: void 0
    }),
    usePromise: () => ({
      data: void 0,
      isLoading: false,
      revalidate: async () => {
      },
      mutate: async () => {
      },
      error: void 0
    }),
    useFetch: () => ({
      data: void 0,
      isLoading: false,
      revalidate: async () => {
      },
      error: void 0
    }),
    useExec: () => ({
      data: void 0,
      isLoading: false,
      error: void 0,
      revalidate: async () => {
      }
    }),
    useLocalStorage: (key, initialValue) => {
      let current = initialValue;
      void localStorage.getItem(key).then((raw) => {
        if (typeof raw === "string") {
          try {
            current = JSON.parse(raw);
          } catch {
          }
        }
      });
      return {
        value: current,
        setValue: async (next) => {
          current = next;
          await localStorage.setItem(key, JSON.stringify(next));
        },
        removeValue: async () => {
          await localStorage.setItem(key, "null");
        },
        isLoading: false
      };
    },
    useForm: () => ({
      itemProps: new Proxy(
        {},
        {
          get: () => ({ value: "", onChange: () => {
          } })
        }
      ),
      values: {},
      setValue: () => {
      },
      setValidationError: () => {
      },
      reset: () => {
      },
      focus: () => {
      },
      handleSubmit: () => async () => true
    }),
    FormValidation: { Required: () => void 0 },
    runAppleScript: runAppleScript2,
    showFailureToast: (error) => {
      ctx.feedback.push({
        kind: "toast",
        style: "failure",
        title: error instanceof Error ? error.message : String(error)
      });
    },
    getFavicon: (baseUrl, options) => {
      const size = Math.max(16, Math.min(256, Number(options?.size) || 64));
      const hostMatch = /^[a-z][a-z0-9+.-]*:\/\/([^/?#]+)/i.exec(String(baseUrl ?? "").trim());
      const host = hostMatch?.[1];
      if (host) {
        return {
          source: `https://www.google.com/s2/favicons?domain=${encodeURIComponent(host)}&sz=${size}`
        };
      }
      return { source: String(options?.fallback ?? "Icon.Globe") };
    }
  };
}
function formatRuntimeFeedback(feedback) {
  if (feedback.kind === "hud") {
    return feedback.message ?? "Extension command completed.";
  }
  const title = feedback.title?.trim() ?? "";
  const message = feedback.message?.trim() ?? "";
  if (title && message) return `${title}: ${message}`;
  return title || message || "Extension command completed.";
}
var import_node_child_process3, import_node_fs4, import_node_os3, import_node_path5, import_node_util3, TOAST_STYLE, IMAGE_MASK2, execFileAsync4;
var init_raycastShim = __esm({
  "src/main/extensions/raycastShim.ts"() {
    "use strict";
    init_desktop();
    import_node_child_process3 = require("node:child_process");
    import_node_fs4 = require("node:fs");
    import_node_os3 = require("node:os");
    import_node_path5 = require("node:path");
    import_node_util3 = require("node:util");
    init_services();
    init_extension_confirm_alert();
    init_services();
    init_powershell_script();
    init_extension_sql();
    init_clipboard();
    TOAST_STYLE = {
      Success: "success",
      Failure: "failure",
      Animated: "animated"
    };
    IMAGE_MASK2 = {
      Circle: "circle",
      RoundedRectangle: "roundedRectangle"
    };
    execFileAsync4 = (0, import_node_util3.promisify)(import_node_child_process3.execFile);
  }
});

// src/main/extensions/service.ts
var service_exports = {};
__export(service_exports, {
  executeExtensionCommandRuntime: () => executeExtensionCommandRuntime,
  getExtensionCommands: () => getExtensionCommands,
  getExtensionInstallError: () => getExtensionInstallError,
  getStoreCatalog: () => getStoreCatalog,
  inspectExtensionIntegrity: () => inspectExtensionIntegrity,
  installExtension: () => installExtension2,
  isUnsupportedRuntimeModeError: () => isUnsupportedRuntimeModeError,
  listInstalledExtensions: () => listInstalledExtensions,
  reinstallExtension: () => reinstallExtension,
  searchStoreExtensions: () => searchStoreExtensions,
  uninstallExtension: () => uninstallExtension2
});
function getDbPath() {
  const dir = (0, import_node_path6.join)(app.getPath("userData"), "extensions");
  (0, import_node_fs5.mkdirSync)(dir, { recursive: true });
  return (0, import_node_path6.join)(dir, "installed.json");
}
function extensionsRootDir() {
  const dir = (0, import_node_path6.join)(app.getPath("userData"), "extensions");
  (0, import_node_fs5.mkdirSync)(dir, { recursive: true });
  return dir;
}
function installedPackageRoot(extensionId) {
  return (0, import_node_path6.join)(extensionsRootDir(), "packages", extensionId);
}
function packageJsonPathForInstalledExtension(extensionId) {
  return (0, import_node_path6.join)(installedPackageRoot(extensionId), "package.json");
}
function scriptPathForInstalledExtensionCommand(extensionId, commandName) {
  return (0, import_node_path6.join)(installedPackageRoot(extensionId), ".sc-build", `${commandName}.js`);
}
function metaPathForInstalledExtension(extensionId) {
  return (0, import_node_path6.join)(installedPackageRoot(extensionId), "meta.json");
}
function backupPackageRoot(extensionId) {
  return (0, import_node_path6.join)(extensionsRootDir(), "packages", `${extensionId}.backup`);
}
function readInstallMeta(extensionId) {
  const p = metaPathForInstalledExtension(extensionId);
  if (!(0, import_node_fs5.existsSync)(p)) return null;
  try {
    return JSON.parse((0, import_node_fs5.readFileSync)(p, "utf8"));
  } catch {
    return null;
  }
}
function writeInstallMeta(meta) {
  const p = metaPathForInstalledExtension(meta.extensionId);
  (0, import_node_fs5.mkdirSync)((0, import_node_path6.dirname)(p), { recursive: true });
  (0, import_node_fs5.writeFileSync)(p, JSON.stringify(meta, null, 2), "utf8");
}
function hashText(text) {
  return (0, import_node_crypto3.createHash)("sha256").update(text).digest("hex");
}
function inspectIntegrity(extensionId) {
  const meta = readInstallMeta(extensionId);
  if (!meta) {
    return {
      extensionId,
      installed: false,
      missingScripts: [],
      tamperedScripts: [],
      healthy: false
    };
  }
  const missing = [...meta.missingScripts];
  const tampered = [];
  for (const name of meta.commandNames) {
    if (meta.missingScripts.includes(name)) continue;
    const scriptPath = scriptPathForInstalledExtensionCommand(extensionId, name);
    if (!(0, import_node_fs5.existsSync)(scriptPath)) {
      missing.push(name);
      continue;
    }
    const expected = meta.scriptHashes[name];
    if (!expected) continue;
    try {
      const actual = hashText((0, import_node_fs5.readFileSync)(scriptPath, "utf8"));
      if (actual !== expected) tampered.push(name);
    } catch {
      missing.push(name);
    }
  }
  return {
    extensionId,
    installed: true,
    commitRef: meta.commitRef,
    missingScripts: Array.from(new Set(missing)),
    tamperedScripts: tampered,
    healthy: missing.length === 0 && tampered.length === 0,
    lastError: meta.lastError
  };
}
function parseJsonSafe(raw) {
  try {
    return JSON.parse(raw);
  } catch {
    return null;
  }
}
function readInstalledPackageJson(extensionId) {
  const p = packageJsonPathForInstalledExtension(extensionId);
  if (!(0, import_node_fs5.existsSync)(p)) return null;
  try {
    const raw = (0, import_node_fs5.readFileSync)(p, "utf8");
    return parseJsonSafe(raw);
  } catch {
    return null;
  }
}
function extensionSlugFromId(extensionId) {
  return extensionId.startsWith("raycast.") ? extensionId.slice("raycast.".length) : extensionId;
}
function normalizeExtensionCommandsFromPackage(pkg) {
  return (pkg.commands ?? []).map((cmd) => {
    const args = Array.isArray(cmd.arguments) ? cmd.arguments.filter((arg) => arg && typeof arg.name === "string" && arg.name.trim().length > 0).map((arg) => ({
      name: String(arg.name),
      required: Boolean(arg.required),
      type: typeof arg.type === "string" ? arg.type : void 0,
      placeholder: typeof arg.placeholder === "string" ? arg.placeholder : void 0,
      title: typeof arg.title === "string" ? arg.title : void 0,
      data: Array.isArray(arg.data) ? arg.data : void 0
    })) : [];
    const firstArg = args[0];
    return {
      name: cmd.name ?? "",
      title: cmd.title ?? cmd.name ?? "",
      subtitle: cmd.subtitle ?? cmd.description ?? "",
      mode: typeof cmd.mode === "string" ? cmd.mode : void 0,
      argumentName: firstArg?.name,
      argumentPlaceholder: firstArg?.placeholder,
      commandArgumentDefinitions: args
    };
  }).filter((cmd) => cmd.name && cmd.title);
}
function readDb() {
  const p = getDbPath();
  try {
    const raw = (0, import_node_fs5.readFileSync)(p, "utf8");
    const parsed = JSON.parse(raw);
    return {
      installed: Array.isArray(parsed.installed) ? parsed.installed : []
    };
  } catch {
    return DEFAULT_DB;
  }
}
function writeDb(db) {
  const p = getDbPath();
  (0, import_node_fs5.writeFileSync)(p, JSON.stringify(db, null, 2), "utf8");
}
function byName(a, b) {
  return a.name.localeCompare(b.name);
}
function normalizeNameFromSlug(slug) {
  return slug.split(/[-_]/g).filter(Boolean).map((part) => part[0]?.toUpperCase() + part.slice(1)).join(" ");
}
async function fetchGithubJson(url) {
  const response = await fetch(url, {
    headers: {
      Accept: "application/vnd.github+json",
      "User-Agent": "tezbar-extension-indexer"
    }
  });
  if (!response.ok) {
    throw new Error(`GitHub API request failed (${response.status}): ${url}`);
  }
  return await response.json();
}
async function fetchText(url) {
  const response = await fetch(url, {
    headers: {
      Accept: "application/vnd.github+json",
      "User-Agent": "tezbar-extension-indexer"
    }
  });
  if (!response.ok) {
    throw new Error(`Request failed (${response.status}): ${url}`);
  }
  return await response.text();
}
async function fetchRaycastPackage(slug) {
  const url = `https://raw.githubusercontent.com/raycast/extensions/${RAYCAST_EXTENSIONS_REF}/${RAYCAST_EXTENSIONS_PATH}/${slug}/package.json`;
  const raw = await fetchText(url);
  const parsed = parseJsonSafe(raw);
  if (!parsed || typeof parsed !== "object") {
    throw new Error(`Invalid package.json for extension: ${slug}`);
  }
  return parsed;
}
async function fetchRaycastCatalogFromAwesome() {
  const raw = await fetchText(AWESOME_RAYCAST_DATA);
  const data = JSON.parse(raw);
  return data.map((item) => ({
    id: `raycast.${item.name}`,
    name: item.title || normalizeNameFromSlug(item.name),
    description: item.description || `Raycast extension: ${item.title}`,
    author: item.author || "Raycast Community",
    version: "latest",
    repository: `${RAYCAST_EXTENSIONS_REPO}/tree/main/${RAYCAST_EXTENSIONS_PATH}/${item.name}`,
    downloadCount: item.download_count,
    owner: item.owner
  }));
}
async function fetchRaycastCatalogFromGithub() {
  try {
    const commit = await fetchGithubJson(
      `https://api.github.com/repos/raycast/extensions/git/commits/${RAYCAST_EXTENSIONS_REF}`
    );
    const rootTree = await fetchGithubJson(
      `https://api.github.com/repos/raycast/extensions/git/trees/${commit.tree.sha}`
    );
    const extensionsDir = rootTree.tree.find(
      (entry) => entry.type === "tree" && entry.path === RAYCAST_EXTENSIONS_PATH
    );
    if (!extensionsDir) return [];
    const extensionsTree = await fetchGithubJson(
      `https://api.github.com/repos/raycast/extensions/git/trees/${extensionsDir.sha}`
    );
    return extensionsTree.tree.filter((entry) => entry.type === "tree").map((entry) => {
      const slug = entry.path;
      const name = normalizeNameFromSlug(slug);
      return {
        id: `raycast.${slug}`,
        name,
        description: `Raycast extension: ${name}`,
        author: "Raycast Community",
        version: RAYCAST_EXTENSIONS_REF.slice(0, 7),
        repository: `${RAYCAST_EXTENSIONS_REPO}/tree/${RAYCAST_EXTENSIONS_REF}/${RAYCAST_EXTENSIONS_PATH}/${slug}`
      };
    });
  } catch {
    return [];
  }
}
async function stageAndInstallExtension(extensionId, slug) {
  const pkg = await fetchRaycastPackage(slug);
  const staging = (0, import_node_fs5.mkdtempSync)((0, import_node_path6.join)((0, import_node_os4.tmpdir)(), `tezbar-ext-${extensionId}-`));
  const stagingBuild = (0, import_node_path6.join)(staging, ".sc-build");
  (0, import_node_fs5.mkdirSync)(stagingBuild, { recursive: true });
  (0, import_node_fs5.writeFileSync)((0, import_node_path6.join)(staging, "package.json"), JSON.stringify(pkg, null, 2), "utf8");
  const commandEntries = (pkg.commands ?? []).map((cmd) => ({
    name: typeof cmd.name === "string" ? cmd.name.trim() : "",
    mode: typeof cmd.mode === "string" ? cmd.mode.trim() : ""
  })).filter((entry) => entry.name.length > 0);
  const missingScripts = [];
  const scriptHashes = {};
  await Promise.all(
    commandEntries.map(async (entry) => {
      const url = `https://raw.githubusercontent.com/raycast/extensions/${RAYCAST_EXTENSIONS_REF}/${RAYCAST_EXTENSIONS_PATH}/${slug}/.sc-build/${entry.name}.js`;
      try {
        const js = await fetchText(url);
        (0, import_node_fs5.writeFileSync)((0, import_node_path6.join)(stagingBuild, `${entry.name}.js`), js, "utf8");
        scriptHashes[entry.name] = hashText(js);
      } catch {
        missingScripts.push(entry.name);
      }
    })
  );
  const root = installedPackageRoot(extensionId);
  const backup = backupPackageRoot(extensionId);
  if ((0, import_node_fs5.existsSync)(backup)) (0, import_node_fs5.rmSync)(backup, { recursive: true, force: true });
  try {
    if ((0, import_node_fs5.existsSync)(root)) (0, import_node_fs5.renameSync)(root, backup);
    (0, import_node_fs5.mkdirSync)((0, import_node_path6.dirname)(root), { recursive: true });
    (0, import_node_fs5.renameSync)(staging, root);
  } catch (error) {
    if ((0, import_node_fs5.existsSync)(backup) && !(0, import_node_fs5.existsSync)(root)) {
      try {
        (0, import_node_fs5.renameSync)(backup, root);
      } catch {
      }
    }
    (0, import_node_fs5.rmSync)(staging, { recursive: true, force: true });
    throw error;
  } finally {
    if ((0, import_node_fs5.existsSync)(backup)) (0, import_node_fs5.rmSync)(backup, { recursive: true, force: true });
  }
  writeInstallMeta({
    extensionId,
    commitRef: RAYCAST_EXTENSIONS_REF,
    installedAt: Date.now(),
    commandNames: commandEntries.map((entry) => entry.name),
    missingScripts,
    scriptHashes
  });
  return pkg;
}
async function ensureRaycastExtensionBundle(extensionId) {
  const meta = readInstallMeta(extensionId);
  const existing = readInstalledPackageJson(extensionId);
  if (meta && existing && meta.commitRef === RAYCAST_EXTENSIONS_REF) {
    const report = inspectIntegrity(extensionId);
    if (report.healthy) return existing;
  }
  const slug = extensionSlugFromId(extensionId);
  return await stageAndInstallExtension(extensionId, slug);
}
async function ensureExtensionBundle(extensionId) {
  if (!extensionId.startsWith("raycast.")) return null;
  try {
    const pkg = await ensureRaycastExtensionBundle(extensionId);
    installErrors.delete(extensionId);
    return pkg;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    installErrors.set(extensionId, message);
    const existingMeta = readInstallMeta(extensionId);
    if (existingMeta) {
      writeInstallMeta({ ...existingMeta, lastError: message });
    }
    console.warn("[extensions] failed to ensure extension bundle:", extensionId, error);
    return readInstalledPackageJson(extensionId);
  }
}
function inspectExtensionIntegrity(extensionId) {
  return inspectIntegrity(extensionId);
}
function getExtensionInstallError(extensionId) {
  return installErrors.get(extensionId) ?? null;
}
async function reinstallExtension(extensionId) {
  const slug = extensionSlugFromId(extensionId);
  try {
    await stageAndInstallExtension(extensionId, slug);
    installErrors.delete(extensionId);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    installErrors.set(extensionId, message);
    throw error;
  }
  commandCache.delete(extensionId);
  return inspectIntegrity(extensionId);
}
async function executeNoViewScript(extensionId, commandName, scriptPath, argumentValues) {
  const fileRequire = (0, import_node_module2.createRequire)(scriptPath);
  const feedback = [];
  const packageRoot = installedPackageRoot(extensionId);
  const shimCtx = { extensionId, commandName, packageRoot, feedback };
  const raycastApiShim = createRaycastApi(shimCtx);
  const raycastUtilsShim = createRaycastUtils(shimCtx);
  const builtinSet = new Set(import_node_module2.builtinModules);
  const customRequire = (specifier) => {
    if (specifier === "@raycast/api") return raycastApiShim;
    if (specifier === "@raycast/utils") return raycastUtilsShim;
    if (specifier.startsWith("./") || specifier.startsWith("../") || specifier.startsWith("/")) {
      return fileRequire(specifier);
    }
    if (specifier.startsWith("node:") || builtinSet.has(specifier)) {
      return fileRequire(specifier);
    }
    throw new Error(`Unsupported runtime dependency: ${specifier}`);
  };
  const mod = { exports: {} };
  let scriptSource = (0, import_node_fs5.readFileSync)(scriptPath, "utf8");
  if (scriptPath.toLowerCase().includes("amphetamine")) {
    scriptSource = scriptSource.replace(
      /if\s*\(\s*isSessionActive\s*===\s*["']true["']\s*\)\s*\{[\s\S]*?toast\.title\s*=\s*["']A session is already running["'][\s\S]*?return\s+false;?\s*\}/g,
      `if (isSessionActive === "true") {
    await runAppleScript(\`
    tell application "Amphetamine"
        end session
    end tell
  \`);
  }`
    );
  }
  const wrapper = new Function(
    "exports",
    "require",
    "module",
    "__filename",
    "__dirname",
    scriptSource
  );
  wrapper(mod.exports, customRequire, mod, scriptPath, (0, import_node_path6.dirname)(scriptPath));
  const exported = mod.exports;
  const command = typeof exported.default === "function" ? exported.default : typeof mod.exports === "function" ? mod.exports : null;
  if (!command) {
    throw new Error("Extension command entry is not executable");
  }
  await Promise.resolve(command({ arguments: argumentValues }));
  const last = feedback.at(-1);
  if (!last) {
    return { ok: true, message: "Extension command completed." };
  }
  const style = (last.style ?? "").toLowerCase();
  const ok = style !== "failure";
  return {
    ok,
    message: formatRuntimeFeedback(last)
  };
}
function unsupportedModeError() {
  const err = new Error("Only no-view extension commands are executable in this runtime.");
  err.code = RUNTIME_UNSUPPORTED_MODE;
  return err;
}
function isUnsupportedRuntimeModeError(error) {
  if (!error || typeof error !== "object") return false;
  return error.code === RUNTIME_UNSUPPORTED_MODE;
}
async function executeExtensionCommandRuntime(extensionId, commandName, argumentValues) {
  const pkg = await ensureExtensionBundle(extensionId);
  if (!pkg) {
    throw new Error(`Runtime bundle not available for extension: ${extensionId}`);
  }
  const commandMeta = (pkg.commands ?? []).find((command) => command.name === commandName);
  if (!commandMeta) {
    throw new Error(`Command not found: ${commandName}`);
  }
  const mode = (commandMeta.mode ?? "").toLowerCase();
  if (mode && mode !== "no-view") {
    throw unsupportedModeError();
  }
  const meta = readInstallMeta(extensionId);
  if (meta?.missingScripts.includes(commandName)) {
    throw new Error(
      `No prebuilt script for ${commandName}. This extension doesn't ship an executable .sc-build file for this command.`
    );
  }
  const scriptPath = scriptPathForInstalledExtensionCommand(extensionId, commandName);
  if (!(0, import_node_fs5.existsSync)(scriptPath)) {
    throw new Error(`Missing command script: ${commandName}.js`);
  }
  return await executeNoViewScript(extensionId, commandName, scriptPath, argumentValues);
}
async function getStoreCatalog() {
  const now = Date.now();
  if (catalogCache2 && now - catalogCache2.fetchedAt < CATALOG_CACHE_TTL_MS) {
    return catalogCache2.catalog;
  }
  try {
    const catalog = await fetchRaycastCatalogFromAwesome();
    catalogCache2 = { fetchedAt: now, catalog };
    return catalog;
  } catch (error) {
    console.warn("[extensions] failed to refresh Raycast catalog from Awesome:", error);
    try {
      const catalog = await fetchRaycastCatalogFromGithub();
      if (catalog.length > 0) {
        catalogCache2 = { fetchedAt: now, catalog };
        return catalog;
      }
    } catch (innerError) {
      console.warn("[extensions] failed to refresh Raycast catalog from Github:", innerError);
    }
    return catalogCache2?.catalog ?? [];
  }
}
function scoreMatch(item, q) {
  const name = item.name.toLowerCase();
  const description = item.description.toLowerCase();
  const slug = item.id.startsWith("raycast.") ? item.id.slice("raycast.".length).toLowerCase() : item.id.toLowerCase();
  let score = 0;
  if (name === q || slug === q) score += 1e3;
  else if (name.startsWith(q) || slug.startsWith(q)) score += 800;
  const nameWords = name.split(/[-_\s.]/g);
  if (nameWords.some((word) => word.startsWith(q))) score += 600;
  const descWords = description.split(/[-_\s.]/g);
  if (descWords.some((word) => word.startsWith(q))) score += 500;
  if (name.includes(q) || slug.includes(q)) score += 200;
  if (description.includes(q)) score += 100;
  if (item.downloadCount && item.downloadCount > 0) {
    score += Math.log10(item.downloadCount) * 50;
  }
  return score;
}
function listInstalledExtensions() {
  const db = readDb();
  return [...db.installed].sort(byName);
}
async function searchStoreExtensions(query) {
  const q = query.trim().toLowerCase();
  const catalog = await getStoreCatalog();
  if (!q) {
    return catalog.sort((a, b) => (b.downloadCount ?? 0) - (a.downloadCount ?? 0));
  }
  const scored = catalog.map((item) => ({ item, score: scoreMatch(item, q) })).filter((entry) => entry.score > 0);
  const fuse = new Fuse(catalog, {
    keys: [
      { name: "name", weight: 0.7 },
      { name: "description", weight: 0.3 }
    ],
    threshold: 0.4,
    includeScore: true
  });
  const fuzzyResults = fuse.search(q);
  const fuzzyMap = /* @__PURE__ */ new Map();
  fuzzyResults.forEach((res) => {
    if (res.score !== void 0) {
      fuzzyMap.set(res.item.id, (1 - res.score) * 500);
    }
  });
  const finalResults = catalog.map((item) => {
    let score = scoreMatch(item, q);
    const fuzzyBoost = fuzzyMap.get(item.id) ?? 0;
    score += fuzzyBoost;
    return { item, score };
  }).filter((entry) => entry.score > 0).sort((a, b) => b.score - a.score || byName(a.item, b.item));
  return finalResults.map((entry) => entry.item);
}
async function installExtension2(extensionId) {
  const catalog = await getStoreCatalog();
  const manifest = catalog.find((item) => item.id === extensionId);
  if (!manifest) {
    throw new Error(`Extension not found in store: ${extensionId}`);
  }
  const db = readDb();
  const existing = db.installed.find((item) => item.id === extensionId);
  if (existing) {
    void ensureExtensionBundle(extensionId);
    return existing;
  }
  const next = {
    ...manifest,
    installedAt: Date.now()
  };
  db.installed.push(next);
  writeDb(db);
  void ensureExtensionBundle(extensionId);
  return next;
}
async function getExtensionCommands(extensionId) {
  const cached = commandCache.get(extensionId);
  if (cached) return cached;
  const localPkg = await ensureExtensionBundle(extensionId);
  if (localPkg) {
    const commands = normalizeExtensionCommandsFromPackage(localPkg);
    commandCache.set(extensionId, commands);
    return commands;
  }
  const slug = extensionSlugFromId(extensionId);
  const url = `https://raw.githubusercontent.com/raycast/extensions/${RAYCAST_EXTENSIONS_REF}/${RAYCAST_EXTENSIONS_PATH}/${slug}/package.json`;
  try {
    const pkg = await fetchGithubJson(url);
    const commands = normalizeExtensionCommandsFromPackage(pkg);
    commandCache.set(extensionId, commands);
    return commands;
  } catch {
    commandCache.set(extensionId, []);
    return [];
  }
}
function uninstallExtension2(extensionId) {
  const db = readDb();
  const before = db.installed.length;
  db.installed = db.installed.filter((item) => item.id !== extensionId);
  if (db.installed.length === before) return false;
  writeDb(db);
  commandCache.delete(extensionId);
  installErrors.delete(extensionId);
  (0, import_node_fs5.rmSync)(installedPackageRoot(extensionId), { recursive: true, force: true });
  (0, import_node_fs5.rmSync)(backupPackageRoot(extensionId), { recursive: true, force: true });
  return true;
}
var import_node_crypto3, import_node_module2, import_node_path6, import_node_fs5, import_node_os4, RAYCAST_EXTENSIONS_REPO, RAYCAST_EXTENSIONS_REF, RAYCAST_EXTENSIONS_PATH, CATALOG_CACHE_TTL_MS, RUNTIME_UNSUPPORTED_MODE, DEFAULT_DB, catalogCache2, commandCache, AWESOME_RAYCAST_DATA, installErrors;
var init_service = __esm({
  "src/main/extensions/service.ts"() {
    "use strict";
    init_desktop();
    import_node_crypto3 = require("node:crypto");
    import_node_module2 = require("node:module");
    import_node_path6 = require("node:path");
    import_node_fs5 = require("node:fs");
    import_node_os4 = require("node:os");
    init_fuse();
    init_raycastShim();
    RAYCAST_EXTENSIONS_REPO = "https://github.com/raycast/extensions";
    RAYCAST_EXTENSIONS_REF = "c0e624ee0420679ed3aa296c25c1a6f29938c56a";
    RAYCAST_EXTENSIONS_PATH = "extensions";
    CATALOG_CACHE_TTL_MS = 10 * 6e4;
    RUNTIME_UNSUPPORTED_MODE = "RUNTIME_UNSUPPORTED_MODE";
    DEFAULT_DB = {
      installed: []
    };
    catalogCache2 = null;
    commandCache = /* @__PURE__ */ new Map();
    AWESOME_RAYCAST_DATA = "https://raw.githubusercontent.com/j3lte/awesome-raycast/main/data/data.json";
    installErrors = /* @__PURE__ */ new Map();
  }
});

// src/main/extensionWorker/main.ts
var import_node_util4 = require("node:util");

// src/main/extensionWorker/framing.ts
var MAX_FRAME_BYTES = 16 * 1024 * 1024;
var FrameReader = class {
  constructor(accept, limit = MAX_FRAME_BYTES) {
    this.accept = accept;
    this.limit = limit;
  }
  accept;
  limit;
  buffered = Buffer.alloc(0);
  decoder = new TextDecoder("utf-8", { fatal: true });
  push(chunk) {
    this.buffered = Buffer.concat([this.buffered, chunk]);
    for (; ; ) {
      const newline = this.buffered.indexOf(10);
      if (newline < 0) break;
      if (newline > this.limit) throw new Error("Extension protocol frame too large");
      const frame = this.buffered.subarray(0, newline);
      this.buffered = this.buffered.subarray(newline + 1);
      this.accept(this.decoder.decode(frame));
    }
    if (this.buffered.length > this.limit) throw new Error("Extension protocol frame too large");
  }
  finish() {
    if (this.buffered.length) throw new Error("Truncated extension protocol frame");
  }
};

// src/main/extensionWorker/main.ts
init_host();
init_services();
init_extensionRepository();
for (const method of ["log", "info", "warn", "error", "debug"]) {
  console[method] = (...args) => {
    process.stderr.write(`${(0, import_node_util4.format)(...args)}
`);
  };
}
async function main() {
  const [runner, registry, legacy] = await Promise.all([
    Promise.resolve().then(() => (init_extension_runner(), extension_runner_exports)),
    Promise.resolve().then(() => (init_extension_registry(), extension_registry_exports)),
    Promise.resolve().then(() => (init_service(), service_exports))
  ]);
  registry.extensionRegistryEvents.on(
    "progress",
    (payload) => send({ type: "event", channel: "extension:install-progress", payload })
  );
  async function invoke(channel, args) {
    const body = args[0];
    const id = () => {
      if (typeof args[0] !== "string" || !/^(?:raycast\.)?[a-zA-Z0-9][a-zA-Z0-9._-]*$/.test(args[0])) {
        throw new Error("A valid extension id is required");
      }
      return args[0];
    };
    const session = () => {
      if (typeof body?.sessionId !== "string") throw new Error("sessionId is required");
      return { sessionId: body.sessionId };
    };
    switch (channel) {
      case "extension:run-command": {
        if (typeof body?.extensionId !== "string" || typeof body.commandName !== "string")
          throw new Error("extensionId and commandName are required");
        await refreshHostSnapshot();
        const argumentValues = body.argumentValues && typeof body.argumentValues === "object" ? Object.fromEntries(
          Object.entries(body.argumentValues).map(([key, value]) => [
            key,
            String(value ?? "")
          ])
        ) : void 0;
        return runner.runExtensionCommand({
          extensionId: body.extensionId,
          commandName: body.commandName,
          argumentValues
        });
      }
      case "extension:invoke-action": {
        if (typeof body?.actionId !== "string") throw new Error("actionId is required");
        const formValues = body.formValues && typeof body.formValues === "object" ? Object.fromEntries(
          Object.entries(body.formValues).map(([key, value]) => [
            key,
            Array.isArray(value) ? value.filter((entry) => typeof entry === "string") : ["string", "number", "boolean"].includes(typeof value) ? value : String(value ?? "")
          ])
        ) : void 0;
        return runner.invokeExtensionAction({ ...session(), actionId: body.actionId, formValues });
      }
      case "extension:search-text-changed":
        return runner.updateSearchText({ ...session(), searchText: String(body?.searchText ?? "") });
      case "extension:refresh-session":
        return runner.refreshExtensionSession(session());
      case "extension:load-more":
        return runner.loadMoreExtensionSession(session());
      case "extension:dispose-session":
        return runner.disposeExtensionSession(session().sessionId);
      case "extension:install": {
        const repository = typeof args[0] === "string" ? parseGitHubRepositoryUrl(args[0]) : null;
        return registry.installRegistryExtension(repository?.url || id());
      }
      case "extensions:install": {
        const extensionId = id();
        const result = await legacy.installExtension(extensionId);
        await legacy.reinstallExtension(extensionId);
        return result;
      }
      case "extensions:reinstall":
        return legacy.reinstallExtension(id());
      case "core:extensions:shutdown":
        runner.clearAllExtensionSessions();
        return true;
      default:
        throw new Error(`Extension Worker does not handle ${channel}`);
    }
  }
  function acceptFrame(line) {
    let frame;
    try {
      frame = JSON.parse(line);
    } catch {
      throw new Error("Invalid extension protocol frame");
    }
    if (frame.type === "core-reply") {
      acceptReply(frame);
      return;
    }
    if (frame.type !== "invoke" || typeof frame.id !== "string" || typeof frame.channel !== "string")
      return;
    void invoke(frame.channel, Array.isArray(frame.payload) ? frame.payload : []).then(
      (result) => send({ type: "reply", id: frame.id, result: result ?? null }),
      (error) => send({
        type: "reply",
        id: frame.id,
        error: error instanceof Error ? error.message : String(error)
      })
    );
  }
  send({ type: "ready", protocol: 1 });
  const frames = new FrameReader(acceptFrame);
  const bun = globalThis.Bun;
  if (bun) {
    const reader = bun.stdin.stream().getReader();
    try {
      for (; ; ) {
        const { done, value } = await reader.read();
        if (done) break;
        frames.push(value);
      }
    } finally {
      reader.releaseLock();
    }
  } else {
    for await (const chunk of process.stdin)
      frames.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }
  frames.finish();
  disconnect();
  runner.clearAllExtensionSessions();
  process.exit(0);
}
void main().catch((error) => {
  console.error(error);
  process.exit(1);
});
//# sourceMappingURL=extension-worker.js.map
