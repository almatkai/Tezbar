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

// src/main/electron-shim.ts
function makeNativeImage(sourcePath) {
  const image = {
    sourcePath,
    isEmpty: () => !sourcePath || !(0, import_node_fs.existsSync)(sourcePath),
    setTemplateImage: () => void 0,
    getSize: () => ({ width: 0, height: 0 }),
    resize: () => image,
    toPNG: () => sourcePath && (0, import_node_fs.existsSync)(sourcePath) ? (0, import_node_fs.readFileSync)(sourcePath) : Buffer.alloc(0)
  };
  return image;
}
var import_node_path, import_node_os, import_node_child_process, import_node_fs, import_node_util, execFileAsync, backendWebContents, IpcMain, ipcMain, app, shell, clipboard, dialog, session, screen, desktopCapturer, nativeImage, globalShortcut, BrowserWindow, systemPreferences;
var init_electron_shim = __esm({
  "src/main/electron-shim.ts"() {
    "use strict";
    import_node_path = require("node:path");
    import_node_os = require("node:os");
    import_node_child_process = require("node:child_process");
    import_node_fs = require("node:fs");
    import_node_util = require("node:util");
    execFileAsync = (0, import_node_util.promisify)(import_node_child_process.execFile);
    backendWebContents = {
      id: 1,
      send(channel, payload) {
        process.stdout.write(`${JSON.stringify({ type: "event", channel, payload })}
`);
      },
      isDestroyed() {
        return false;
      },
      once() {
      }
    };
    IpcMain = class {
      // Map containing channel handlers
      _handlers = /* @__PURE__ */ new Map();
      handle(channel, callback) {
        this._handlers.set(channel, callback);
      }
      on(channel, callback) {
        this._handlers.set(channel, callback);
      }
      // Trigger a registered IPC handler from the outside
      async _invoke(channel, ...args) {
        const handler = this._handlers.get(channel);
        if (!handler) {
          throw new Error(`No handler registered for channel: ${channel}`);
        }
        return handler({ sender: backendWebContents }, ...args);
      }
    };
    ipcMain = new IpcMain();
    app = {
      isPackaged: process.env.IS_TAURI === "true",
      name: "Tezbar",
      getPath(name) {
        if (name === "userData") {
          return process.env.APPDATA_DIR || (0, import_node_path.join)((0, import_node_os.homedir)(), ".tezbar");
        }
        if (name === "temp") {
          return process.env.TEMP_DIR || (0, import_node_os.tmpdir)();
        }
        if (name === "home") {
          return (0, import_node_os.homedir)();
        }
        return (0, import_node_path.join)((0, import_node_os.homedir)(), `.${name}`);
      },
      getVersion() {
        return process.env.APP_VERSION || "0.0.3";
      },
      getName() {
        return "Tezbar";
      },
      getAppPath() {
        return process.cwd();
      },
      focus() {
      },
      hide() {
        if (process.env.IS_TAURI === "true") {
          process.stdout.write(`${JSON.stringify({ type: "app_visibility", visible: false })}
`);
        }
      },
      show() {
        if (process.env.IS_TAURI === "true") {
          process.stdout.write(`${JSON.stringify({ type: "app_visibility", visible: true })}
`);
        }
      },
      once() {
      },
      quit() {
        process.stdout.write(`${JSON.stringify({ type: "app_quit" })}
`);
      },
      exit() {
        process.stdout.write(`${JSON.stringify({ type: "app_quit" })}
`);
      }
    };
    shell = {
      async openExternal(url) {
        const command = process.platform === "darwin" ? "open" : "xdg-open";
        await execFileAsync(command, [url]);
      },
      async openPath(target) {
        const command = process.platform === "darwin" ? "open" : "xdg-open";
        try {
          await execFileAsync(command, [target]);
          return "";
        } catch (error) {
          return error instanceof Error ? error.message : String(error);
        }
      },
      showItemInFolder(target) {
        if (process.platform === "darwin") void execFileAsync("open", ["-R", target]);
        else void execFileAsync("xdg-open", [(0, import_node_path.join)(target, "..")]);
      }
    };
    clipboard = {
      readText() {
        try {
          return (0, import_node_child_process.execFileSync)("pbpaste", [], { encoding: "utf8" });
        } catch {
          return "";
        }
      },
      writeText(text) {
        try {
          const child = (0, import_node_child_process.spawn)("pbcopy");
          child.stdin.write(text);
          child.stdin.end();
        } catch {
        }
      },
      availableFormats() {
        return this.readText() ? ["text/plain"] : [];
      },
      read() {
        return "";
      },
      readImage() {
        return makeNativeImage();
      },
      writeImage(image) {
        if (!image.sourcePath || process.platform !== "darwin") return;
        const escaped = image.sourcePath.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
        void execFileAsync("osascript", [
          "-e",
          `set the clipboard to (read POSIX file "${escaped}" as PNG picture)`
        ]);
      },
      write(payload) {
        if (payload.text) this.writeText(payload.text);
      },
      clear() {
        this.writeText("");
      }
    };
    dialog = {
      async showMessageBox(windowOrOptions, maybeOptions) {
        const options = maybeOptions ?? windowOrOptions ?? {};
        const buttons = Array.isArray(options.buttons) && options.buttons.length > 0 ? options.buttons.map(String) : ["OK"];
        if (process.platform !== "darwin") return { response: options.cancelId ?? 0 };
        const escapeAppleScript = (value) => String(value ?? "").replace(/\\/g, "\\\\").replace(/"/g, '\\"');
        const buttonList = buttons.map((button) => `"${escapeAppleScript(button)}"`).join(", ");
        const defaultIndex = Math.min(Math.max(Number(options.defaultId) || 0, 0), buttons.length - 1);
        const cancelIndex = Math.min(Math.max(Number(options.cancelId) || 0, 0), buttons.length - 1);
        const message = [options.message, options.detail].filter(Boolean).join("\n\n");
        const script = `display dialog "${escapeAppleScript(message)}" with title "${escapeAppleScript(options.title ?? "Tezbar")}" buttons {${buttonList}} default button "${escapeAppleScript(buttons[defaultIndex])}" cancel button "${escapeAppleScript(buttons[cancelIndex])}"`;
        process.stdout.write(`${JSON.stringify({ type: "window_suppress_blur", value: true })}
`);
        try {
          const { stdout } = await execFileAsync("osascript", ["-e", script]);
          const selected = buttons.findIndex((button) => stdout.includes(`button returned:${button}`));
          return { response: selected >= 0 ? selected : cancelIndex };
        } catch {
          return { response: cancelIndex };
        } finally {
          process.stdout.write(`${JSON.stringify({ type: "window_suppress_blur", value: false })}
`);
        }
      }
    };
    session = {
      defaultSession: {
        async clearCache() {
        },
        async clearStorageData() {
        },
        setPermissionRequestHandler() {
        },
        setPermissionCheckHandler() {
        }
      }
    };
    screen = {
      getDisplayNearestPoint() {
        return {
          id: 1,
          size: { width: 1920, height: 1080 },
          bounds: { x: 0, y: 0, width: 1920, height: 1080 },
          workArea: { x: 0, y: 0, width: 1920, height: 1080 }
        };
      },
      getCursorScreenPoint() {
        return { x: 0, y: 0 };
      },
      getAllDisplays() {
        return [this.getDisplayNearestPoint()];
      }
    };
    desktopCapturer = {
      async getSources() {
        return [];
      }
    };
    nativeImage = {
      createFromPath(path7) {
        return makeNativeImage(path7);
      },
      createFromDataURL() {
        return makeNativeImage();
      }
    };
    globalShortcut = {
      register() {
        return true;
      },
      unregister() {
      },
      unregisterAll() {
      }
    };
    BrowserWindow = class _BrowserWindow {
      static windows = [];
      static getAllWindows() {
        return [..._BrowserWindow.windows];
      }
      static getFocusedWindow() {
        return _BrowserWindow.windows[0] ?? null;
      }
      static fromWebContents() {
        return _BrowserWindow.windows[0] ?? null;
      }
      webContents = backendWebContents;
      visible = true;
      opacity = 1;
      contentSize = [760, 640];
      constructor() {
        _BrowserWindow.windows.push(this);
      }
      isDestroyed() {
        return false;
      }
      isVisible() {
        return this.visible;
      }
      destroy() {
        _BrowserWindow.windows = _BrowserWindow.windows.filter((window2) => window2 !== this);
      }
      close() {
        this.destroy();
      }
      focus() {
      }
      show() {
        this.visible = true;
      }
      hide() {
        this.visible = false;
      }
      getContentSize() {
        return this.contentSize;
      }
      setContentSize(width, height) {
        this.contentSize = [width, height];
      }
      getOpacity() {
        return this.opacity;
      }
      setOpacity(value) {
        this.opacity = value;
      }
      setContentProtection(enabled) {
        void enabled;
      }
      setMaximumSize() {
      }
    };
    systemPreferences = {
      isTrusted() {
        return true;
      },
      isTrustedAccessibilityClient() {
        return false;
      },
      async askForMediaAccess() {
        return false;
      }
    };
  }
});

// src/main/windowState.ts
function setSuppressBlurHide(value) {
  suppressBlurHide = value;
}
var suppressBlurHide;
var init_windowState = __esm({
  "src/main/windowState.ts"() {
    "use strict";
    suppressBlurHide = false;
  }
});

// src/shared/aiProviders.ts
function isCustomProvider(id) {
  return id.startsWith("custom:");
}
function recommendedModel(provider) {
  return isCustomProvider(provider) ? "" : RECOMMENDED_AI_MODEL[provider];
}
function defaultModels(provider) {
  return isCustomProvider(provider) ? [] : DEFAULT_PROVIDER_MODELS[provider];
}
function inferCapabilities(modelId) {
  const lower = modelId.toLowerCase();
  const caps = [];
  if (/vision|vl|llava|gpt-4o|gemini|claude/.test(lower)) caps.unshift("vision");
  if (/reason|think|r1|o\d|sonnet|pro|v4-pro|claude|deepseek/.test(lower)) caps.push("thinking");
  if (!/embed|whisper|tts/.test(lower)) caps.push("tools");
  return Array.from(new Set(caps));
}
function normalizeModelList(models, fallbackId) {
  const seen = /* @__PURE__ */ new Set();
  const normalized = models.map((model) => {
    const next = {
      id: String(model.id || "").trim(),
      capabilities: Array.isArray(model.capabilities) ? model.capabilities.filter(
        (capability) => capability === "vision" || capability === "thinking" || capability === "tools"
      ) : inferCapabilities(model.id)
    };
    if (typeof model.contextWindow === "number" && Number.isFinite(model.contextWindow)) {
      next.contextWindow = Math.max(0, Math.round(model.contextWindow));
    }
    return next;
  }).filter((model) => {
    if (!model.id || seen.has(model.id)) return false;
    seen.add(model.id);
    return true;
  });
  if (fallbackId && !seen.has(fallbackId)) {
    normalized.unshift({ id: fallbackId, capabilities: inferCapabilities(fallbackId) });
  }
  return normalized;
}
function normalizeProviderModelList(provider, models) {
  if (isCustomProvider(provider)) {
    return normalizeModelList(models, models[0]?.id ?? "");
  }
  if (provider === "openai-compatible") {
    return normalizeModelList(models, RECOMMENDED_AI_MODEL[provider]);
  }
  const ownDefaults = new Set(DEFAULT_PROVIDER_MODELS[provider].map((model) => model.id));
  const otherDefaults = /* @__PURE__ */ new Set();
  for (const [otherProvider, otherModels] of Object.entries(DEFAULT_PROVIDER_MODELS)) {
    if (otherProvider === provider) continue;
    for (const model of otherModels) {
      otherDefaults.add(model.id);
    }
  }
  return normalizeModelList(
    models.filter((model) => ownDefaults.has(model.id) || !otherDefaults.has(model.id)),
    RECOMMENDED_AI_MODEL[provider]
  );
}
var RECOMMENDED_AI_MODEL, DEFAULT_PROVIDER_MODELS;
var init_aiProviders = __esm({
  "src/shared/aiProviders.ts"() {
    "use strict";
    RECOMMENDED_AI_MODEL = {
      openai: "gpt-4o-mini",
      deepseek: "deepseek-v4-flash",
      "openai-compatible": "gpt-4o-mini",
      gemini: "gemini-2.0-flash",
      anthropic: "claude-3-5-haiku-20241022",
      ollama: "llama3.2",
      copilot: "gpt-4o",
      opencode: "opencode/big-pickle"
    };
    DEFAULT_PROVIDER_MODELS = {
      openai: [
        { id: "gpt-4o-mini", capabilities: ["vision", "tools"], contextWindow: 128e3 },
        { id: "gpt-4o", capabilities: ["vision", "tools"], contextWindow: 128e3 },
        { id: "o3-mini", capabilities: ["thinking", "tools"], contextWindow: 2e5 }
      ],
      deepseek: [
        { id: "deepseek-v4-flash", capabilities: ["tools"], contextWindow: 128e3 },
        { id: "deepseek-v4-pro", capabilities: ["thinking", "tools"], contextWindow: 128e3 },
        { id: "deepseek-reasoner", capabilities: ["thinking"], contextWindow: 64e3 }
      ],
      "openai-compatible": [
        { id: "gpt-4o-mini", capabilities: ["vision", "tools"], contextWindow: 128e3 }
      ],
      gemini: [
        { id: "gemini-2.0-flash", capabilities: ["vision", "tools"], contextWindow: 1e6 },
        { id: "gemini-1.5-pro", capabilities: ["vision", "thinking", "tools"], contextWindow: 2e6 }
      ],
      anthropic: [
        { id: "claude-3-5-haiku-20241022", capabilities: ["vision", "tools"], contextWindow: 2e5 },
        { id: "claude-3-5-sonnet-20241022", capabilities: ["vision", "thinking", "tools"], contextWindow: 2e5 }
      ],
      ollama: [
        { id: "llama3.2", capabilities: ["tools"], contextWindow: 128e3 },
        { id: "llava", capabilities: ["vision"], contextWindow: 32e3 }
      ],
      copilot: [
        { id: "gpt-4o", capabilities: ["vision", "tools"], contextWindow: 128e3 },
        { id: "claude-3.5-sonnet", capabilities: ["thinking", "tools"], contextWindow: 2e5 }
      ],
      opencode: [
        { id: "opencode/big-pickle", capabilities: ["thinking", "tools"], contextWindow: 128e3 }
      ]
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/tslib.mjs
function __classPrivateFieldSet(receiver, state, value, kind, f) {
  if (kind === "m")
    throw new TypeError("Private method is not writable");
  if (kind === "a" && !f)
    throw new TypeError("Private accessor was defined without a setter");
  if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver))
    throw new TypeError("Cannot write private member to an object whose class did not declare it");
  return kind === "a" ? f.call(receiver, value) : f ? f.value = value : state.set(receiver, value), value;
}
function __classPrivateFieldGet(receiver, state, kind, f) {
  if (kind === "a" && !f)
    throw new TypeError("Private accessor was defined without a getter");
  if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver))
    throw new TypeError("Cannot read private member from an object whose class did not declare it");
  return kind === "m" ? f : kind === "a" ? f.call(receiver) : f ? f.value : state.get(receiver);
}
var init_tslib = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/tslib.mjs"() {
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/utils/uuid.mjs
var uuid4;
var init_uuid = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/utils/uuid.mjs"() {
    uuid4 = function() {
      const { crypto } = globalThis;
      if (crypto?.randomUUID) {
        uuid4 = crypto.randomUUID.bind(crypto);
        return crypto.randomUUID();
      }
      const u8 = new Uint8Array(1);
      const randomByte = crypto ? () => crypto.getRandomValues(u8)[0] : () => Math.random() * 255 & 255;
      return "10000000-1000-4000-8000-100000000000".replace(/[018]/g, (c) => (+c ^ randomByte() & 15 >> +c / 4).toString(16));
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/errors.mjs
function isAbortError(err) {
  return typeof err === "object" && err !== null && // Spec-compliant fetch implementations
  ("name" in err && err.name === "AbortError" || // Expo fetch
  "message" in err && String(err.message).includes("FetchRequestCanceledException"));
}
var castToError;
var init_errors = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/errors.mjs"() {
    castToError = (err) => {
      if (err instanceof Error)
        return err;
      if (typeof err === "object" && err !== null) {
        try {
          if (Object.prototype.toString.call(err) === "[object Error]") {
            const error = new Error(err.message, err.cause ? { cause: err.cause } : {});
            if (err.stack)
              error.stack = err.stack;
            if (err.cause && !error.cause)
              error.cause = err.cause;
            if (err.name)
              error.name = err.name;
            return error;
          }
        } catch {
        }
        try {
          return new Error(JSON.stringify(err));
        } catch {
        }
      }
      return new Error(err);
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/core/error.mjs
var AnthropicError, APIError, APIUserAbortError, APIConnectionError, APIConnectionTimeoutError, BadRequestError, AuthenticationError, PermissionDeniedError, NotFoundError, ConflictError, UnprocessableEntityError, RateLimitError, InternalServerError;
var init_error = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/core/error.mjs"() {
    init_errors();
    AnthropicError = class extends Error {
    };
    APIError = class _APIError extends AnthropicError {
      constructor(status, error, message, headers, type) {
        super(`${_APIError.makeMessage(status, error, message)}`);
        this.status = status;
        this.headers = headers;
        this.requestID = headers?.get("request-id");
        this.error = error;
        this.type = type ?? null;
      }
      static makeMessage(status, error, message) {
        const msg = error?.message ? typeof error.message === "string" ? error.message : JSON.stringify(error.message) : error ? JSON.stringify(error) : message;
        if (status && msg) {
          return `${status} ${msg}`;
        }
        if (status) {
          return `${status} status code (no body)`;
        }
        if (msg) {
          return msg;
        }
        return "(no status code or body)";
      }
      static generate(status, errorResponse, message, headers) {
        if (!status || !headers) {
          return new APIConnectionError({ message, cause: castToError(errorResponse) });
        }
        const error = errorResponse;
        const type = error?.["error"]?.["type"];
        if (status === 400) {
          return new BadRequestError(status, error, message, headers, type);
        }
        if (status === 401) {
          return new AuthenticationError(status, error, message, headers, type);
        }
        if (status === 403) {
          return new PermissionDeniedError(status, error, message, headers, type);
        }
        if (status === 404) {
          return new NotFoundError(status, error, message, headers, type);
        }
        if (status === 409) {
          return new ConflictError(status, error, message, headers, type);
        }
        if (status === 422) {
          return new UnprocessableEntityError(status, error, message, headers, type);
        }
        if (status === 429) {
          return new RateLimitError(status, error, message, headers, type);
        }
        if (status >= 500) {
          return new InternalServerError(status, error, message, headers, type);
        }
        return new _APIError(status, error, message, headers, type);
      }
    };
    APIUserAbortError = class extends APIError {
      constructor({ message } = {}) {
        super(void 0, void 0, message || "Request was aborted.", void 0);
      }
    };
    APIConnectionError = class extends APIError {
      constructor({ message, cause }) {
        super(void 0, void 0, message || "Connection error.", void 0);
        if (cause)
          this.cause = cause;
      }
    };
    APIConnectionTimeoutError = class extends APIConnectionError {
      constructor({ message } = {}) {
        super({ message: message ?? "Request timed out." });
      }
    };
    BadRequestError = class extends APIError {
    };
    AuthenticationError = class extends APIError {
    };
    PermissionDeniedError = class extends APIError {
    };
    NotFoundError = class extends APIError {
    };
    ConflictError = class extends APIError {
    };
    UnprocessableEntityError = class extends APIError {
    };
    RateLimitError = class extends APIError {
    };
    InternalServerError = class extends APIError {
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/utils/values.mjs
function maybeObj(x) {
  if (typeof x !== "object") {
    return {};
  }
  return x ?? {};
}
function isEmptyObj(obj) {
  if (!obj)
    return true;
  for (const _k in obj)
    return false;
  return true;
}
function hasOwn(obj, key) {
  return Object.prototype.hasOwnProperty.call(obj, key);
}
var startsWithSchemeRegexp, isAbsoluteURL, isArray, isReadonlyArray, validatePositiveInteger, safeJSON;
var init_values = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/utils/values.mjs"() {
    init_error();
    startsWithSchemeRegexp = /^[a-z][a-z0-9+.-]*:/i;
    isAbsoluteURL = (url) => {
      return startsWithSchemeRegexp.test(url);
    };
    isArray = (val) => (isArray = Array.isArray, isArray(val));
    isReadonlyArray = isArray;
    validatePositiveInteger = (name, n) => {
      if (typeof n !== "number" || !Number.isInteger(n)) {
        throw new AnthropicError(`${name} must be an integer`);
      }
      if (n < 0) {
        throw new AnthropicError(`${name} must be a positive integer`);
      }
      return n;
    };
    safeJSON = (text) => {
      try {
        return JSON.parse(text);
      } catch (err) {
        return void 0;
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/utils/sleep.mjs
var sleep;
var init_sleep = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/utils/sleep.mjs"() {
    sleep = (ms) => new Promise((resolve4) => setTimeout(resolve4, ms));
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/version.mjs
var VERSION;
var init_version = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/version.mjs"() {
    VERSION = "0.90.0";
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/detect-platform.mjs
function getDetectedPlatform() {
  if (typeof Deno !== "undefined" && Deno.build != null) {
    return "deno";
  }
  if (typeof EdgeRuntime !== "undefined") {
    return "edge";
  }
  if (Object.prototype.toString.call(typeof globalThis.process !== "undefined" ? globalThis.process : 0) === "[object process]") {
    return "node";
  }
  return "unknown";
}
function getBrowserInfo() {
  if (typeof navigator === "undefined" || !navigator) {
    return null;
  }
  const browserPatterns = [
    { key: "edge", pattern: /Edge(?:\W+(\d+)\.(\d+)(?:\.(\d+))?)?/ },
    { key: "ie", pattern: /MSIE(?:\W+(\d+)\.(\d+)(?:\.(\d+))?)?/ },
    { key: "ie", pattern: /Trident(?:.*rv\:(\d+)\.(\d+)(?:\.(\d+))?)?/ },
    { key: "chrome", pattern: /Chrome(?:\W+(\d+)\.(\d+)(?:\.(\d+))?)?/ },
    { key: "firefox", pattern: /Firefox(?:\W+(\d+)\.(\d+)(?:\.(\d+))?)?/ },
    { key: "safari", pattern: /(?:Version\W+(\d+)\.(\d+)(?:\.(\d+))?)?(?:\W+Mobile\S*)?\W+Safari/ }
  ];
  for (const { key, pattern } of browserPatterns) {
    const match = pattern.exec(navigator.userAgent);
    if (match) {
      const major = match[1] || 0;
      const minor = match[2] || 0;
      const patch = match[3] || 0;
      return { browser: key, version: `${major}.${minor}.${patch}` };
    }
  }
  return null;
}
var isRunningInBrowser, getPlatformProperties, normalizeArch, normalizePlatform, _platformHeaders, getPlatformHeaders;
var init_detect_platform = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/detect-platform.mjs"() {
    init_version();
    isRunningInBrowser = () => {
      return (
        // @ts-ignore
        typeof window !== "undefined" && // @ts-ignore
        typeof window.document !== "undefined" && // @ts-ignore
        typeof navigator !== "undefined"
      );
    };
    getPlatformProperties = () => {
      const detectedPlatform = getDetectedPlatform();
      if (detectedPlatform === "deno") {
        return {
          "X-Stainless-Lang": "js",
          "X-Stainless-Package-Version": VERSION,
          "X-Stainless-OS": normalizePlatform(Deno.build.os),
          "X-Stainless-Arch": normalizeArch(Deno.build.arch),
          "X-Stainless-Runtime": "deno",
          "X-Stainless-Runtime-Version": typeof Deno.version === "string" ? Deno.version : Deno.version?.deno ?? "unknown"
        };
      }
      if (typeof EdgeRuntime !== "undefined") {
        return {
          "X-Stainless-Lang": "js",
          "X-Stainless-Package-Version": VERSION,
          "X-Stainless-OS": "Unknown",
          "X-Stainless-Arch": `other:${EdgeRuntime}`,
          "X-Stainless-Runtime": "edge",
          "X-Stainless-Runtime-Version": globalThis.process.version
        };
      }
      if (detectedPlatform === "node") {
        return {
          "X-Stainless-Lang": "js",
          "X-Stainless-Package-Version": VERSION,
          "X-Stainless-OS": normalizePlatform(globalThis.process.platform ?? "unknown"),
          "X-Stainless-Arch": normalizeArch(globalThis.process.arch ?? "unknown"),
          "X-Stainless-Runtime": "node",
          "X-Stainless-Runtime-Version": globalThis.process.version ?? "unknown"
        };
      }
      const browserInfo = getBrowserInfo();
      if (browserInfo) {
        return {
          "X-Stainless-Lang": "js",
          "X-Stainless-Package-Version": VERSION,
          "X-Stainless-OS": "Unknown",
          "X-Stainless-Arch": "unknown",
          "X-Stainless-Runtime": `browser:${browserInfo.browser}`,
          "X-Stainless-Runtime-Version": browserInfo.version
        };
      }
      return {
        "X-Stainless-Lang": "js",
        "X-Stainless-Package-Version": VERSION,
        "X-Stainless-OS": "Unknown",
        "X-Stainless-Arch": "unknown",
        "X-Stainless-Runtime": "unknown",
        "X-Stainless-Runtime-Version": "unknown"
      };
    };
    normalizeArch = (arch) => {
      if (arch === "x32")
        return "x32";
      if (arch === "x86_64" || arch === "x64")
        return "x64";
      if (arch === "arm")
        return "arm";
      if (arch === "aarch64" || arch === "arm64")
        return "arm64";
      if (arch)
        return `other:${arch}`;
      return "unknown";
    };
    normalizePlatform = (platform) => {
      platform = platform.toLowerCase();
      if (platform.includes("ios"))
        return "iOS";
      if (platform === "android")
        return "Android";
      if (platform === "darwin")
        return "MacOS";
      if (platform === "win32")
        return "Windows";
      if (platform === "freebsd")
        return "FreeBSD";
      if (platform === "openbsd")
        return "OpenBSD";
      if (platform === "linux")
        return "Linux";
      if (platform)
        return `Other:${platform}`;
      return "Unknown";
    };
    getPlatformHeaders = () => {
      return _platformHeaders ?? (_platformHeaders = getPlatformProperties());
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/shims.mjs
function getDefaultFetch() {
  if (typeof fetch !== "undefined") {
    return fetch;
  }
  throw new Error("`fetch` is not defined as a global; Either pass `fetch` to the client, `new Anthropic({ fetch })` or polyfill the global, `globalThis.fetch = fetch`");
}
function makeReadableStream(...args) {
  const ReadableStream = globalThis.ReadableStream;
  if (typeof ReadableStream === "undefined") {
    throw new Error("`ReadableStream` is not defined as a global; You will need to polyfill it, `globalThis.ReadableStream = ReadableStream`");
  }
  return new ReadableStream(...args);
}
function ReadableStreamFrom(iterable) {
  let iter = Symbol.asyncIterator in iterable ? iterable[Symbol.asyncIterator]() : iterable[Symbol.iterator]();
  return makeReadableStream({
    start() {
    },
    async pull(controller) {
      const { done, value } = await iter.next();
      if (done) {
        controller.close();
      } else {
        controller.enqueue(value);
      }
    },
    async cancel() {
      await iter.return?.();
    }
  });
}
function ReadableStreamToAsyncIterable(stream) {
  if (stream[Symbol.asyncIterator])
    return stream;
  const reader = stream.getReader();
  return {
    async next() {
      try {
        const result = await reader.read();
        if (result?.done)
          reader.releaseLock();
        return result;
      } catch (e) {
        reader.releaseLock();
        throw e;
      }
    },
    async return() {
      const cancelPromise = reader.cancel();
      reader.releaseLock();
      await cancelPromise;
      return { done: true, value: void 0 };
    },
    [Symbol.asyncIterator]() {
      return this;
    }
  };
}
async function CancelReadableStream(stream) {
  if (stream === null || typeof stream !== "object")
    return;
  if (stream[Symbol.asyncIterator]) {
    await stream[Symbol.asyncIterator]().return?.();
    return;
  }
  const reader = stream.getReader();
  const cancelPromise = reader.cancel();
  reader.releaseLock();
  await cancelPromise;
}
var init_shims = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/shims.mjs"() {
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/request-options.mjs
var FallbackEncoder;
var init_request_options = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/request-options.mjs"() {
    FallbackEncoder = ({ headers, body }) => {
      return {
        bodyHeaders: {
          "content-type": "application/json"
        },
        body: JSON.stringify(body)
      };
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/utils/query.mjs
function stringifyQuery(query) {
  return Object.entries(query).filter(([_, value]) => typeof value !== "undefined").map(([key, value]) => {
    if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
      return `${encodeURIComponent(key)}=${encodeURIComponent(value)}`;
    }
    if (value === null) {
      return `${encodeURIComponent(key)}=`;
    }
    throw new AnthropicError(`Cannot stringify type ${typeof value}; Expected string, number, boolean, or null. If you need to pass nested query parameters, you can manually encode them, e.g. { query: { 'foo[key1]': value1, 'foo[key2]': value2 } }, and please open a GitHub issue requesting better support for your use case.`);
  }).join("&");
}
var init_query = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/utils/query.mjs"() {
    init_error();
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/utils/bytes.mjs
function concatBytes(buffers) {
  let length = 0;
  for (const buffer of buffers) {
    length += buffer.length;
  }
  const output = new Uint8Array(length);
  let index = 0;
  for (const buffer of buffers) {
    output.set(buffer, index);
    index += buffer.length;
  }
  return output;
}
function encodeUTF8(str2) {
  let encoder;
  return (encodeUTF8_ ?? (encoder = new globalThis.TextEncoder(), encodeUTF8_ = encoder.encode.bind(encoder)))(str2);
}
function decodeUTF8(bytes) {
  let decoder;
  return (decodeUTF8_ ?? (decoder = new globalThis.TextDecoder(), decodeUTF8_ = decoder.decode.bind(decoder)))(bytes);
}
var encodeUTF8_, decodeUTF8_;
var init_bytes = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/utils/bytes.mjs"() {
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/decoders/line.mjs
function findNewlineIndex(buffer, startIndex) {
  const newline = 10;
  const carriage = 13;
  for (let i = startIndex ?? 0; i < buffer.length; i++) {
    if (buffer[i] === newline) {
      return { preceding: i, index: i + 1, carriage: false };
    }
    if (buffer[i] === carriage) {
      return { preceding: i, index: i + 1, carriage: true };
    }
  }
  return null;
}
function findDoubleNewlineIndex(buffer) {
  const newline = 10;
  const carriage = 13;
  for (let i = 0; i < buffer.length - 1; i++) {
    if (buffer[i] === newline && buffer[i + 1] === newline) {
      return i + 2;
    }
    if (buffer[i] === carriage && buffer[i + 1] === carriage) {
      return i + 2;
    }
    if (buffer[i] === carriage && buffer[i + 1] === newline && i + 3 < buffer.length && buffer[i + 2] === carriage && buffer[i + 3] === newline) {
      return i + 4;
    }
  }
  return -1;
}
var _LineDecoder_buffer, _LineDecoder_carriageReturnIndex, LineDecoder;
var init_line = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/decoders/line.mjs"() {
    init_tslib();
    init_bytes();
    LineDecoder = class {
      constructor() {
        _LineDecoder_buffer.set(this, void 0);
        _LineDecoder_carriageReturnIndex.set(this, void 0);
        __classPrivateFieldSet(this, _LineDecoder_buffer, new Uint8Array(), "f");
        __classPrivateFieldSet(this, _LineDecoder_carriageReturnIndex, null, "f");
      }
      decode(chunk) {
        if (chunk == null) {
          return [];
        }
        const binaryChunk = chunk instanceof ArrayBuffer ? new Uint8Array(chunk) : typeof chunk === "string" ? encodeUTF8(chunk) : chunk;
        __classPrivateFieldSet(this, _LineDecoder_buffer, concatBytes([__classPrivateFieldGet(this, _LineDecoder_buffer, "f"), binaryChunk]), "f");
        const lines = [];
        let patternIndex;
        while ((patternIndex = findNewlineIndex(__classPrivateFieldGet(this, _LineDecoder_buffer, "f"), __classPrivateFieldGet(this, _LineDecoder_carriageReturnIndex, "f"))) != null) {
          if (patternIndex.carriage && __classPrivateFieldGet(this, _LineDecoder_carriageReturnIndex, "f") == null) {
            __classPrivateFieldSet(this, _LineDecoder_carriageReturnIndex, patternIndex.index, "f");
            continue;
          }
          if (__classPrivateFieldGet(this, _LineDecoder_carriageReturnIndex, "f") != null && (patternIndex.index !== __classPrivateFieldGet(this, _LineDecoder_carriageReturnIndex, "f") + 1 || patternIndex.carriage)) {
            lines.push(decodeUTF8(__classPrivateFieldGet(this, _LineDecoder_buffer, "f").subarray(0, __classPrivateFieldGet(this, _LineDecoder_carriageReturnIndex, "f") - 1)));
            __classPrivateFieldSet(this, _LineDecoder_buffer, __classPrivateFieldGet(this, _LineDecoder_buffer, "f").subarray(__classPrivateFieldGet(this, _LineDecoder_carriageReturnIndex, "f")), "f");
            __classPrivateFieldSet(this, _LineDecoder_carriageReturnIndex, null, "f");
            continue;
          }
          const endIndex = __classPrivateFieldGet(this, _LineDecoder_carriageReturnIndex, "f") !== null ? patternIndex.preceding - 1 : patternIndex.preceding;
          const line = decodeUTF8(__classPrivateFieldGet(this, _LineDecoder_buffer, "f").subarray(0, endIndex));
          lines.push(line);
          __classPrivateFieldSet(this, _LineDecoder_buffer, __classPrivateFieldGet(this, _LineDecoder_buffer, "f").subarray(patternIndex.index), "f");
          __classPrivateFieldSet(this, _LineDecoder_carriageReturnIndex, null, "f");
        }
        return lines;
      }
      flush() {
        if (!__classPrivateFieldGet(this, _LineDecoder_buffer, "f").length) {
          return [];
        }
        return this.decode("\n");
      }
    };
    _LineDecoder_buffer = /* @__PURE__ */ new WeakMap(), _LineDecoder_carriageReturnIndex = /* @__PURE__ */ new WeakMap();
    LineDecoder.NEWLINE_CHARS = /* @__PURE__ */ new Set(["\n", "\r"]);
    LineDecoder.NEWLINE_REGEXP = /\r\n|[\n\r]/g;
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/utils/log.mjs
function noop() {
}
function makeLogFn(fnLevel, logger, logLevel) {
  if (!logger || levelNumbers[fnLevel] > levelNumbers[logLevel]) {
    return noop;
  } else {
    return logger[fnLevel].bind(logger);
  }
}
function loggerFor(client) {
  const logger = client.logger;
  const logLevel = client.logLevel ?? "off";
  if (!logger) {
    return noopLogger;
  }
  const cachedLogger = cachedLoggers.get(logger);
  if (cachedLogger && cachedLogger[0] === logLevel) {
    return cachedLogger[1];
  }
  const levelLogger = {
    error: makeLogFn("error", logger, logLevel),
    warn: makeLogFn("warn", logger, logLevel),
    info: makeLogFn("info", logger, logLevel),
    debug: makeLogFn("debug", logger, logLevel)
  };
  cachedLoggers.set(logger, [logLevel, levelLogger]);
  return levelLogger;
}
var levelNumbers, parseLogLevel, noopLogger, cachedLoggers, formatRequestDetails;
var init_log = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/utils/log.mjs"() {
    init_values();
    levelNumbers = {
      off: 0,
      error: 200,
      warn: 300,
      info: 400,
      debug: 500
    };
    parseLogLevel = (maybeLevel, sourceName, client) => {
      if (!maybeLevel) {
        return void 0;
      }
      if (hasOwn(levelNumbers, maybeLevel)) {
        return maybeLevel;
      }
      loggerFor(client).warn(`${sourceName} was set to ${JSON.stringify(maybeLevel)}, expected one of ${JSON.stringify(Object.keys(levelNumbers))}`);
      return void 0;
    };
    noopLogger = {
      error: noop,
      warn: noop,
      info: noop,
      debug: noop
    };
    cachedLoggers = /* @__PURE__ */ new WeakMap();
    formatRequestDetails = (details) => {
      if (details.options) {
        details.options = { ...details.options };
        delete details.options["headers"];
      }
      if (details.headers) {
        details.headers = Object.fromEntries((details.headers instanceof Headers ? [...details.headers] : Object.entries(details.headers)).map(([name, value]) => [
          name,
          name.toLowerCase() === "x-api-key" || name.toLowerCase() === "authorization" || name.toLowerCase() === "cookie" || name.toLowerCase() === "set-cookie" ? "***" : value
        ]));
      }
      if ("retryOfRequestLogID" in details) {
        if (details.retryOfRequestLogID) {
          details.retryOf = details.retryOfRequestLogID;
        }
        delete details.retryOfRequestLogID;
      }
      return details;
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/core/streaming.mjs
async function* _iterSSEMessages(response, controller) {
  if (!response.body) {
    controller.abort();
    if (typeof globalThis.navigator !== "undefined" && globalThis.navigator.product === "ReactNative") {
      throw new AnthropicError(`The default react-native fetch implementation does not support streaming. Please use expo/fetch: https://docs.expo.dev/versions/latest/sdk/expo/#expofetch-api`);
    }
    throw new AnthropicError(`Attempted to iterate over a response with no body`);
  }
  const sseDecoder = new SSEDecoder();
  const lineDecoder = new LineDecoder();
  const iter = ReadableStreamToAsyncIterable(response.body);
  for await (const sseChunk of iterSSEChunks(iter)) {
    for (const line of lineDecoder.decode(sseChunk)) {
      const sse = sseDecoder.decode(line);
      if (sse)
        yield sse;
    }
  }
  for (const line of lineDecoder.flush()) {
    const sse = sseDecoder.decode(line);
    if (sse)
      yield sse;
  }
}
async function* iterSSEChunks(iterator) {
  let data = new Uint8Array();
  for await (const chunk of iterator) {
    if (chunk == null) {
      continue;
    }
    const binaryChunk = chunk instanceof ArrayBuffer ? new Uint8Array(chunk) : typeof chunk === "string" ? encodeUTF8(chunk) : chunk;
    let newData = new Uint8Array(data.length + binaryChunk.length);
    newData.set(data);
    newData.set(binaryChunk, data.length);
    data = newData;
    let patternIndex;
    while ((patternIndex = findDoubleNewlineIndex(data)) !== -1) {
      yield data.slice(0, patternIndex);
      data = data.slice(patternIndex);
    }
  }
  if (data.length > 0) {
    yield data;
  }
}
function partition(str2, delimiter2) {
  const index = str2.indexOf(delimiter2);
  if (index !== -1) {
    return [str2.substring(0, index), delimiter2, str2.substring(index + delimiter2.length)];
  }
  return [str2, "", ""];
}
var _Stream_client, Stream, SSEDecoder;
var init_streaming = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/core/streaming.mjs"() {
    init_tslib();
    init_error();
    init_shims();
    init_line();
    init_shims();
    init_errors();
    init_values();
    init_bytes();
    init_log();
    init_error();
    Stream = class _Stream {
      constructor(iterator, controller, client) {
        this.iterator = iterator;
        _Stream_client.set(this, void 0);
        this.controller = controller;
        __classPrivateFieldSet(this, _Stream_client, client, "f");
      }
      static fromSSEResponse(response, controller, client) {
        let consumed = false;
        const logger = client ? loggerFor(client) : console;
        async function* iterator() {
          if (consumed) {
            throw new AnthropicError("Cannot iterate over a consumed stream, use `.tee()` to split the stream.");
          }
          consumed = true;
          let done = false;
          try {
            for await (const sse of _iterSSEMessages(response, controller)) {
              if (sse.event === "completion") {
                try {
                  yield JSON.parse(sse.data);
                } catch (e) {
                  logger.error(`Could not parse message into JSON:`, sse.data);
                  logger.error(`From chunk:`, sse.raw);
                  throw e;
                }
              }
              if (sse.event === "message_start" || sse.event === "message_delta" || sse.event === "message_stop" || sse.event === "content_block_start" || sse.event === "content_block_delta" || sse.event === "content_block_stop" || sse.event === "message" || sse.event === "user.message" || sse.event === "user.interrupt" || sse.event === "user.tool_confirmation" || sse.event === "user.custom_tool_result" || sse.event === "agent.message" || sse.event === "agent.thinking" || sse.event === "agent.tool_use" || sse.event === "agent.tool_result" || sse.event === "agent.mcp_tool_use" || sse.event === "agent.mcp_tool_result" || sse.event === "agent.custom_tool_use" || sse.event === "agent.thread_context_compacted" || sse.event === "session.status_running" || sse.event === "session.status_idle" || sse.event === "session.status_rescheduled" || sse.event === "session.status_terminated" || sse.event === "session.error" || sse.event === "session.deleted" || sse.event === "span.model_request_start" || sse.event === "span.model_request_end") {
                try {
                  yield JSON.parse(sse.data);
                } catch (e) {
                  logger.error(`Could not parse message into JSON:`, sse.data);
                  logger.error(`From chunk:`, sse.raw);
                  throw e;
                }
              }
              if (sse.event === "ping") {
                continue;
              }
              if (sse.event === "error") {
                const body = safeJSON(sse.data) ?? sse.data;
                const type = body?.error?.type;
                throw new APIError(void 0, body, void 0, response.headers, type);
              }
            }
            done = true;
          } catch (e) {
            if (isAbortError(e))
              return;
            throw e;
          } finally {
            if (!done)
              controller.abort();
          }
        }
        return new _Stream(iterator, controller, client);
      }
      /**
       * Generates a Stream from a newline-separated ReadableStream
       * where each item is a JSON value.
       */
      static fromReadableStream(readableStream, controller, client) {
        let consumed = false;
        async function* iterLines() {
          const lineDecoder = new LineDecoder();
          const iter = ReadableStreamToAsyncIterable(readableStream);
          for await (const chunk of iter) {
            for (const line of lineDecoder.decode(chunk)) {
              yield line;
            }
          }
          for (const line of lineDecoder.flush()) {
            yield line;
          }
        }
        async function* iterator() {
          if (consumed) {
            throw new AnthropicError("Cannot iterate over a consumed stream, use `.tee()` to split the stream.");
          }
          consumed = true;
          let done = false;
          try {
            for await (const line of iterLines()) {
              if (done)
                continue;
              if (line)
                yield JSON.parse(line);
            }
            done = true;
          } catch (e) {
            if (isAbortError(e))
              return;
            throw e;
          } finally {
            if (!done)
              controller.abort();
          }
        }
        return new _Stream(iterator, controller, client);
      }
      [(_Stream_client = /* @__PURE__ */ new WeakMap(), Symbol.asyncIterator)]() {
        return this.iterator();
      }
      /**
       * Splits the stream into two streams which can be
       * independently read from at different speeds.
       */
      tee() {
        const left = [];
        const right = [];
        const iterator = this.iterator();
        const teeIterator = (queue) => {
          return {
            next: () => {
              if (queue.length === 0) {
                const result = iterator.next();
                left.push(result);
                right.push(result);
              }
              return queue.shift();
            }
          };
        };
        return [
          new _Stream(() => teeIterator(left), this.controller, __classPrivateFieldGet(this, _Stream_client, "f")),
          new _Stream(() => teeIterator(right), this.controller, __classPrivateFieldGet(this, _Stream_client, "f"))
        ];
      }
      /**
       * Converts this stream to a newline-separated ReadableStream of
       * JSON stringified values in the stream
       * which can be turned back into a Stream with `Stream.fromReadableStream()`.
       */
      toReadableStream() {
        const self = this;
        let iter;
        return makeReadableStream({
          async start() {
            iter = self[Symbol.asyncIterator]();
          },
          async pull(ctrl) {
            try {
              const { value, done } = await iter.next();
              if (done)
                return ctrl.close();
              const bytes = encodeUTF8(JSON.stringify(value) + "\n");
              ctrl.enqueue(bytes);
            } catch (err) {
              ctrl.error(err);
            }
          },
          async cancel() {
            await iter.return?.();
          }
        });
      }
    };
    SSEDecoder = class {
      constructor() {
        this.event = null;
        this.data = [];
        this.chunks = [];
      }
      decode(line) {
        if (line.endsWith("\r")) {
          line = line.substring(0, line.length - 1);
        }
        if (!line) {
          if (!this.event && !this.data.length)
            return null;
          const sse = {
            event: this.event,
            data: this.data.join("\n"),
            raw: this.chunks
          };
          this.event = null;
          this.data = [];
          this.chunks = [];
          return sse;
        }
        this.chunks.push(line);
        if (line.startsWith(":")) {
          return null;
        }
        let [fieldname, _, value] = partition(line, ":");
        if (value.startsWith(" ")) {
          value = value.substring(1);
        }
        if (fieldname === "event") {
          this.event = value;
        } else if (fieldname === "data") {
          this.data.push(value);
        }
        return null;
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/parse.mjs
async function defaultParseResponse(client, props) {
  const { response, requestLogID, retryOfRequestLogID, startTime } = props;
  const body = await (async () => {
    if (props.options.stream) {
      loggerFor(client).debug("response", response.status, response.url, response.headers, response.body);
      if (props.options.__streamClass) {
        return props.options.__streamClass.fromSSEResponse(response, props.controller);
      }
      return Stream.fromSSEResponse(response, props.controller);
    }
    if (response.status === 204) {
      return null;
    }
    if (props.options.__binaryResponse) {
      return response;
    }
    const contentType = response.headers.get("content-type");
    const mediaType = contentType?.split(";")[0]?.trim();
    const isJSON = mediaType?.includes("application/json") || mediaType?.endsWith("+json");
    if (isJSON) {
      const contentLength = response.headers.get("content-length");
      if (contentLength === "0") {
        return void 0;
      }
      const json = await response.json();
      return addRequestID(json, response);
    }
    const text = await response.text();
    return text;
  })();
  loggerFor(client).debug(`[${requestLogID}] response parsed`, formatRequestDetails({
    retryOfRequestLogID,
    url: response.url,
    status: response.status,
    body,
    durationMs: Date.now() - startTime
  }));
  return body;
}
function addRequestID(value, response) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return value;
  }
  return Object.defineProperty(value, "_request_id", {
    value: response.headers.get("request-id"),
    enumerable: false
  });
}
var init_parse = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/parse.mjs"() {
    init_streaming();
    init_log();
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/core/api-promise.mjs
var _APIPromise_client, APIPromise;
var init_api_promise = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/core/api-promise.mjs"() {
    init_tslib();
    init_parse();
    APIPromise = class _APIPromise extends Promise {
      constructor(client, responsePromise, parseResponse = defaultParseResponse) {
        super((resolve4) => {
          resolve4(null);
        });
        this.responsePromise = responsePromise;
        this.parseResponse = parseResponse;
        _APIPromise_client.set(this, void 0);
        __classPrivateFieldSet(this, _APIPromise_client, client, "f");
      }
      _thenUnwrap(transform) {
        return new _APIPromise(__classPrivateFieldGet(this, _APIPromise_client, "f"), this.responsePromise, async (client, props) => addRequestID(transform(await this.parseResponse(client, props), props), props.response));
      }
      /**
       * Gets the raw `Response` instance instead of parsing the response
       * data.
       *
       * If you want to parse the response body but still get the `Response`
       * instance, you can use {@link withResponse()}.
       *
       * 👋 Getting the wrong TypeScript type for `Response`?
       * Try setting `"moduleResolution": "NodeNext"` or add `"lib": ["DOM"]`
       * to your `tsconfig.json`.
       */
      asResponse() {
        return this.responsePromise.then((p) => p.response);
      }
      /**
       * Gets the parsed response data, the raw `Response` instance and the ID of the request,
       * returned via the `request-id` header which is useful for debugging requests and resporting
       * issues to Anthropic.
       *
       * If you just want to get the raw `Response` instance without parsing it,
       * you can use {@link asResponse()}.
       *
       * 👋 Getting the wrong TypeScript type for `Response`?
       * Try setting `"moduleResolution": "NodeNext"` or add `"lib": ["DOM"]`
       * to your `tsconfig.json`.
       */
      async withResponse() {
        const [data, response] = await Promise.all([this.parse(), this.asResponse()]);
        return { data, response, request_id: response.headers.get("request-id") };
      }
      parse() {
        if (!this.parsedPromise) {
          this.parsedPromise = this.responsePromise.then((data) => this.parseResponse(__classPrivateFieldGet(this, _APIPromise_client, "f"), data));
        }
        return this.parsedPromise;
      }
      then(onfulfilled, onrejected) {
        return this.parse().then(onfulfilled, onrejected);
      }
      catch(onrejected) {
        return this.parse().catch(onrejected);
      }
      finally(onfinally) {
        return this.parse().finally(onfinally);
      }
    };
    _APIPromise_client = /* @__PURE__ */ new WeakMap();
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/core/pagination.mjs
var _AbstractPage_client, AbstractPage, PagePromise, Page, PageCursor;
var init_pagination = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/core/pagination.mjs"() {
    init_tslib();
    init_error();
    init_parse();
    init_api_promise();
    init_values();
    AbstractPage = class {
      constructor(client, response, body, options) {
        _AbstractPage_client.set(this, void 0);
        __classPrivateFieldSet(this, _AbstractPage_client, client, "f");
        this.options = options;
        this.response = response;
        this.body = body;
      }
      hasNextPage() {
        const items = this.getPaginatedItems();
        if (!items.length)
          return false;
        return this.nextPageRequestOptions() != null;
      }
      async getNextPage() {
        const nextOptions = this.nextPageRequestOptions();
        if (!nextOptions) {
          throw new AnthropicError("No next page expected; please check `.hasNextPage()` before calling `.getNextPage()`.");
        }
        return await __classPrivateFieldGet(this, _AbstractPage_client, "f").requestAPIList(this.constructor, nextOptions);
      }
      async *iterPages() {
        let page = this;
        yield page;
        while (page.hasNextPage()) {
          page = await page.getNextPage();
          yield page;
        }
      }
      async *[(_AbstractPage_client = /* @__PURE__ */ new WeakMap(), Symbol.asyncIterator)]() {
        for await (const page of this.iterPages()) {
          for (const item of page.getPaginatedItems()) {
            yield item;
          }
        }
      }
    };
    PagePromise = class extends APIPromise {
      constructor(client, request, Page2) {
        super(client, request, async (client2, props) => new Page2(client2, props.response, await defaultParseResponse(client2, props), props.options));
      }
      /**
       * Allow auto-paginating iteration on an unawaited list call, eg:
       *
       *    for await (const item of client.items.list()) {
       *      console.log(item)
       *    }
       */
      async *[Symbol.asyncIterator]() {
        const page = await this;
        for await (const item of page) {
          yield item;
        }
      }
    };
    Page = class extends AbstractPage {
      constructor(client, response, body, options) {
        super(client, response, body, options);
        this.data = body.data || [];
        this.has_more = body.has_more || false;
        this.first_id = body.first_id || null;
        this.last_id = body.last_id || null;
      }
      getPaginatedItems() {
        return this.data ?? [];
      }
      hasNextPage() {
        if (this.has_more === false) {
          return false;
        }
        return super.hasNextPage();
      }
      nextPageRequestOptions() {
        if (this.options.query?.["before_id"]) {
          const first_id = this.first_id;
          if (!first_id) {
            return null;
          }
          return {
            ...this.options,
            query: {
              ...maybeObj(this.options.query),
              before_id: first_id
            }
          };
        }
        const cursor = this.last_id;
        if (!cursor) {
          return null;
        }
        return {
          ...this.options,
          query: {
            ...maybeObj(this.options.query),
            after_id: cursor
          }
        };
      }
    };
    PageCursor = class extends AbstractPage {
      constructor(client, response, body, options) {
        super(client, response, body, options);
        this.data = body.data || [];
        this.next_page = body.next_page || null;
      }
      getPaginatedItems() {
        return this.data ?? [];
      }
      nextPageRequestOptions() {
        const cursor = this.next_page;
        if (!cursor) {
          return null;
        }
        return {
          ...this.options,
          query: {
            ...maybeObj(this.options.query),
            page: cursor
          }
        };
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/uploads.mjs
function makeFile(fileBits, fileName, options) {
  checkFileSupport();
  return new File(fileBits, fileName ?? "unknown_file", options);
}
function getName(value, stripPath) {
  const val = typeof value === "object" && value !== null && ("name" in value && value.name && String(value.name) || "url" in value && value.url && String(value.url) || "filename" in value && value.filename && String(value.filename) || "path" in value && value.path && String(value.path)) || "";
  return stripPath ? val.split(/[\\/]/).pop() || void 0 : val;
}
function supportsFormData(fetchObject) {
  const fetch2 = typeof fetchObject === "function" ? fetchObject : fetchObject.fetch;
  const cached = supportsFormDataMap.get(fetch2);
  if (cached)
    return cached;
  const promise = (async () => {
    try {
      const FetchResponse = "Response" in fetch2 ? fetch2.Response : (await fetch2("data:,")).constructor;
      const data = new FormData();
      if (data.toString() === await new FetchResponse(data).text()) {
        return false;
      }
      return true;
    } catch {
      return true;
    }
  })();
  supportsFormDataMap.set(fetch2, promise);
  return promise;
}
var checkFileSupport, isAsyncIterable, multipartFormRequestOptions, supportsFormDataMap, createForm, isNamedBlob, addFormValue;
var init_uploads = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/uploads.mjs"() {
    init_shims();
    checkFileSupport = () => {
      if (typeof File === "undefined") {
        const { process: process2 } = globalThis;
        const isOldNode = typeof process2?.versions?.node === "string" && parseInt(process2.versions.node.split(".")) < 20;
        throw new Error("`File` is not defined as a global, which is required for file uploads." + (isOldNode ? " Update to Node 20 LTS or newer, or set `globalThis.File` to `import('node:buffer').File`." : ""));
      }
    };
    isAsyncIterable = (value) => value != null && typeof value === "object" && typeof value[Symbol.asyncIterator] === "function";
    multipartFormRequestOptions = async (opts, fetch2, stripFilenames = true) => {
      return { ...opts, body: await createForm(opts.body, fetch2, stripFilenames) };
    };
    supportsFormDataMap = /* @__PURE__ */ new WeakMap();
    createForm = async (body, fetch2, stripFilenames = true) => {
      if (!await supportsFormData(fetch2)) {
        throw new TypeError("The provided fetch function does not support file uploads with the current global FormData class.");
      }
      const form = new FormData();
      await Promise.all(Object.entries(body || {}).map(([key, value]) => addFormValue(form, key, value, stripFilenames)));
      return form;
    };
    isNamedBlob = (value) => value instanceof Blob && "name" in value;
    addFormValue = async (form, key, value, stripFilenames) => {
      if (value === void 0)
        return;
      if (value == null) {
        throw new TypeError(`Received null for "${key}"; to pass null in FormData, you must use the string 'null'`);
      }
      if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
        form.append(key, String(value));
      } else if (value instanceof Response) {
        let options = {};
        const contentType = value.headers.get("Content-Type");
        if (contentType) {
          options = { type: contentType };
        }
        form.append(key, makeFile([await value.blob()], getName(value, stripFilenames), options));
      } else if (isAsyncIterable(value)) {
        form.append(key, makeFile([await new Response(ReadableStreamFrom(value)).blob()], getName(value, stripFilenames)));
      } else if (isNamedBlob(value)) {
        form.append(key, makeFile([value], getName(value, stripFilenames), { type: value.type }));
      } else if (Array.isArray(value)) {
        await Promise.all(value.map((entry) => addFormValue(form, key + "[]", entry, stripFilenames)));
      } else if (typeof value === "object") {
        await Promise.all(Object.entries(value).map(([name, prop]) => addFormValue(form, `${key}[${name}]`, prop, stripFilenames)));
      } else {
        throw new TypeError(`Invalid value given to form, expected a string, number, boolean, object, Array, File or Blob but got ${value} instead`);
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/to-file.mjs
async function toFile(value, name, options) {
  checkFileSupport();
  value = await value;
  name || (name = getName(value, true));
  if (isFileLike(value)) {
    if (value instanceof File && name == null && options == null) {
      return value;
    }
    return makeFile([await value.arrayBuffer()], name ?? value.name, {
      type: value.type,
      lastModified: value.lastModified,
      ...options
    });
  }
  if (isResponseLike(value)) {
    const blob = await value.blob();
    name || (name = new URL(value.url).pathname.split(/[\\/]/).pop());
    return makeFile(await getBytes(blob), name, options);
  }
  const parts = await getBytes(value);
  if (!options?.type) {
    const type = parts.find((part) => typeof part === "object" && "type" in part && part.type);
    if (typeof type === "string") {
      options = { ...options, type };
    }
  }
  return makeFile(parts, name, options);
}
async function getBytes(value) {
  let parts = [];
  if (typeof value === "string" || ArrayBuffer.isView(value) || // includes Uint8Array, Buffer, etc.
  value instanceof ArrayBuffer) {
    parts.push(value);
  } else if (isBlobLike(value)) {
    parts.push(value instanceof Blob ? value : await value.arrayBuffer());
  } else if (isAsyncIterable(value)) {
    for await (const chunk of value) {
      parts.push(...await getBytes(chunk));
    }
  } else {
    const constructor = value?.constructor?.name;
    throw new Error(`Unexpected data type: ${typeof value}${constructor ? `; constructor: ${constructor}` : ""}${propsForError(value)}`);
  }
  return parts;
}
function propsForError(value) {
  if (typeof value !== "object" || value === null)
    return "";
  const props = Object.getOwnPropertyNames(value);
  return `; props: [${props.map((p) => `"${p}"`).join(", ")}]`;
}
var isBlobLike, isFileLike, isResponseLike;
var init_to_file = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/to-file.mjs"() {
    init_uploads();
    init_uploads();
    isBlobLike = (value) => value != null && typeof value === "object" && typeof value.size === "number" && typeof value.type === "string" && typeof value.text === "function" && typeof value.slice === "function" && typeof value.arrayBuffer === "function";
    isFileLike = (value) => value != null && typeof value === "object" && typeof value.name === "string" && typeof value.lastModified === "number" && isBlobLike(value);
    isResponseLike = (value) => value != null && typeof value === "object" && typeof value.url === "string" && typeof value.blob === "function";
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/core/uploads.mjs
var init_uploads2 = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/core/uploads.mjs"() {
    init_to_file();
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/shared.mjs
var init_shared = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/shared.mjs"() {
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/core/resource.mjs
var APIResource;
var init_resource = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/core/resource.mjs"() {
    APIResource = class {
      constructor(client) {
        this._client = client;
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/headers.mjs
function* iterateHeaders(headers) {
  if (!headers)
    return;
  if (brand_privateNullableHeaders in headers) {
    const { values, nulls } = headers;
    yield* values.entries();
    for (const name of nulls) {
      yield [name, null];
    }
    return;
  }
  let shouldClear = false;
  let iter;
  if (headers instanceof Headers) {
    iter = headers.entries();
  } else if (isReadonlyArray(headers)) {
    iter = headers;
  } else {
    shouldClear = true;
    iter = Object.entries(headers ?? {});
  }
  for (let row of iter) {
    const name = row[0];
    if (typeof name !== "string")
      throw new TypeError("expected header name to be a string");
    const values = isReadonlyArray(row[1]) ? row[1] : [row[1]];
    let didClear = false;
    for (const value of values) {
      if (value === void 0)
        continue;
      if (shouldClear && !didClear) {
        didClear = true;
        yield [name, null];
      }
      yield [name, value];
    }
  }
}
var brand_privateNullableHeaders, buildHeaders;
var init_headers = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/headers.mjs"() {
    init_values();
    brand_privateNullableHeaders = /* @__PURE__ */ Symbol.for("brand.privateNullableHeaders");
    buildHeaders = (newHeaders) => {
      const targetHeaders = new Headers();
      const nullHeaders = /* @__PURE__ */ new Set();
      for (const headers of newHeaders) {
        const seenHeaders = /* @__PURE__ */ new Set();
        for (const [name, value] of iterateHeaders(headers)) {
          const lowerName = name.toLowerCase();
          if (!seenHeaders.has(lowerName)) {
            targetHeaders.delete(name);
            seenHeaders.add(lowerName);
          }
          if (value === null) {
            targetHeaders.delete(name);
            nullHeaders.add(lowerName);
          } else {
            targetHeaders.append(name, value);
            nullHeaders.delete(lowerName);
          }
        }
      }
      return { [brand_privateNullableHeaders]: true, values: targetHeaders, nulls: nullHeaders };
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/utils/path.mjs
function encodeURIPath(str2) {
  return str2.replace(/[^A-Za-z0-9\-._~!$&'()*+,;=:@]+/g, encodeURIComponent);
}
var EMPTY, createPathTagFunction, path3;
var init_path = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/utils/path.mjs"() {
    init_error();
    EMPTY = /* @__PURE__ */ Object.freeze(/* @__PURE__ */ Object.create(null));
    createPathTagFunction = (pathEncoder = encodeURIPath) => function path7(statics, ...params) {
      if (statics.length === 1)
        return statics[0];
      let postPath = false;
      const invalidSegments = [];
      const path8 = statics.reduce((previousValue, currentValue, index) => {
        if (/[?#]/.test(currentValue)) {
          postPath = true;
        }
        const value = params[index];
        let encoded = (postPath ? encodeURIComponent : pathEncoder)("" + value);
        if (index !== params.length && (value == null || typeof value === "object" && // handle values from other realms
        value.toString === Object.getPrototypeOf(Object.getPrototypeOf(value.hasOwnProperty ?? EMPTY) ?? EMPTY)?.toString)) {
          encoded = value + "";
          invalidSegments.push({
            start: previousValue.length + currentValue.length,
            length: encoded.length,
            error: `Value of type ${Object.prototype.toString.call(value).slice(8, -1)} is not a valid path parameter`
          });
        }
        return previousValue + currentValue + (index === params.length ? "" : encoded);
      }, "");
      const pathOnly = path8.split(/[?#]/, 1)[0];
      const invalidSegmentPattern = /(?<=^|\/)(?:\.|%2e){1,2}(?=\/|$)/gi;
      let match;
      while ((match = invalidSegmentPattern.exec(pathOnly)) !== null) {
        invalidSegments.push({
          start: match.index,
          length: match[0].length,
          error: `Value "${match[0]}" can't be safely passed as a path parameter`
        });
      }
      invalidSegments.sort((a, b) => a.start - b.start);
      if (invalidSegments.length > 0) {
        let lastEnd = 0;
        const underline = invalidSegments.reduce((acc, segment) => {
          const spaces = " ".repeat(segment.start - lastEnd);
          const arrows = "^".repeat(segment.length);
          lastEnd = segment.start + segment.length;
          return acc + spaces + arrows;
        }, "");
        throw new AnthropicError(`Path parameters result in path with invalid segments:
${invalidSegments.map((e) => e.error).join("\n")}
${path8}
${underline}`);
      }
      return path8;
    };
    path3 = /* @__PURE__ */ createPathTagFunction(encodeURIPath);
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/environments.mjs
var Environments;
var init_environments = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/environments.mjs"() {
    init_resource();
    init_pagination();
    init_headers();
    init_path();
    Environments = class extends APIResource {
      /**
       * Create a new environment with the specified configuration.
       *
       * @example
       * ```ts
       * const betaEnvironment =
       *   await client.beta.environments.create({
       *     name: 'python-data-analysis',
       *   });
       * ```
       */
      create(params, options) {
        const { betas, ...body } = params;
        return this._client.post("/v1/environments?beta=true", {
          body,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Retrieve a specific environment by ID.
       *
       * @example
       * ```ts
       * const betaEnvironment =
       *   await client.beta.environments.retrieve(
       *     'env_011CZkZ9X2dpNyB7HsEFoRfW',
       *   );
       * ```
       */
      retrieve(environmentID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.get(path3`/v1/environments/${environmentID}?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Update an existing environment's configuration.
       *
       * @example
       * ```ts
       * const betaEnvironment =
       *   await client.beta.environments.update(
       *     'env_011CZkZ9X2dpNyB7HsEFoRfW',
       *   );
       * ```
       */
      update(environmentID, params, options) {
        const { betas, ...body } = params;
        return this._client.post(path3`/v1/environments/${environmentID}?beta=true`, {
          body,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * List environments with pagination support.
       *
       * @example
       * ```ts
       * // Automatically fetches more pages as needed.
       * for await (const betaEnvironment of client.beta.environments.list()) {
       *   // ...
       * }
       * ```
       */
      list(params = {}, options) {
        const { betas, ...query } = params ?? {};
        return this._client.getAPIList("/v1/environments?beta=true", PageCursor, {
          query,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Delete an environment by ID. Returns a confirmation of the deletion.
       *
       * @example
       * ```ts
       * const betaEnvironmentDeleteResponse =
       *   await client.beta.environments.delete(
       *     'env_011CZkZ9X2dpNyB7HsEFoRfW',
       *   );
       * ```
       */
      delete(environmentID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.delete(path3`/v1/environments/${environmentID}?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Archive an environment by ID. Archived environments cannot be used to create new
       * sessions.
       *
       * @example
       * ```ts
       * const betaEnvironment =
       *   await client.beta.environments.archive(
       *     'env_011CZkZ9X2dpNyB7HsEFoRfW',
       *   );
       * ```
       */
      archive(environmentID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.post(path3`/v1/environments/${environmentID}/archive?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/lib/stainless-helper-header.mjs
function wasCreatedByStainlessHelper(value) {
  return typeof value === "object" && value !== null && SDK_HELPER_SYMBOL in value;
}
function collectStainlessHelpers(tools, messages) {
  const helpers = /* @__PURE__ */ new Set();
  if (tools) {
    for (const tool of tools) {
      if (wasCreatedByStainlessHelper(tool)) {
        helpers.add(tool[SDK_HELPER_SYMBOL]);
      }
    }
  }
  if (messages) {
    for (const message of messages) {
      if (wasCreatedByStainlessHelper(message)) {
        helpers.add(message[SDK_HELPER_SYMBOL]);
      }
      if (Array.isArray(message.content)) {
        for (const block of message.content) {
          if (wasCreatedByStainlessHelper(block)) {
            helpers.add(block[SDK_HELPER_SYMBOL]);
          }
        }
      }
    }
  }
  return Array.from(helpers);
}
function stainlessHelperHeader(tools, messages) {
  const helpers = collectStainlessHelpers(tools, messages);
  if (helpers.length === 0)
    return {};
  return { "x-stainless-helper": helpers.join(", ") };
}
function stainlessHelperHeaderFromFile(file) {
  if (wasCreatedByStainlessHelper(file)) {
    return { "x-stainless-helper": file[SDK_HELPER_SYMBOL] };
  }
  return {};
}
var SDK_HELPER_SYMBOL;
var init_stainless_helper_header = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/lib/stainless-helper-header.mjs"() {
    SDK_HELPER_SYMBOL = /* @__PURE__ */ Symbol("anthropic.sdk.stainlessHelper");
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/files.mjs
var Files;
var init_files = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/files.mjs"() {
    init_resource();
    init_pagination();
    init_headers();
    init_stainless_helper_header();
    init_uploads();
    init_path();
    Files = class extends APIResource {
      /**
       * List Files
       *
       * @example
       * ```ts
       * // Automatically fetches more pages as needed.
       * for await (const fileMetadata of client.beta.files.list()) {
       *   // ...
       * }
       * ```
       */
      list(params = {}, options) {
        const { betas, ...query } = params ?? {};
        return this._client.getAPIList("/v1/files", Page, {
          query,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "files-api-2025-04-14"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Delete File
       *
       * @example
       * ```ts
       * const deletedFile = await client.beta.files.delete(
       *   'file_id',
       * );
       * ```
       */
      delete(fileID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.delete(path3`/v1/files/${fileID}`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "files-api-2025-04-14"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Download File
       *
       * @example
       * ```ts
       * const response = await client.beta.files.download(
       *   'file_id',
       * );
       *
       * const content = await response.blob();
       * console.log(content);
       * ```
       */
      download(fileID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.get(path3`/v1/files/${fileID}/content`, {
          ...options,
          headers: buildHeaders([
            {
              "anthropic-beta": [...betas ?? [], "files-api-2025-04-14"].toString(),
              Accept: "application/binary"
            },
            options?.headers
          ]),
          __binaryResponse: true
        });
      }
      /**
       * Get File Metadata
       *
       * @example
       * ```ts
       * const fileMetadata =
       *   await client.beta.files.retrieveMetadata('file_id');
       * ```
       */
      retrieveMetadata(fileID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.get(path3`/v1/files/${fileID}`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "files-api-2025-04-14"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Upload File
       *
       * @example
       * ```ts
       * const fileMetadata = await client.beta.files.upload({
       *   file: fs.createReadStream('path/to/file'),
       * });
       * ```
       */
      upload(params, options) {
        const { betas, ...body } = params;
        return this._client.post("/v1/files", multipartFormRequestOptions({
          body,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "files-api-2025-04-14"].toString() },
            stainlessHelperHeaderFromFile(body.file),
            options?.headers
          ])
        }, this._client));
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/models.mjs
var Models;
var init_models = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/models.mjs"() {
    init_resource();
    init_pagination();
    init_headers();
    init_path();
    Models = class extends APIResource {
      /**
       * Get a specific model.
       *
       * The Models API response can be used to determine information about a specific
       * model or resolve a model alias to a model ID.
       *
       * @example
       * ```ts
       * const betaModelInfo = await client.beta.models.retrieve(
       *   'model_id',
       * );
       * ```
       */
      retrieve(modelID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.get(path3`/v1/models/${modelID}?beta=true`, {
          ...options,
          headers: buildHeaders([
            { ...betas?.toString() != null ? { "anthropic-beta": betas?.toString() } : void 0 },
            options?.headers
          ])
        });
      }
      /**
       * List available models.
       *
       * The Models API response can be used to determine which models are available for
       * use in the API. More recently released models are listed first.
       *
       * @example
       * ```ts
       * // Automatically fetches more pages as needed.
       * for await (const betaModelInfo of client.beta.models.list()) {
       *   // ...
       * }
       * ```
       */
      list(params = {}, options) {
        const { betas, ...query } = params ?? {};
        return this._client.getAPIList("/v1/models?beta=true", Page, {
          query,
          ...options,
          headers: buildHeaders([
            { ...betas?.toString() != null ? { "anthropic-beta": betas?.toString() } : void 0 },
            options?.headers
          ])
        });
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/user-profiles.mjs
var UserProfiles;
var init_user_profiles = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/user-profiles.mjs"() {
    init_resource();
    init_pagination();
    init_headers();
    init_path();
    UserProfiles = class extends APIResource {
      /**
       * Create User Profile
       *
       * @example
       * ```ts
       * const betaUserProfile =
       *   await client.beta.userProfiles.create();
       * ```
       */
      create(params, options) {
        const { betas, ...body } = params;
        return this._client.post("/v1/user_profiles?beta=true", {
          body,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "user-profiles-2026-03-24"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Get User Profile
       *
       * @example
       * ```ts
       * const betaUserProfile =
       *   await client.beta.userProfiles.retrieve(
       *     'uprof_011CZkZCu8hGbp5mYRQgUmz9',
       *   );
       * ```
       */
      retrieve(userProfileID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.get(path3`/v1/user_profiles/${userProfileID}?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "user-profiles-2026-03-24"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Update User Profile
       *
       * @example
       * ```ts
       * const betaUserProfile =
       *   await client.beta.userProfiles.update(
       *     'uprof_011CZkZCu8hGbp5mYRQgUmz9',
       *   );
       * ```
       */
      update(userProfileID, params, options) {
        const { betas, ...body } = params;
        return this._client.post(path3`/v1/user_profiles/${userProfileID}?beta=true`, {
          body,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "user-profiles-2026-03-24"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * List User Profiles
       *
       * @example
       * ```ts
       * // Automatically fetches more pages as needed.
       * for await (const betaUserProfile of client.beta.userProfiles.list()) {
       *   // ...
       * }
       * ```
       */
      list(params = {}, options) {
        const { betas, ...query } = params ?? {};
        return this._client.getAPIList("/v1/user_profiles?beta=true", PageCursor, {
          query,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "user-profiles-2026-03-24"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Create Enrollment URL
       *
       * @example
       * ```ts
       * const betaUserProfileEnrollmentURL =
       *   await client.beta.userProfiles.createEnrollmentURL(
       *     'uprof_011CZkZCu8hGbp5mYRQgUmz9',
       *   );
       * ```
       */
      createEnrollmentURL(userProfileID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.post(path3`/v1/user_profiles/${userProfileID}/enrollment_url?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "user-profiles-2026-03-24"].toString() },
            options?.headers
          ])
        });
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/agents/versions.mjs
var Versions;
var init_versions = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/agents/versions.mjs"() {
    init_resource();
    init_pagination();
    init_headers();
    init_path();
    Versions = class extends APIResource {
      /**
       * List Agent Versions
       *
       * @example
       * ```ts
       * // Automatically fetches more pages as needed.
       * for await (const betaManagedAgentsAgent of client.beta.agents.versions.list(
       *   'agent_011CZkYpogX7uDKUyvBTophP',
       * )) {
       *   // ...
       * }
       * ```
       */
      list(agentID, params = {}, options) {
        const { betas, ...query } = params ?? {};
        return this._client.getAPIList(path3`/v1/agents/${agentID}/versions?beta=true`, PageCursor, {
          query,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/agents/agents.mjs
var Agents;
var init_agents = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/agents/agents.mjs"() {
    init_resource();
    init_versions();
    init_versions();
    init_pagination();
    init_headers();
    init_path();
    Agents = class extends APIResource {
      constructor() {
        super(...arguments);
        this.versions = new Versions(this._client);
      }
      /**
       * Create Agent
       *
       * @example
       * ```ts
       * const betaManagedAgentsAgent =
       *   await client.beta.agents.create({
       *     model: 'claude-sonnet-4-6',
       *     name: 'My First Agent',
       *   });
       * ```
       */
      create(params, options) {
        const { betas, ...body } = params;
        return this._client.post("/v1/agents?beta=true", {
          body,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Get Agent
       *
       * @example
       * ```ts
       * const betaManagedAgentsAgent =
       *   await client.beta.agents.retrieve(
       *     'agent_011CZkYpogX7uDKUyvBTophP',
       *   );
       * ```
       */
      retrieve(agentID, params = {}, options) {
        const { betas, ...query } = params ?? {};
        return this._client.get(path3`/v1/agents/${agentID}?beta=true`, {
          query,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Update Agent
       *
       * @example
       * ```ts
       * const betaManagedAgentsAgent =
       *   await client.beta.agents.update(
       *     'agent_011CZkYpogX7uDKUyvBTophP',
       *     { version: 1 },
       *   );
       * ```
       */
      update(agentID, params, options) {
        const { betas, ...body } = params;
        return this._client.post(path3`/v1/agents/${agentID}?beta=true`, {
          body,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * List Agents
       *
       * @example
       * ```ts
       * // Automatically fetches more pages as needed.
       * for await (const betaManagedAgentsAgent of client.beta.agents.list()) {
       *   // ...
       * }
       * ```
       */
      list(params = {}, options) {
        const { betas, ...query } = params ?? {};
        return this._client.getAPIList("/v1/agents?beta=true", PageCursor, {
          query,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Archive Agent
       *
       * @example
       * ```ts
       * const betaManagedAgentsAgent =
       *   await client.beta.agents.archive(
       *     'agent_011CZkYpogX7uDKUyvBTophP',
       *   );
       * ```
       */
      archive(agentID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.post(path3`/v1/agents/${agentID}/archive?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
    };
    Agents.Versions = Versions;
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/error.mjs
var init_error2 = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/error.mjs"() {
    init_error();
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/constants.mjs
var MODEL_NONSTREAMING_TOKENS;
var init_constants = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/constants.mjs"() {
    MODEL_NONSTREAMING_TOKENS = {
      "claude-opus-4-20250514": 8192,
      "claude-opus-4-0": 8192,
      "claude-4-opus-20250514": 8192,
      "anthropic.claude-opus-4-20250514-v1:0": 8192,
      "claude-opus-4@20250514": 8192,
      "claude-opus-4-1-20250805": 8192,
      "anthropic.claude-opus-4-1-20250805-v1:0": 8192,
      "claude-opus-4-1@20250805": 8192
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/lib/beta-parser.mjs
function getOutputFormat(params) {
  return params?.output_format ?? params?.output_config?.format;
}
function maybeParseBetaMessage(message, params, opts) {
  const outputFormat = getOutputFormat(params);
  if (!params || !("parse" in (outputFormat ?? {}))) {
    return {
      ...message,
      content: message.content.map((block) => {
        if (block.type === "text") {
          const parsedBlock = Object.defineProperty({ ...block }, "parsed_output", {
            value: null,
            enumerable: false
          });
          return Object.defineProperty(parsedBlock, "parsed", {
            get() {
              opts.logger.warn("The `parsed` property on `text` blocks is deprecated, please use `parsed_output` instead.");
              return null;
            },
            enumerable: false
          });
        }
        return block;
      }),
      parsed_output: null
    };
  }
  return parseBetaMessage(message, params, opts);
}
function parseBetaMessage(message, params, opts) {
  let firstParsedOutput = null;
  const content = message.content.map((block) => {
    if (block.type === "text") {
      const parsedOutput = parseBetaOutputFormat(params, block.text);
      if (firstParsedOutput === null) {
        firstParsedOutput = parsedOutput;
      }
      const parsedBlock = Object.defineProperty({ ...block }, "parsed_output", {
        value: parsedOutput,
        enumerable: false
      });
      return Object.defineProperty(parsedBlock, "parsed", {
        get() {
          opts.logger.warn("The `parsed` property on `text` blocks is deprecated, please use `parsed_output` instead.");
          return parsedOutput;
        },
        enumerable: false
      });
    }
    return block;
  });
  return {
    ...message,
    content,
    parsed_output: firstParsedOutput
  };
}
function parseBetaOutputFormat(params, content) {
  const outputFormat = getOutputFormat(params);
  if (outputFormat?.type !== "json_schema") {
    return null;
  }
  try {
    if ("parse" in outputFormat) {
      return outputFormat.parse(content);
    }
    return JSON.parse(content);
  } catch (error) {
    throw new AnthropicError(`Failed to parse structured output: ${error}`);
  }
}
var init_beta_parser = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/lib/beta-parser.mjs"() {
    init_error();
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/_vendor/partial-json-parser/parser.mjs
var tokenize2, strip, unstrip, generate, partialParse;
var init_parser = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/_vendor/partial-json-parser/parser.mjs"() {
    tokenize2 = (input) => {
      let current = 0;
      let tokens = [];
      while (current < input.length) {
        let char = input[current];
        if (char === "\\") {
          current++;
          continue;
        }
        if (char === "{") {
          tokens.push({
            type: "brace",
            value: "{"
          });
          current++;
          continue;
        }
        if (char === "}") {
          tokens.push({
            type: "brace",
            value: "}"
          });
          current++;
          continue;
        }
        if (char === "[") {
          tokens.push({
            type: "paren",
            value: "["
          });
          current++;
          continue;
        }
        if (char === "]") {
          tokens.push({
            type: "paren",
            value: "]"
          });
          current++;
          continue;
        }
        if (char === ":") {
          tokens.push({
            type: "separator",
            value: ":"
          });
          current++;
          continue;
        }
        if (char === ",") {
          tokens.push({
            type: "delimiter",
            value: ","
          });
          current++;
          continue;
        }
        if (char === '"') {
          let value = "";
          let danglingQuote = false;
          char = input[++current];
          while (char !== '"') {
            if (current === input.length) {
              danglingQuote = true;
              break;
            }
            if (char === "\\") {
              current++;
              if (current === input.length) {
                danglingQuote = true;
                break;
              }
              value += char + input[current];
              char = input[++current];
            } else {
              value += char;
              char = input[++current];
            }
          }
          char = input[++current];
          if (!danglingQuote) {
            tokens.push({
              type: "string",
              value
            });
          }
          continue;
        }
        let WHITESPACE = /\s/;
        if (char && WHITESPACE.test(char)) {
          current++;
          continue;
        }
        let NUMBERS = /[0-9]/;
        if (char && NUMBERS.test(char) || char === "-" || char === ".") {
          let value = "";
          if (char === "-") {
            value += char;
            char = input[++current];
          }
          while (char && NUMBERS.test(char) || char === ".") {
            value += char;
            char = input[++current];
          }
          tokens.push({
            type: "number",
            value
          });
          continue;
        }
        let LETTERS = /[a-z]/i;
        if (char && LETTERS.test(char)) {
          let value = "";
          while (char && LETTERS.test(char)) {
            if (current === input.length) {
              break;
            }
            value += char;
            char = input[++current];
          }
          if (value == "true" || value == "false" || value === "null") {
            tokens.push({
              type: "name",
              value
            });
          } else {
            current++;
            continue;
          }
          continue;
        }
        current++;
      }
      return tokens;
    };
    strip = (tokens) => {
      if (tokens.length === 0) {
        return tokens;
      }
      let lastToken = tokens[tokens.length - 1];
      switch (lastToken.type) {
        case "separator":
          tokens = tokens.slice(0, tokens.length - 1);
          return strip(tokens);
          break;
        case "number":
          let lastCharacterOfLastToken = lastToken.value[lastToken.value.length - 1];
          if (lastCharacterOfLastToken === "." || lastCharacterOfLastToken === "-") {
            tokens = tokens.slice(0, tokens.length - 1);
            return strip(tokens);
          }
        case "string":
          let tokenBeforeTheLastToken = tokens[tokens.length - 2];
          if (tokenBeforeTheLastToken?.type === "delimiter") {
            tokens = tokens.slice(0, tokens.length - 1);
            return strip(tokens);
          } else if (tokenBeforeTheLastToken?.type === "brace" && tokenBeforeTheLastToken.value === "{") {
            tokens = tokens.slice(0, tokens.length - 1);
            return strip(tokens);
          }
          break;
        case "delimiter":
          tokens = tokens.slice(0, tokens.length - 1);
          return strip(tokens);
          break;
      }
      return tokens;
    };
    unstrip = (tokens) => {
      let tail = [];
      tokens.map((token) => {
        if (token.type === "brace") {
          if (token.value === "{") {
            tail.push("}");
          } else {
            tail.splice(tail.lastIndexOf("}"), 1);
          }
        }
        if (token.type === "paren") {
          if (token.value === "[") {
            tail.push("]");
          } else {
            tail.splice(tail.lastIndexOf("]"), 1);
          }
        }
      });
      if (tail.length > 0) {
        tail.reverse().map((item) => {
          if (item === "}") {
            tokens.push({
              type: "brace",
              value: "}"
            });
          } else if (item === "]") {
            tokens.push({
              type: "paren",
              value: "]"
            });
          }
        });
      }
      return tokens;
    };
    generate = (tokens) => {
      let output = "";
      tokens.map((token) => {
        switch (token.type) {
          case "string":
            output += '"' + token.value + '"';
            break;
          default:
            output += token.value;
            break;
        }
      });
      return output;
    };
    partialParse = (input) => JSON.parse(generate(unstrip(strip(tokenize2(input)))));
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/streaming.mjs
var init_streaming2 = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/streaming.mjs"() {
    init_streaming();
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/lib/BetaMessageStream.mjs
function tracksToolInput(content) {
  return content.type === "tool_use" || content.type === "server_tool_use" || content.type === "mcp_tool_use";
}
function checkNever(x) {
}
var _BetaMessageStream_instances, _BetaMessageStream_currentMessageSnapshot, _BetaMessageStream_params, _BetaMessageStream_connectedPromise, _BetaMessageStream_resolveConnectedPromise, _BetaMessageStream_rejectConnectedPromise, _BetaMessageStream_endPromise, _BetaMessageStream_resolveEndPromise, _BetaMessageStream_rejectEndPromise, _BetaMessageStream_listeners, _BetaMessageStream_ended, _BetaMessageStream_errored, _BetaMessageStream_aborted, _BetaMessageStream_catchingPromiseCreated, _BetaMessageStream_response, _BetaMessageStream_request_id, _BetaMessageStream_logger, _BetaMessageStream_getFinalMessage, _BetaMessageStream_getFinalText, _BetaMessageStream_handleError, _BetaMessageStream_beginRequest, _BetaMessageStream_addStreamEvent, _BetaMessageStream_endRequest, _BetaMessageStream_accumulateMessage, JSON_BUF_PROPERTY, BetaMessageStream;
var init_BetaMessageStream = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/lib/BetaMessageStream.mjs"() {
    init_tslib();
    init_parser();
    init_error2();
    init_errors();
    init_streaming2();
    init_beta_parser();
    JSON_BUF_PROPERTY = "__json_buf";
    BetaMessageStream = class _BetaMessageStream {
      constructor(params, opts) {
        _BetaMessageStream_instances.add(this);
        this.messages = [];
        this.receivedMessages = [];
        _BetaMessageStream_currentMessageSnapshot.set(this, void 0);
        _BetaMessageStream_params.set(this, null);
        this.controller = new AbortController();
        _BetaMessageStream_connectedPromise.set(this, void 0);
        _BetaMessageStream_resolveConnectedPromise.set(this, () => {
        });
        _BetaMessageStream_rejectConnectedPromise.set(this, () => {
        });
        _BetaMessageStream_endPromise.set(this, void 0);
        _BetaMessageStream_resolveEndPromise.set(this, () => {
        });
        _BetaMessageStream_rejectEndPromise.set(this, () => {
        });
        _BetaMessageStream_listeners.set(this, {});
        _BetaMessageStream_ended.set(this, false);
        _BetaMessageStream_errored.set(this, false);
        _BetaMessageStream_aborted.set(this, false);
        _BetaMessageStream_catchingPromiseCreated.set(this, false);
        _BetaMessageStream_response.set(this, void 0);
        _BetaMessageStream_request_id.set(this, void 0);
        _BetaMessageStream_logger.set(this, void 0);
        _BetaMessageStream_handleError.set(this, (error) => {
          __classPrivateFieldSet(this, _BetaMessageStream_errored, true, "f");
          if (isAbortError(error)) {
            error = new APIUserAbortError();
          }
          if (error instanceof APIUserAbortError) {
            __classPrivateFieldSet(this, _BetaMessageStream_aborted, true, "f");
            return this._emit("abort", error);
          }
          if (error instanceof AnthropicError) {
            return this._emit("error", error);
          }
          if (error instanceof Error) {
            const anthropicError = new AnthropicError(error.message);
            anthropicError.cause = error;
            return this._emit("error", anthropicError);
          }
          return this._emit("error", new AnthropicError(String(error)));
        });
        __classPrivateFieldSet(this, _BetaMessageStream_connectedPromise, new Promise((resolve4, reject) => {
          __classPrivateFieldSet(this, _BetaMessageStream_resolveConnectedPromise, resolve4, "f");
          __classPrivateFieldSet(this, _BetaMessageStream_rejectConnectedPromise, reject, "f");
        }), "f");
        __classPrivateFieldSet(this, _BetaMessageStream_endPromise, new Promise((resolve4, reject) => {
          __classPrivateFieldSet(this, _BetaMessageStream_resolveEndPromise, resolve4, "f");
          __classPrivateFieldSet(this, _BetaMessageStream_rejectEndPromise, reject, "f");
        }), "f");
        __classPrivateFieldGet(this, _BetaMessageStream_connectedPromise, "f").catch(() => {
        });
        __classPrivateFieldGet(this, _BetaMessageStream_endPromise, "f").catch(() => {
        });
        __classPrivateFieldSet(this, _BetaMessageStream_params, params, "f");
        __classPrivateFieldSet(this, _BetaMessageStream_logger, opts?.logger ?? console, "f");
      }
      get response() {
        return __classPrivateFieldGet(this, _BetaMessageStream_response, "f");
      }
      get request_id() {
        return __classPrivateFieldGet(this, _BetaMessageStream_request_id, "f");
      }
      /**
       * Returns the `MessageStream` data, the raw `Response` instance and the ID of the request,
       * returned vie the `request-id` header which is useful for debugging requests and resporting
       * issues to Anthropic.
       *
       * This is the same as the `APIPromise.withResponse()` method.
       *
       * This method will raise an error if you created the stream using `MessageStream.fromReadableStream`
       * as no `Response` is available.
       */
      async withResponse() {
        __classPrivateFieldSet(this, _BetaMessageStream_catchingPromiseCreated, true, "f");
        const response = await __classPrivateFieldGet(this, _BetaMessageStream_connectedPromise, "f");
        if (!response) {
          throw new Error("Could not resolve a `Response` object");
        }
        return {
          data: this,
          response,
          request_id: response.headers.get("request-id")
        };
      }
      /**
       * Intended for use on the frontend, consuming a stream produced with
       * `.toReadableStream()` on the backend.
       *
       * Note that messages sent to the model do not appear in `.on('message')`
       * in this context.
       */
      static fromReadableStream(stream) {
        const runner = new _BetaMessageStream(null);
        runner._run(() => runner._fromReadableStream(stream));
        return runner;
      }
      static createMessage(messages, params, options, { logger } = {}) {
        const runner = new _BetaMessageStream(params, { logger });
        for (const message of params.messages) {
          runner._addMessageParam(message);
        }
        __classPrivateFieldSet(runner, _BetaMessageStream_params, { ...params, stream: true }, "f");
        runner._run(() => runner._createMessage(messages, { ...params, stream: true }, { ...options, headers: { ...options?.headers, "X-Stainless-Helper-Method": "stream" } }));
        return runner;
      }
      _run(executor) {
        executor().then(() => {
          this._emitFinal();
          this._emit("end");
        }, __classPrivateFieldGet(this, _BetaMessageStream_handleError, "f"));
      }
      _addMessageParam(message) {
        this.messages.push(message);
      }
      _addMessage(message, emit = true) {
        this.receivedMessages.push(message);
        if (emit) {
          this._emit("message", message);
        }
      }
      async _createMessage(messages, params, options) {
        const signal = options?.signal;
        let abortHandler;
        if (signal) {
          if (signal.aborted)
            this.controller.abort();
          abortHandler = this.controller.abort.bind(this.controller);
          signal.addEventListener("abort", abortHandler);
        }
        try {
          __classPrivateFieldGet(this, _BetaMessageStream_instances, "m", _BetaMessageStream_beginRequest).call(this);
          const { response, data: stream } = await messages.create({ ...params, stream: true }, { ...options, signal: this.controller.signal }).withResponse();
          this._connected(response);
          for await (const event of stream) {
            __classPrivateFieldGet(this, _BetaMessageStream_instances, "m", _BetaMessageStream_addStreamEvent).call(this, event);
          }
          if (stream.controller.signal?.aborted) {
            throw new APIUserAbortError();
          }
          __classPrivateFieldGet(this, _BetaMessageStream_instances, "m", _BetaMessageStream_endRequest).call(this);
        } finally {
          if (signal && abortHandler) {
            signal.removeEventListener("abort", abortHandler);
          }
        }
      }
      _connected(response) {
        if (this.ended)
          return;
        __classPrivateFieldSet(this, _BetaMessageStream_response, response, "f");
        __classPrivateFieldSet(this, _BetaMessageStream_request_id, response?.headers.get("request-id"), "f");
        __classPrivateFieldGet(this, _BetaMessageStream_resolveConnectedPromise, "f").call(this, response);
        this._emit("connect");
      }
      get ended() {
        return __classPrivateFieldGet(this, _BetaMessageStream_ended, "f");
      }
      get errored() {
        return __classPrivateFieldGet(this, _BetaMessageStream_errored, "f");
      }
      get aborted() {
        return __classPrivateFieldGet(this, _BetaMessageStream_aborted, "f");
      }
      abort() {
        this.controller.abort();
      }
      /**
       * Adds the listener function to the end of the listeners array for the event.
       * No checks are made to see if the listener has already been added. Multiple calls passing
       * the same combination of event and listener will result in the listener being added, and
       * called, multiple times.
       * @returns this MessageStream, so that calls can be chained
       */
      on(event, listener) {
        const listeners = __classPrivateFieldGet(this, _BetaMessageStream_listeners, "f")[event] || (__classPrivateFieldGet(this, _BetaMessageStream_listeners, "f")[event] = []);
        listeners.push({ listener });
        return this;
      }
      /**
       * Removes the specified listener from the listener array for the event.
       * off() will remove, at most, one instance of a listener from the listener array. If any single
       * listener has been added multiple times to the listener array for the specified event, then
       * off() must be called multiple times to remove each instance.
       * @returns this MessageStream, so that calls can be chained
       */
      off(event, listener) {
        const listeners = __classPrivateFieldGet(this, _BetaMessageStream_listeners, "f")[event];
        if (!listeners)
          return this;
        const index = listeners.findIndex((l) => l.listener === listener);
        if (index >= 0)
          listeners.splice(index, 1);
        return this;
      }
      /**
       * Adds a one-time listener function for the event. The next time the event is triggered,
       * this listener is removed and then invoked.
       * @returns this MessageStream, so that calls can be chained
       */
      once(event, listener) {
        const listeners = __classPrivateFieldGet(this, _BetaMessageStream_listeners, "f")[event] || (__classPrivateFieldGet(this, _BetaMessageStream_listeners, "f")[event] = []);
        listeners.push({ listener, once: true });
        return this;
      }
      /**
       * This is similar to `.once()`, but returns a Promise that resolves the next time
       * the event is triggered, instead of calling a listener callback.
       * @returns a Promise that resolves the next time given event is triggered,
       * or rejects if an error is emitted.  (If you request the 'error' event,
       * returns a promise that resolves with the error).
       *
       * Example:
       *
       *   const message = await stream.emitted('message') // rejects if the stream errors
       */
      emitted(event) {
        return new Promise((resolve4, reject) => {
          __classPrivateFieldSet(this, _BetaMessageStream_catchingPromiseCreated, true, "f");
          if (event !== "error")
            this.once("error", reject);
          this.once(event, resolve4);
        });
      }
      async done() {
        __classPrivateFieldSet(this, _BetaMessageStream_catchingPromiseCreated, true, "f");
        await __classPrivateFieldGet(this, _BetaMessageStream_endPromise, "f");
      }
      get currentMessage() {
        return __classPrivateFieldGet(this, _BetaMessageStream_currentMessageSnapshot, "f");
      }
      /**
       * @returns a promise that resolves with the the final assistant Message response,
       * or rejects if an error occurred or the stream ended prematurely without producing a Message.
       * If structured outputs were used, this will be a ParsedMessage with a `parsed` field.
       */
      async finalMessage() {
        await this.done();
        return __classPrivateFieldGet(this, _BetaMessageStream_instances, "m", _BetaMessageStream_getFinalMessage).call(this);
      }
      /**
       * @returns a promise that resolves with the the final assistant Message's text response, concatenated
       * together if there are more than one text blocks.
       * Rejects if an error occurred or the stream ended prematurely without producing a Message.
       */
      async finalText() {
        await this.done();
        return __classPrivateFieldGet(this, _BetaMessageStream_instances, "m", _BetaMessageStream_getFinalText).call(this);
      }
      _emit(event, ...args) {
        if (__classPrivateFieldGet(this, _BetaMessageStream_ended, "f"))
          return;
        if (event === "end") {
          __classPrivateFieldSet(this, _BetaMessageStream_ended, true, "f");
          __classPrivateFieldGet(this, _BetaMessageStream_resolveEndPromise, "f").call(this);
        }
        const listeners = __classPrivateFieldGet(this, _BetaMessageStream_listeners, "f")[event];
        if (listeners) {
          __classPrivateFieldGet(this, _BetaMessageStream_listeners, "f")[event] = listeners.filter((l) => !l.once);
          listeners.forEach(({ listener }) => listener(...args));
        }
        if (event === "abort") {
          const error = args[0];
          if (!__classPrivateFieldGet(this, _BetaMessageStream_catchingPromiseCreated, "f") && !listeners?.length) {
            Promise.reject(error);
          }
          __classPrivateFieldGet(this, _BetaMessageStream_rejectConnectedPromise, "f").call(this, error);
          __classPrivateFieldGet(this, _BetaMessageStream_rejectEndPromise, "f").call(this, error);
          this._emit("end");
          return;
        }
        if (event === "error") {
          const error = args[0];
          if (!__classPrivateFieldGet(this, _BetaMessageStream_catchingPromiseCreated, "f") && !listeners?.length) {
            Promise.reject(error);
          }
          __classPrivateFieldGet(this, _BetaMessageStream_rejectConnectedPromise, "f").call(this, error);
          __classPrivateFieldGet(this, _BetaMessageStream_rejectEndPromise, "f").call(this, error);
          this._emit("end");
        }
      }
      _emitFinal() {
        const finalMessage = this.receivedMessages.at(-1);
        if (finalMessage) {
          this._emit("finalMessage", __classPrivateFieldGet(this, _BetaMessageStream_instances, "m", _BetaMessageStream_getFinalMessage).call(this));
        }
      }
      async _fromReadableStream(readableStream, options) {
        const signal = options?.signal;
        let abortHandler;
        if (signal) {
          if (signal.aborted)
            this.controller.abort();
          abortHandler = this.controller.abort.bind(this.controller);
          signal.addEventListener("abort", abortHandler);
        }
        try {
          __classPrivateFieldGet(this, _BetaMessageStream_instances, "m", _BetaMessageStream_beginRequest).call(this);
          this._connected(null);
          const stream = Stream.fromReadableStream(readableStream, this.controller);
          for await (const event of stream) {
            __classPrivateFieldGet(this, _BetaMessageStream_instances, "m", _BetaMessageStream_addStreamEvent).call(this, event);
          }
          if (stream.controller.signal?.aborted) {
            throw new APIUserAbortError();
          }
          __classPrivateFieldGet(this, _BetaMessageStream_instances, "m", _BetaMessageStream_endRequest).call(this);
        } finally {
          if (signal && abortHandler) {
            signal.removeEventListener("abort", abortHandler);
          }
        }
      }
      [(_BetaMessageStream_currentMessageSnapshot = /* @__PURE__ */ new WeakMap(), _BetaMessageStream_params = /* @__PURE__ */ new WeakMap(), _BetaMessageStream_connectedPromise = /* @__PURE__ */ new WeakMap(), _BetaMessageStream_resolveConnectedPromise = /* @__PURE__ */ new WeakMap(), _BetaMessageStream_rejectConnectedPromise = /* @__PURE__ */ new WeakMap(), _BetaMessageStream_endPromise = /* @__PURE__ */ new WeakMap(), _BetaMessageStream_resolveEndPromise = /* @__PURE__ */ new WeakMap(), _BetaMessageStream_rejectEndPromise = /* @__PURE__ */ new WeakMap(), _BetaMessageStream_listeners = /* @__PURE__ */ new WeakMap(), _BetaMessageStream_ended = /* @__PURE__ */ new WeakMap(), _BetaMessageStream_errored = /* @__PURE__ */ new WeakMap(), _BetaMessageStream_aborted = /* @__PURE__ */ new WeakMap(), _BetaMessageStream_catchingPromiseCreated = /* @__PURE__ */ new WeakMap(), _BetaMessageStream_response = /* @__PURE__ */ new WeakMap(), _BetaMessageStream_request_id = /* @__PURE__ */ new WeakMap(), _BetaMessageStream_logger = /* @__PURE__ */ new WeakMap(), _BetaMessageStream_handleError = /* @__PURE__ */ new WeakMap(), _BetaMessageStream_instances = /* @__PURE__ */ new WeakSet(), _BetaMessageStream_getFinalMessage = function _BetaMessageStream_getFinalMessage2() {
        if (this.receivedMessages.length === 0) {
          throw new AnthropicError("stream ended without producing a Message with role=assistant");
        }
        return this.receivedMessages.at(-1);
      }, _BetaMessageStream_getFinalText = function _BetaMessageStream_getFinalText2() {
        if (this.receivedMessages.length === 0) {
          throw new AnthropicError("stream ended without producing a Message with role=assistant");
        }
        const textBlocks = this.receivedMessages.at(-1).content.filter((block) => block.type === "text").map((block) => block.text);
        if (textBlocks.length === 0) {
          throw new AnthropicError("stream ended without producing a content block with type=text");
        }
        return textBlocks.join(" ");
      }, _BetaMessageStream_beginRequest = function _BetaMessageStream_beginRequest2() {
        if (this.ended)
          return;
        __classPrivateFieldSet(this, _BetaMessageStream_currentMessageSnapshot, void 0, "f");
      }, _BetaMessageStream_addStreamEvent = function _BetaMessageStream_addStreamEvent2(event) {
        if (this.ended)
          return;
        const messageSnapshot = __classPrivateFieldGet(this, _BetaMessageStream_instances, "m", _BetaMessageStream_accumulateMessage).call(this, event);
        this._emit("streamEvent", event, messageSnapshot);
        switch (event.type) {
          case "content_block_delta": {
            const content = messageSnapshot.content.at(-1);
            switch (event.delta.type) {
              case "text_delta": {
                if (content.type === "text") {
                  this._emit("text", event.delta.text, content.text || "");
                }
                break;
              }
              case "citations_delta": {
                if (content.type === "text") {
                  this._emit("citation", event.delta.citation, content.citations ?? []);
                }
                break;
              }
              case "input_json_delta": {
                if (tracksToolInput(content) && content.input) {
                  this._emit("inputJson", event.delta.partial_json, content.input);
                }
                break;
              }
              case "thinking_delta": {
                if (content.type === "thinking") {
                  this._emit("thinking", event.delta.thinking, content.thinking);
                }
                break;
              }
              case "signature_delta": {
                if (content.type === "thinking") {
                  this._emit("signature", content.signature);
                }
                break;
              }
              case "compaction_delta": {
                if (content.type === "compaction" && content.content) {
                  this._emit("compaction", content.content);
                }
                break;
              }
              default:
                checkNever(event.delta);
            }
            break;
          }
          case "message_stop": {
            this._addMessageParam(messageSnapshot);
            this._addMessage(maybeParseBetaMessage(messageSnapshot, __classPrivateFieldGet(this, _BetaMessageStream_params, "f"), { logger: __classPrivateFieldGet(this, _BetaMessageStream_logger, "f") }), true);
            break;
          }
          case "content_block_stop": {
            this._emit("contentBlock", messageSnapshot.content.at(-1));
            break;
          }
          case "message_start": {
            __classPrivateFieldSet(this, _BetaMessageStream_currentMessageSnapshot, messageSnapshot, "f");
            break;
          }
          case "content_block_start":
          case "message_delta":
            break;
        }
      }, _BetaMessageStream_endRequest = function _BetaMessageStream_endRequest2() {
        if (this.ended) {
          throw new AnthropicError(`stream has ended, this shouldn't happen`);
        }
        const snapshot = __classPrivateFieldGet(this, _BetaMessageStream_currentMessageSnapshot, "f");
        if (!snapshot) {
          throw new AnthropicError(`request ended without sending any chunks`);
        }
        __classPrivateFieldSet(this, _BetaMessageStream_currentMessageSnapshot, void 0, "f");
        return maybeParseBetaMessage(snapshot, __classPrivateFieldGet(this, _BetaMessageStream_params, "f"), { logger: __classPrivateFieldGet(this, _BetaMessageStream_logger, "f") });
      }, _BetaMessageStream_accumulateMessage = function _BetaMessageStream_accumulateMessage2(event) {
        let snapshot = __classPrivateFieldGet(this, _BetaMessageStream_currentMessageSnapshot, "f");
        if (event.type === "message_start") {
          if (snapshot) {
            throw new AnthropicError(`Unexpected event order, got ${event.type} before receiving "message_stop"`);
          }
          return event.message;
        }
        if (!snapshot) {
          throw new AnthropicError(`Unexpected event order, got ${event.type} before "message_start"`);
        }
        switch (event.type) {
          case "message_stop":
            return snapshot;
          case "message_delta":
            snapshot.container = event.delta.container;
            snapshot.stop_reason = event.delta.stop_reason;
            snapshot.stop_sequence = event.delta.stop_sequence;
            snapshot.usage.output_tokens = event.usage.output_tokens;
            snapshot.context_management = event.context_management;
            if (event.usage.input_tokens != null) {
              snapshot.usage.input_tokens = event.usage.input_tokens;
            }
            if (event.usage.cache_creation_input_tokens != null) {
              snapshot.usage.cache_creation_input_tokens = event.usage.cache_creation_input_tokens;
            }
            if (event.usage.cache_read_input_tokens != null) {
              snapshot.usage.cache_read_input_tokens = event.usage.cache_read_input_tokens;
            }
            if (event.usage.server_tool_use != null) {
              snapshot.usage.server_tool_use = event.usage.server_tool_use;
            }
            if (event.usage.iterations != null) {
              snapshot.usage.iterations = event.usage.iterations;
            }
            return snapshot;
          case "content_block_start":
            snapshot.content.push(event.content_block);
            return snapshot;
          case "content_block_delta": {
            const snapshotContent = snapshot.content.at(event.index);
            switch (event.delta.type) {
              case "text_delta": {
                if (snapshotContent?.type === "text") {
                  snapshot.content[event.index] = {
                    ...snapshotContent,
                    text: (snapshotContent.text || "") + event.delta.text
                  };
                }
                break;
              }
              case "citations_delta": {
                if (snapshotContent?.type === "text") {
                  snapshot.content[event.index] = {
                    ...snapshotContent,
                    citations: [...snapshotContent.citations ?? [], event.delta.citation]
                  };
                }
                break;
              }
              case "input_json_delta": {
                if (snapshotContent && tracksToolInput(snapshotContent)) {
                  let jsonBuf = snapshotContent[JSON_BUF_PROPERTY] || "";
                  jsonBuf += event.delta.partial_json;
                  const newContent = { ...snapshotContent };
                  Object.defineProperty(newContent, JSON_BUF_PROPERTY, {
                    value: jsonBuf,
                    enumerable: false,
                    writable: true
                  });
                  if (jsonBuf) {
                    try {
                      newContent.input = partialParse(jsonBuf);
                    } catch (err) {
                      const error = new AnthropicError(`Unable to parse tool parameter JSON from model. Please retry your request or adjust your prompt. Error: ${err}. JSON: ${jsonBuf}`);
                      __classPrivateFieldGet(this, _BetaMessageStream_handleError, "f").call(this, error);
                    }
                  }
                  snapshot.content[event.index] = newContent;
                }
                break;
              }
              case "thinking_delta": {
                if (snapshotContent?.type === "thinking") {
                  snapshot.content[event.index] = {
                    ...snapshotContent,
                    thinking: snapshotContent.thinking + event.delta.thinking
                  };
                }
                break;
              }
              case "signature_delta": {
                if (snapshotContent?.type === "thinking") {
                  snapshot.content[event.index] = {
                    ...snapshotContent,
                    signature: event.delta.signature
                  };
                }
                break;
              }
              case "compaction_delta": {
                if (snapshotContent?.type === "compaction") {
                  snapshot.content[event.index] = {
                    ...snapshotContent,
                    content: (snapshotContent.content || "") + event.delta.content
                  };
                }
                break;
              }
              default:
                checkNever(event.delta);
            }
            return snapshot;
          }
          case "content_block_stop":
            return snapshot;
        }
      }, Symbol.asyncIterator)]() {
        const pushQueue = [];
        const readQueue = [];
        let done = false;
        this.on("streamEvent", (event) => {
          const reader = readQueue.shift();
          if (reader) {
            reader.resolve(event);
          } else {
            pushQueue.push(event);
          }
        });
        this.on("end", () => {
          done = true;
          for (const reader of readQueue) {
            reader.resolve(void 0);
          }
          readQueue.length = 0;
        });
        this.on("abort", (err) => {
          done = true;
          for (const reader of readQueue) {
            reader.reject(err);
          }
          readQueue.length = 0;
        });
        this.on("error", (err) => {
          done = true;
          for (const reader of readQueue) {
            reader.reject(err);
          }
          readQueue.length = 0;
        });
        return {
          next: async () => {
            if (!pushQueue.length) {
              if (done) {
                return { value: void 0, done: true };
              }
              return new Promise((resolve4, reject) => readQueue.push({ resolve: resolve4, reject })).then((chunk2) => chunk2 ? { value: chunk2, done: false } : { value: void 0, done: true });
            }
            const chunk = pushQueue.shift();
            return { value: chunk, done: false };
          },
          return: async () => {
            this.abort();
            return { value: void 0, done: true };
          }
        };
      }
      toReadableStream() {
        const stream = new Stream(this[Symbol.asyncIterator].bind(this), this.controller);
        return stream.toReadableStream();
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/lib/tools/ToolError.mjs
var ToolError;
var init_ToolError = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/lib/tools/ToolError.mjs"() {
    ToolError = class extends Error {
      constructor(content) {
        const message = typeof content === "string" ? content : content.map((block) => {
          if (block.type === "text")
            return block.text;
          return `[${block.type}]`;
        }).join(" ");
        super(message);
        this.name = "ToolError";
        this.content = content;
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/lib/tools/CompactionControl.mjs
var DEFAULT_TOKEN_THRESHOLD, DEFAULT_SUMMARY_PROMPT;
var init_CompactionControl = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/lib/tools/CompactionControl.mjs"() {
    DEFAULT_TOKEN_THRESHOLD = 1e5;
    DEFAULT_SUMMARY_PROMPT = `You have been working on the task described above but have not yet completed it. Write a continuation summary that will allow you (or another instance of yourself) to resume work efficiently in a future context window where the conversation history will be replaced with this summary. Your summary should be structured, concise, and actionable. Include:
1. Task Overview
The user's core request and success criteria
Any clarifications or constraints they specified
2. Current State
What has been completed so far
Files created, modified, or analyzed (with paths if relevant)
Key outputs or artifacts produced
3. Important Discoveries
Technical constraints or requirements uncovered
Decisions made and their rationale
Errors encountered and how they were resolved
What approaches were tried that didn't work (and why)
4. Next Steps
Specific actions needed to complete the task
Any blockers or open questions to resolve
Priority order if multiple steps remain
5. Context to Preserve
User preferences or style requirements
Domain-specific details that aren't obvious
Any promises made to the user
Be concise but complete\u2014err on the side of including information that would prevent duplicate work or repeated mistakes. Write in a way that enables immediate resumption of the task.
Wrap your summary in <summary></summary> tags.`;
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/lib/tools/BetaToolRunner.mjs
function promiseWithResolvers() {
  let resolve4;
  let reject;
  const promise = new Promise((res, rej) => {
    resolve4 = res;
    reject = rej;
  });
  return { promise, resolve: resolve4, reject };
}
async function generateToolResponse(params, lastMessage = params.messages.at(-1), requestOptions) {
  if (!lastMessage || lastMessage.role !== "assistant" || !lastMessage.content || typeof lastMessage.content === "string") {
    return null;
  }
  const toolUseBlocks = lastMessage.content.filter((content) => content.type === "tool_use");
  if (toolUseBlocks.length === 0) {
    return null;
  }
  const toolResults = await Promise.all(toolUseBlocks.map(async (toolUse) => {
    const tool = params.tools.find((t) => ("name" in t ? t.name : t.mcp_server_name) === toolUse.name);
    if (!tool || !("run" in tool)) {
      return {
        type: "tool_result",
        tool_use_id: toolUse.id,
        content: `Error: Tool '${toolUse.name}' not found`,
        is_error: true
      };
    }
    try {
      let input = toolUse.input;
      if ("parse" in tool && tool.parse) {
        input = tool.parse(input);
      }
      const result = await tool.run(input, {
        toolUseBlock: toolUse,
        signal: requestOptions?.signal
      });
      return {
        type: "tool_result",
        tool_use_id: toolUse.id,
        content: result
      };
    } catch (error) {
      return {
        type: "tool_result",
        tool_use_id: toolUse.id,
        content: error instanceof ToolError ? error.content : `Error: ${error instanceof Error ? error.message : String(error)}`,
        is_error: true
      };
    }
  }));
  return {
    role: "user",
    content: toolResults
  };
}
var _BetaToolRunner_instances, _BetaToolRunner_consumed, _BetaToolRunner_mutated, _BetaToolRunner_state, _BetaToolRunner_options, _BetaToolRunner_message, _BetaToolRunner_toolResponse, _BetaToolRunner_completion, _BetaToolRunner_iterationCount, _BetaToolRunner_checkAndCompact, _BetaToolRunner_generateToolResponse, BetaToolRunner;
var init_BetaToolRunner = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/lib/tools/BetaToolRunner.mjs"() {
    init_tslib();
    init_ToolError();
    init_error();
    init_headers();
    init_CompactionControl();
    init_stainless_helper_header();
    BetaToolRunner = class {
      constructor(client, params, options) {
        _BetaToolRunner_instances.add(this);
        this.client = client;
        _BetaToolRunner_consumed.set(this, false);
        _BetaToolRunner_mutated.set(this, false);
        _BetaToolRunner_state.set(this, void 0);
        _BetaToolRunner_options.set(this, void 0);
        _BetaToolRunner_message.set(this, void 0);
        _BetaToolRunner_toolResponse.set(this, void 0);
        _BetaToolRunner_completion.set(this, void 0);
        _BetaToolRunner_iterationCount.set(this, 0);
        __classPrivateFieldSet(this, _BetaToolRunner_state, {
          params: {
            // You can't clone the entire params since there are functions as handlers.
            // You also don't really need to clone params.messages, but it probably will prevent a foot gun
            // somewhere.
            ...params,
            messages: structuredClone(params.messages)
          }
        }, "f");
        const helpers = collectStainlessHelpers(params.tools, params.messages);
        const helperValue = ["BetaToolRunner", ...helpers].join(", ");
        __classPrivateFieldSet(this, _BetaToolRunner_options, {
          ...options,
          headers: buildHeaders([{ "x-stainless-helper": helperValue }, options?.headers])
        }, "f");
        __classPrivateFieldSet(this, _BetaToolRunner_completion, promiseWithResolvers(), "f");
        if (params.compactionControl?.enabled) {
          console.warn('Anthropic: The `compactionControl` parameter is deprecated and will be removed in a future version. Use server-side compaction instead by passing `edits: [{ type: "compact_20260112" }]` in the params passed to `toolRunner()`. See https://platform.claude.com/docs/en/build-with-claude/compaction');
        }
      }
      async *[(_BetaToolRunner_consumed = /* @__PURE__ */ new WeakMap(), _BetaToolRunner_mutated = /* @__PURE__ */ new WeakMap(), _BetaToolRunner_state = /* @__PURE__ */ new WeakMap(), _BetaToolRunner_options = /* @__PURE__ */ new WeakMap(), _BetaToolRunner_message = /* @__PURE__ */ new WeakMap(), _BetaToolRunner_toolResponse = /* @__PURE__ */ new WeakMap(), _BetaToolRunner_completion = /* @__PURE__ */ new WeakMap(), _BetaToolRunner_iterationCount = /* @__PURE__ */ new WeakMap(), _BetaToolRunner_instances = /* @__PURE__ */ new WeakSet(), _BetaToolRunner_checkAndCompact = async function _BetaToolRunner_checkAndCompact2() {
        const compactionControl = __classPrivateFieldGet(this, _BetaToolRunner_state, "f").params.compactionControl;
        if (!compactionControl || !compactionControl.enabled) {
          return false;
        }
        let tokensUsed = 0;
        if (__classPrivateFieldGet(this, _BetaToolRunner_message, "f") !== void 0) {
          try {
            const message = await __classPrivateFieldGet(this, _BetaToolRunner_message, "f");
            const totalInputTokens = message.usage.input_tokens + (message.usage.cache_creation_input_tokens ?? 0) + (message.usage.cache_read_input_tokens ?? 0);
            tokensUsed = totalInputTokens + message.usage.output_tokens;
          } catch {
            return false;
          }
        }
        const threshold = compactionControl.contextTokenThreshold ?? DEFAULT_TOKEN_THRESHOLD;
        if (tokensUsed < threshold) {
          return false;
        }
        const model = compactionControl.model ?? __classPrivateFieldGet(this, _BetaToolRunner_state, "f").params.model;
        const summaryPrompt = compactionControl.summaryPrompt ?? DEFAULT_SUMMARY_PROMPT;
        const messages = __classPrivateFieldGet(this, _BetaToolRunner_state, "f").params.messages;
        if (messages[messages.length - 1].role === "assistant") {
          const lastMessage = messages[messages.length - 1];
          if (Array.isArray(lastMessage.content)) {
            const nonToolBlocks = lastMessage.content.filter((block) => block.type !== "tool_use");
            if (nonToolBlocks.length === 0) {
              messages.pop();
            } else {
              lastMessage.content = nonToolBlocks;
            }
          }
        }
        const response = await this.client.beta.messages.create({
          model,
          messages: [
            ...messages,
            {
              role: "user",
              content: [
                {
                  type: "text",
                  text: summaryPrompt
                }
              ]
            }
          ],
          max_tokens: __classPrivateFieldGet(this, _BetaToolRunner_state, "f").params.max_tokens
        }, {
          signal: __classPrivateFieldGet(this, _BetaToolRunner_options, "f").signal,
          headers: buildHeaders([__classPrivateFieldGet(this, _BetaToolRunner_options, "f").headers, { "x-stainless-helper": "compaction" }])
        });
        if (response.content[0]?.type !== "text") {
          throw new AnthropicError("Expected text response for compaction");
        }
        __classPrivateFieldGet(this, _BetaToolRunner_state, "f").params.messages = [
          {
            role: "user",
            content: response.content
          }
        ];
        return true;
      }, Symbol.asyncIterator)]() {
        var _a2;
        if (__classPrivateFieldGet(this, _BetaToolRunner_consumed, "f")) {
          throw new AnthropicError("Cannot iterate over a consumed stream");
        }
        __classPrivateFieldSet(this, _BetaToolRunner_consumed, true, "f");
        __classPrivateFieldSet(this, _BetaToolRunner_mutated, true, "f");
        __classPrivateFieldSet(this, _BetaToolRunner_toolResponse, void 0, "f");
        try {
          while (true) {
            let stream;
            try {
              if (__classPrivateFieldGet(this, _BetaToolRunner_state, "f").params.max_iterations && __classPrivateFieldGet(this, _BetaToolRunner_iterationCount, "f") >= __classPrivateFieldGet(this, _BetaToolRunner_state, "f").params.max_iterations) {
                break;
              }
              __classPrivateFieldSet(this, _BetaToolRunner_mutated, false, "f");
              __classPrivateFieldSet(this, _BetaToolRunner_toolResponse, void 0, "f");
              __classPrivateFieldSet(this, _BetaToolRunner_iterationCount, (_a2 = __classPrivateFieldGet(this, _BetaToolRunner_iterationCount, "f"), _a2++, _a2), "f");
              __classPrivateFieldSet(this, _BetaToolRunner_message, void 0, "f");
              const { max_iterations, compactionControl, ...params } = __classPrivateFieldGet(this, _BetaToolRunner_state, "f").params;
              if (params.stream) {
                stream = this.client.beta.messages.stream({ ...params }, __classPrivateFieldGet(this, _BetaToolRunner_options, "f"));
                __classPrivateFieldSet(this, _BetaToolRunner_message, stream.finalMessage(), "f");
                __classPrivateFieldGet(this, _BetaToolRunner_message, "f").catch(() => {
                });
                yield stream;
              } else {
                __classPrivateFieldSet(this, _BetaToolRunner_message, this.client.beta.messages.create({ ...params, stream: false }, __classPrivateFieldGet(this, _BetaToolRunner_options, "f")), "f");
                yield __classPrivateFieldGet(this, _BetaToolRunner_message, "f");
              }
              const isCompacted = await __classPrivateFieldGet(this, _BetaToolRunner_instances, "m", _BetaToolRunner_checkAndCompact).call(this);
              if (!isCompacted) {
                if (!__classPrivateFieldGet(this, _BetaToolRunner_mutated, "f")) {
                  const { role, content } = await __classPrivateFieldGet(this, _BetaToolRunner_message, "f");
                  __classPrivateFieldGet(this, _BetaToolRunner_state, "f").params.messages.push({ role, content });
                }
                const toolMessage = await __classPrivateFieldGet(this, _BetaToolRunner_instances, "m", _BetaToolRunner_generateToolResponse).call(this, __classPrivateFieldGet(this, _BetaToolRunner_state, "f").params.messages.at(-1));
                if (toolMessage) {
                  __classPrivateFieldGet(this, _BetaToolRunner_state, "f").params.messages.push(toolMessage);
                } else if (!__classPrivateFieldGet(this, _BetaToolRunner_mutated, "f")) {
                  break;
                }
              }
            } finally {
              if (stream) {
                stream.abort();
              }
            }
          }
          if (!__classPrivateFieldGet(this, _BetaToolRunner_message, "f")) {
            throw new AnthropicError("ToolRunner concluded without a message from the server");
          }
          __classPrivateFieldGet(this, _BetaToolRunner_completion, "f").resolve(await __classPrivateFieldGet(this, _BetaToolRunner_message, "f"));
        } catch (error) {
          __classPrivateFieldSet(this, _BetaToolRunner_consumed, false, "f");
          __classPrivateFieldGet(this, _BetaToolRunner_completion, "f").promise.catch(() => {
          });
          __classPrivateFieldGet(this, _BetaToolRunner_completion, "f").reject(error);
          __classPrivateFieldSet(this, _BetaToolRunner_completion, promiseWithResolvers(), "f");
          throw error;
        }
      }
      setMessagesParams(paramsOrMutator) {
        if (typeof paramsOrMutator === "function") {
          __classPrivateFieldGet(this, _BetaToolRunner_state, "f").params = paramsOrMutator(__classPrivateFieldGet(this, _BetaToolRunner_state, "f").params);
        } else {
          __classPrivateFieldGet(this, _BetaToolRunner_state, "f").params = paramsOrMutator;
        }
        __classPrivateFieldSet(this, _BetaToolRunner_mutated, true, "f");
        __classPrivateFieldSet(this, _BetaToolRunner_toolResponse, void 0, "f");
      }
      setRequestOptions(optionsOrMutator) {
        if (typeof optionsOrMutator === "function") {
          __classPrivateFieldSet(this, _BetaToolRunner_options, optionsOrMutator(__classPrivateFieldGet(this, _BetaToolRunner_options, "f")), "f");
        } else {
          __classPrivateFieldSet(this, _BetaToolRunner_options, { ...__classPrivateFieldGet(this, _BetaToolRunner_options, "f"), ...optionsOrMutator }, "f");
        }
      }
      /**
       * Get the tool response for the last message from the assistant.
       * Avoids redundant tool executions by caching results.
       *
       * @returns A promise that resolves to a BetaMessageParam containing tool results, or null if no tools need to be executed
       *
       * @example
       * const toolResponse = await runner.generateToolResponse();
       * if (toolResponse) {
       *   console.log('Tool results:', toolResponse.content);
       * }
       */
      async generateToolResponse(signal = __classPrivateFieldGet(this, _BetaToolRunner_options, "f").signal) {
        const message = await __classPrivateFieldGet(this, _BetaToolRunner_message, "f") ?? this.params.messages.at(-1);
        if (!message) {
          return null;
        }
        return __classPrivateFieldGet(this, _BetaToolRunner_instances, "m", _BetaToolRunner_generateToolResponse).call(this, message, signal);
      }
      /**
       * Wait for the async iterator to complete. This works even if the async iterator hasn't yet started, and
       * will wait for an instance to start and go to completion.
       *
       * @returns A promise that resolves to the final BetaMessage when the iterator completes
       *
       * @example
       * // Start consuming the iterator
       * for await (const message of runner) {
       *   console.log('Message:', message.content);
       * }
       *
       * // Meanwhile, wait for completion from another part of the code
       * const finalMessage = await runner.done();
       * console.log('Final response:', finalMessage.content);
       */
      done() {
        return __classPrivateFieldGet(this, _BetaToolRunner_completion, "f").promise;
      }
      /**
       * Returns a promise indicating that the stream is done. Unlike .done(), this will eagerly read the stream:
       * * If the iterator has not been consumed, consume the entire iterator and return the final message from the
       * assistant.
       * * If the iterator has been consumed, waits for it to complete and returns the final message.
       *
       * @returns A promise that resolves to the final BetaMessage from the conversation
       * @throws {AnthropicError} If no messages were processed during the conversation
       *
       * @example
       * const finalMessage = await runner.runUntilDone();
       * console.log('Final response:', finalMessage.content);
       */
      async runUntilDone() {
        if (!__classPrivateFieldGet(this, _BetaToolRunner_consumed, "f")) {
          for await (const _ of this) {
          }
        }
        return this.done();
      }
      /**
       * Get the current parameters being used by the ToolRunner.
       *
       * @returns A readonly view of the current ToolRunnerParams
       *
       * @example
       * const currentParams = runner.params;
       * console.log('Current model:', currentParams.model);
       * console.log('Message count:', currentParams.messages.length);
       */
      get params() {
        return __classPrivateFieldGet(this, _BetaToolRunner_state, "f").params;
      }
      /**
       * Add one or more messages to the conversation history.
       *
       * @param messages - One or more BetaMessageParam objects to add to the conversation
       *
       * @example
       * runner.pushMessages(
       *   { role: 'user', content: 'Also, what about the weather in NYC?' }
       * );
       *
       * @example
       * // Adding multiple messages
       * runner.pushMessages(
       *   { role: 'user', content: 'What about NYC?' },
       *   { role: 'user', content: 'And Boston?' }
       * );
       */
      pushMessages(...messages) {
        this.setMessagesParams((params) => ({
          ...params,
          messages: [...params.messages, ...messages]
        }));
      }
      /**
       * Makes the ToolRunner directly awaitable, equivalent to calling .runUntilDone()
       * This allows using `await runner` instead of `await runner.runUntilDone()`
       */
      then(onfulfilled, onrejected) {
        return this.runUntilDone().then(onfulfilled, onrejected);
      }
    };
    _BetaToolRunner_generateToolResponse = async function _BetaToolRunner_generateToolResponse2(lastMessage, signal = __classPrivateFieldGet(this, _BetaToolRunner_options, "f").signal) {
      if (__classPrivateFieldGet(this, _BetaToolRunner_toolResponse, "f") !== void 0) {
        return __classPrivateFieldGet(this, _BetaToolRunner_toolResponse, "f");
      }
      __classPrivateFieldSet(this, _BetaToolRunner_toolResponse, generateToolResponse(__classPrivateFieldGet(this, _BetaToolRunner_state, "f").params, lastMessage, {
        ...__classPrivateFieldGet(this, _BetaToolRunner_options, "f"),
        signal
      }), "f");
      return __classPrivateFieldGet(this, _BetaToolRunner_toolResponse, "f");
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/decoders/jsonl.mjs
var JSONLDecoder;
var init_jsonl = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/decoders/jsonl.mjs"() {
    init_error();
    init_shims();
    init_line();
    JSONLDecoder = class _JSONLDecoder {
      constructor(iterator, controller) {
        this.iterator = iterator;
        this.controller = controller;
      }
      async *decoder() {
        const lineDecoder = new LineDecoder();
        for await (const chunk of this.iterator) {
          for (const line of lineDecoder.decode(chunk)) {
            yield JSON.parse(line);
          }
        }
        for (const line of lineDecoder.flush()) {
          yield JSON.parse(line);
        }
      }
      [Symbol.asyncIterator]() {
        return this.decoder();
      }
      static fromResponse(response, controller) {
        if (!response.body) {
          controller.abort();
          if (typeof globalThis.navigator !== "undefined" && globalThis.navigator.product === "ReactNative") {
            throw new AnthropicError(`The default react-native fetch implementation does not support streaming. Please use expo/fetch: https://docs.expo.dev/versions/latest/sdk/expo/#expofetch-api`);
          }
          throw new AnthropicError(`Attempted to iterate over a response with no body`);
        }
        return new _JSONLDecoder(ReadableStreamToAsyncIterable(response.body), controller);
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/messages/batches.mjs
var Batches;
var init_batches = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/messages/batches.mjs"() {
    init_resource();
    init_pagination();
    init_headers();
    init_jsonl();
    init_error2();
    init_path();
    Batches = class extends APIResource {
      /**
       * Send a batch of Message creation requests.
       *
       * The Message Batches API can be used to process multiple Messages API requests at
       * once. Once a Message Batch is created, it begins processing immediately. Batches
       * can take up to 24 hours to complete.
       *
       * Learn more about the Message Batches API in our
       * [user guide](https://docs.claude.com/en/docs/build-with-claude/batch-processing)
       *
       * @example
       * ```ts
       * const betaMessageBatch =
       *   await client.beta.messages.batches.create({
       *     requests: [
       *       {
       *         custom_id: 'my-custom-id-1',
       *         params: {
       *           max_tokens: 1024,
       *           messages: [
       *             { content: 'Hello, world', role: 'user' },
       *           ],
       *           model: 'claude-opus-4-6',
       *         },
       *       },
       *     ],
       *   });
       * ```
       */
      create(params, options) {
        const { betas, ...body } = params;
        return this._client.post("/v1/messages/batches?beta=true", {
          body,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "message-batches-2024-09-24"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * This endpoint is idempotent and can be used to poll for Message Batch
       * completion. To access the results of a Message Batch, make a request to the
       * `results_url` field in the response.
       *
       * Learn more about the Message Batches API in our
       * [user guide](https://docs.claude.com/en/docs/build-with-claude/batch-processing)
       *
       * @example
       * ```ts
       * const betaMessageBatch =
       *   await client.beta.messages.batches.retrieve(
       *     'message_batch_id',
       *   );
       * ```
       */
      retrieve(messageBatchID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.get(path3`/v1/messages/batches/${messageBatchID}?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "message-batches-2024-09-24"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * List all Message Batches within a Workspace. Most recently created batches are
       * returned first.
       *
       * Learn more about the Message Batches API in our
       * [user guide](https://docs.claude.com/en/docs/build-with-claude/batch-processing)
       *
       * @example
       * ```ts
       * // Automatically fetches more pages as needed.
       * for await (const betaMessageBatch of client.beta.messages.batches.list()) {
       *   // ...
       * }
       * ```
       */
      list(params = {}, options) {
        const { betas, ...query } = params ?? {};
        return this._client.getAPIList("/v1/messages/batches?beta=true", Page, {
          query,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "message-batches-2024-09-24"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Delete a Message Batch.
       *
       * Message Batches can only be deleted once they've finished processing. If you'd
       * like to delete an in-progress batch, you must first cancel it.
       *
       * Learn more about the Message Batches API in our
       * [user guide](https://docs.claude.com/en/docs/build-with-claude/batch-processing)
       *
       * @example
       * ```ts
       * const betaDeletedMessageBatch =
       *   await client.beta.messages.batches.delete(
       *     'message_batch_id',
       *   );
       * ```
       */
      delete(messageBatchID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.delete(path3`/v1/messages/batches/${messageBatchID}?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "message-batches-2024-09-24"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Batches may be canceled any time before processing ends. Once cancellation is
       * initiated, the batch enters a `canceling` state, at which time the system may
       * complete any in-progress, non-interruptible requests before finalizing
       * cancellation.
       *
       * The number of canceled requests is specified in `request_counts`. To determine
       * which requests were canceled, check the individual results within the batch.
       * Note that cancellation may not result in any canceled requests if they were
       * non-interruptible.
       *
       * Learn more about the Message Batches API in our
       * [user guide](https://docs.claude.com/en/docs/build-with-claude/batch-processing)
       *
       * @example
       * ```ts
       * const betaMessageBatch =
       *   await client.beta.messages.batches.cancel(
       *     'message_batch_id',
       *   );
       * ```
       */
      cancel(messageBatchID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.post(path3`/v1/messages/batches/${messageBatchID}/cancel?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "message-batches-2024-09-24"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Streams the results of a Message Batch as a `.jsonl` file.
       *
       * Each line in the file is a JSON object containing the result of a single request
       * in the Message Batch. Results are not guaranteed to be in the same order as
       * requests. Use the `custom_id` field to match results to requests.
       *
       * Learn more about the Message Batches API in our
       * [user guide](https://docs.claude.com/en/docs/build-with-claude/batch-processing)
       *
       * @example
       * ```ts
       * const betaMessageBatchIndividualResponse =
       *   await client.beta.messages.batches.results(
       *     'message_batch_id',
       *   );
       * ```
       */
      async results(messageBatchID, params = {}, options) {
        const batch = await this.retrieve(messageBatchID);
        if (!batch.results_url) {
          throw new AnthropicError(`No batch \`results_url\`; Has it finished processing? ${batch.processing_status} - ${batch.id}`);
        }
        const { betas } = params ?? {};
        return this._client.get(batch.results_url, {
          ...options,
          headers: buildHeaders([
            {
              "anthropic-beta": [...betas ?? [], "message-batches-2024-09-24"].toString(),
              Accept: "application/binary"
            },
            options?.headers
          ]),
          stream: true,
          __binaryResponse: true
        })._thenUnwrap((_, props) => JSONLDecoder.fromResponse(props.response, props.controller));
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/messages/messages.mjs
function transformOutputFormat(params) {
  if (!params.output_format) {
    return params;
  }
  if (params.output_config?.format) {
    throw new AnthropicError("Both output_format and output_config.format were provided. Please use only output_config.format (output_format is deprecated).");
  }
  const { output_format, ...rest } = params;
  return {
    ...rest,
    output_config: {
      ...params.output_config,
      format: output_format
    }
  };
}
var DEPRECATED_MODELS, MODELS_TO_WARN_WITH_THINKING_ENABLED, Messages;
var init_messages = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/messages/messages.mjs"() {
    init_error2();
    init_resource();
    init_constants();
    init_headers();
    init_stainless_helper_header();
    init_beta_parser();
    init_BetaMessageStream();
    init_BetaToolRunner();
    init_ToolError();
    init_batches();
    init_batches();
    init_BetaToolRunner();
    init_ToolError();
    DEPRECATED_MODELS = {
      "claude-1.3": "November 6th, 2024",
      "claude-1.3-100k": "November 6th, 2024",
      "claude-instant-1.1": "November 6th, 2024",
      "claude-instant-1.1-100k": "November 6th, 2024",
      "claude-instant-1.2": "November 6th, 2024",
      "claude-3-sonnet-20240229": "July 21st, 2025",
      "claude-3-opus-20240229": "January 5th, 2026",
      "claude-2.1": "July 21st, 2025",
      "claude-2.0": "July 21st, 2025",
      "claude-3-7-sonnet-latest": "February 19th, 2026",
      "claude-3-7-sonnet-20250219": "February 19th, 2026"
    };
    MODELS_TO_WARN_WITH_THINKING_ENABLED = ["claude-mythos-preview", "claude-opus-4-6"];
    Messages = class extends APIResource {
      constructor() {
        super(...arguments);
        this.batches = new Batches(this._client);
      }
      create(params, options) {
        const modifiedParams = transformOutputFormat(params);
        const { betas, ...body } = modifiedParams;
        if (body.model in DEPRECATED_MODELS) {
          console.warn(`The model '${body.model}' is deprecated and will reach end-of-life on ${DEPRECATED_MODELS[body.model]}
Please migrate to a newer model. Visit https://docs.anthropic.com/en/docs/resources/model-deprecations for more information.`);
        }
        if (MODELS_TO_WARN_WITH_THINKING_ENABLED.includes(body.model) && body.thinking && body.thinking.type === "enabled") {
          console.warn(`Using Claude with ${body.model} and 'thinking.type=enabled' is deprecated. Use 'thinking.type=adaptive' instead which results in better model performance in our testing: https://platform.claude.com/docs/en/build-with-claude/adaptive-thinking`);
        }
        let timeout = this._client._options.timeout;
        if (!body.stream && timeout == null) {
          const maxNonstreamingTokens = MODEL_NONSTREAMING_TOKENS[body.model] ?? void 0;
          timeout = this._client.calculateNonstreamingTimeout(body.max_tokens, maxNonstreamingTokens);
        }
        const helperHeader = stainlessHelperHeader(body.tools, body.messages);
        return this._client.post("/v1/messages?beta=true", {
          body,
          timeout: timeout ?? 6e5,
          ...options,
          headers: buildHeaders([
            { ...betas?.toString() != null ? { "anthropic-beta": betas?.toString() } : void 0 },
            helperHeader,
            options?.headers
          ]),
          stream: modifiedParams.stream ?? false
        });
      }
      /**
       * Send a structured list of input messages with text and/or image content, along with an expected `output_format` and
       * the response will be automatically parsed and available in the `parsed_output` property of the message.
       *
       * @example
       * ```ts
       * const message = await client.beta.messages.parse({
       *   model: 'claude-3-5-sonnet-20241022',
       *   max_tokens: 1024,
       *   messages: [{ role: 'user', content: 'What is 2+2?' }],
       *   output_format: zodOutputFormat(z.object({ answer: z.number() }), 'math'),
       * });
       *
       * console.log(message.parsed_output?.answer); // 4
       * ```
       */
      parse(params, options) {
        options = {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...params.betas ?? [], "structured-outputs-2025-12-15"].toString() },
            options?.headers
          ])
        };
        return this.create(params, options).then((message) => parseBetaMessage(message, params, { logger: this._client.logger ?? console }));
      }
      /**
       * Create a Message stream
       */
      stream(body, options) {
        return BetaMessageStream.createMessage(this, body, options);
      }
      /**
       * Count the number of tokens in a Message.
       *
       * The Token Count API can be used to count the number of tokens in a Message,
       * including tools, images, and documents, without creating it.
       *
       * Learn more about token counting in our
       * [user guide](https://docs.claude.com/en/docs/build-with-claude/token-counting)
       *
       * @example
       * ```ts
       * const betaMessageTokensCount =
       *   await client.beta.messages.countTokens({
       *     messages: [{ content: 'Hello, world', role: 'user' }],
       *     model: 'claude-opus-4-6',
       *   });
       * ```
       */
      countTokens(params, options) {
        const modifiedParams = transformOutputFormat(params);
        const { betas, ...body } = modifiedParams;
        return this._client.post("/v1/messages/count_tokens?beta=true", {
          body,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "token-counting-2024-11-01"].toString() },
            options?.headers
          ])
        });
      }
      toolRunner(body, options) {
        return new BetaToolRunner(this._client, body, options);
      }
    };
    Messages.Batches = Batches;
    Messages.BetaToolRunner = BetaToolRunner;
    Messages.ToolError = ToolError;
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/sessions/events.mjs
var Events;
var init_events = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/sessions/events.mjs"() {
    init_resource();
    init_pagination();
    init_headers();
    init_path();
    Events = class extends APIResource {
      /**
       * List Events
       *
       * @example
       * ```ts
       * // Automatically fetches more pages as needed.
       * for await (const betaManagedAgentsSessionEvent of client.beta.sessions.events.list(
       *   'sesn_011CZkZAtmR3yMPDzynEDxu7',
       * )) {
       *   // ...
       * }
       * ```
       */
      list(sessionID, params = {}, options) {
        const { betas, ...query } = params ?? {};
        return this._client.getAPIList(path3`/v1/sessions/${sessionID}/events?beta=true`, PageCursor, {
          query,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Send Events
       *
       * @example
       * ```ts
       * const betaManagedAgentsSendSessionEvents =
       *   await client.beta.sessions.events.send(
       *     'sesn_011CZkZAtmR3yMPDzynEDxu7',
       *     {
       *       events: [
       *         {
       *           content: [
       *             {
       *               text: 'Where is my order #1234?',
       *               type: 'text',
       *             },
       *           ],
       *           type: 'user.message',
       *         },
       *       ],
       *     },
       *   );
       * ```
       */
      send(sessionID, params, options) {
        const { betas, ...body } = params;
        return this._client.post(path3`/v1/sessions/${sessionID}/events?beta=true`, {
          body,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Stream Events
       *
       * @example
       * ```ts
       * const betaManagedAgentsStreamSessionEvents =
       *   await client.beta.sessions.events.stream(
       *     'sesn_011CZkZAtmR3yMPDzynEDxu7',
       *   );
       * ```
       */
      stream(sessionID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.get(path3`/v1/sessions/${sessionID}/events/stream?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ]),
          stream: true
        });
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/sessions/resources.mjs
var Resources;
var init_resources = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/sessions/resources.mjs"() {
    init_resource();
    init_pagination();
    init_headers();
    init_path();
    Resources = class extends APIResource {
      /**
       * Get Session Resource
       *
       * @example
       * ```ts
       * const resource =
       *   await client.beta.sessions.resources.retrieve(
       *     'sesrsc_011CZkZBJq5dWxk9fVLNcPht',
       *     { session_id: 'sesn_011CZkZAtmR3yMPDzynEDxu7' },
       *   );
       * ```
       */
      retrieve(resourceID, params, options) {
        const { session_id, betas } = params;
        return this._client.get(path3`/v1/sessions/${session_id}/resources/${resourceID}?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Update Session Resource
       *
       * @example
       * ```ts
       * const resource =
       *   await client.beta.sessions.resources.update(
       *     'sesrsc_011CZkZBJq5dWxk9fVLNcPht',
       *     {
       *       session_id: 'sesn_011CZkZAtmR3yMPDzynEDxu7',
       *       authorization_token: 'ghp_exampletoken',
       *     },
       *   );
       * ```
       */
      update(resourceID, params, options) {
        const { session_id, betas, ...body } = params;
        return this._client.post(path3`/v1/sessions/${session_id}/resources/${resourceID}?beta=true`, {
          body,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * List Session Resources
       *
       * @example
       * ```ts
       * // Automatically fetches more pages as needed.
       * for await (const betaManagedAgentsSessionResource of client.beta.sessions.resources.list(
       *   'sesn_011CZkZAtmR3yMPDzynEDxu7',
       * )) {
       *   // ...
       * }
       * ```
       */
      list(sessionID, params = {}, options) {
        const { betas, ...query } = params ?? {};
        return this._client.getAPIList(path3`/v1/sessions/${sessionID}/resources?beta=true`, PageCursor, {
          query,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Delete Session Resource
       *
       * @example
       * ```ts
       * const betaManagedAgentsDeleteSessionResource =
       *   await client.beta.sessions.resources.delete(
       *     'sesrsc_011CZkZBJq5dWxk9fVLNcPht',
       *     { session_id: 'sesn_011CZkZAtmR3yMPDzynEDxu7' },
       *   );
       * ```
       */
      delete(resourceID, params, options) {
        const { session_id, betas } = params;
        return this._client.delete(path3`/v1/sessions/${session_id}/resources/${resourceID}?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Add Session Resource
       *
       * @example
       * ```ts
       * const betaManagedAgentsFileResource =
       *   await client.beta.sessions.resources.add(
       *     'sesn_011CZkZAtmR3yMPDzynEDxu7',
       *     {
       *       file_id: 'file_011CNha8iCJcU1wXNR6q4V8w',
       *       type: 'file',
       *     },
       *   );
       * ```
       */
      add(sessionID, params, options) {
        const { betas, ...body } = params;
        return this._client.post(path3`/v1/sessions/${sessionID}/resources?beta=true`, {
          body,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/sessions/sessions.mjs
var Sessions;
var init_sessions = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/sessions/sessions.mjs"() {
    init_resource();
    init_events();
    init_events();
    init_resources();
    init_resources();
    init_pagination();
    init_headers();
    init_path();
    Sessions = class extends APIResource {
      constructor() {
        super(...arguments);
        this.events = new Events(this._client);
        this.resources = new Resources(this._client);
      }
      /**
       * Create Session
       *
       * @example
       * ```ts
       * const betaManagedAgentsSession =
       *   await client.beta.sessions.create({
       *     agent: 'agent_011CZkYpogX7uDKUyvBTophP',
       *     environment_id: 'env_011CZkZ9X2dpNyB7HsEFoRfW',
       *   });
       * ```
       */
      create(params, options) {
        const { betas, ...body } = params;
        return this._client.post("/v1/sessions?beta=true", {
          body,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Get Session
       *
       * @example
       * ```ts
       * const betaManagedAgentsSession =
       *   await client.beta.sessions.retrieve(
       *     'sesn_011CZkZAtmR3yMPDzynEDxu7',
       *   );
       * ```
       */
      retrieve(sessionID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.get(path3`/v1/sessions/${sessionID}?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Update Session
       *
       * @example
       * ```ts
       * const betaManagedAgentsSession =
       *   await client.beta.sessions.update(
       *     'sesn_011CZkZAtmR3yMPDzynEDxu7',
       *   );
       * ```
       */
      update(sessionID, params, options) {
        const { betas, ...body } = params;
        return this._client.post(path3`/v1/sessions/${sessionID}?beta=true`, {
          body,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * List Sessions
       *
       * @example
       * ```ts
       * // Automatically fetches more pages as needed.
       * for await (const betaManagedAgentsSession of client.beta.sessions.list()) {
       *   // ...
       * }
       * ```
       */
      list(params = {}, options) {
        const { betas, ...query } = params ?? {};
        return this._client.getAPIList("/v1/sessions?beta=true", PageCursor, {
          query,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Delete Session
       *
       * @example
       * ```ts
       * const betaManagedAgentsDeletedSession =
       *   await client.beta.sessions.delete(
       *     'sesn_011CZkZAtmR3yMPDzynEDxu7',
       *   );
       * ```
       */
      delete(sessionID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.delete(path3`/v1/sessions/${sessionID}?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Archive Session
       *
       * @example
       * ```ts
       * const betaManagedAgentsSession =
       *   await client.beta.sessions.archive(
       *     'sesn_011CZkZAtmR3yMPDzynEDxu7',
       *   );
       * ```
       */
      archive(sessionID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.post(path3`/v1/sessions/${sessionID}/archive?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
    };
    Sessions.Events = Events;
    Sessions.Resources = Resources;
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/skills/versions.mjs
var Versions2;
var init_versions2 = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/skills/versions.mjs"() {
    init_resource();
    init_pagination();
    init_headers();
    init_uploads();
    init_path();
    Versions2 = class extends APIResource {
      /**
       * Create Skill Version
       *
       * @example
       * ```ts
       * const version = await client.beta.skills.versions.create(
       *   'skill_id',
       * );
       * ```
       */
      create(skillID, params = {}, options) {
        const { betas, ...body } = params ?? {};
        return this._client.post(path3`/v1/skills/${skillID}/versions?beta=true`, multipartFormRequestOptions({
          body,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "skills-2025-10-02"].toString() },
            options?.headers
          ])
        }, this._client));
      }
      /**
       * Get Skill Version
       *
       * @example
       * ```ts
       * const version = await client.beta.skills.versions.retrieve(
       *   'version',
       *   { skill_id: 'skill_id' },
       * );
       * ```
       */
      retrieve(version, params, options) {
        const { skill_id, betas } = params;
        return this._client.get(path3`/v1/skills/${skill_id}/versions/${version}?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "skills-2025-10-02"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * List Skill Versions
       *
       * @example
       * ```ts
       * // Automatically fetches more pages as needed.
       * for await (const versionListResponse of client.beta.skills.versions.list(
       *   'skill_id',
       * )) {
       *   // ...
       * }
       * ```
       */
      list(skillID, params = {}, options) {
        const { betas, ...query } = params ?? {};
        return this._client.getAPIList(path3`/v1/skills/${skillID}/versions?beta=true`, PageCursor, {
          query,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "skills-2025-10-02"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Delete Skill Version
       *
       * @example
       * ```ts
       * const version = await client.beta.skills.versions.delete(
       *   'version',
       *   { skill_id: 'skill_id' },
       * );
       * ```
       */
      delete(version, params, options) {
        const { skill_id, betas } = params;
        return this._client.delete(path3`/v1/skills/${skill_id}/versions/${version}?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "skills-2025-10-02"].toString() },
            options?.headers
          ])
        });
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/skills/skills.mjs
var Skills;
var init_skills = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/skills/skills.mjs"() {
    init_resource();
    init_versions2();
    init_versions2();
    init_pagination();
    init_headers();
    init_uploads();
    init_path();
    Skills = class extends APIResource {
      constructor() {
        super(...arguments);
        this.versions = new Versions2(this._client);
      }
      /**
       * Create Skill
       *
       * @example
       * ```ts
       * const skill = await client.beta.skills.create();
       * ```
       */
      create(params = {}, options) {
        const { betas, ...body } = params ?? {};
        return this._client.post("/v1/skills?beta=true", multipartFormRequestOptions({
          body,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "skills-2025-10-02"].toString() },
            options?.headers
          ])
        }, this._client, false));
      }
      /**
       * Get Skill
       *
       * @example
       * ```ts
       * const skill = await client.beta.skills.retrieve('skill_id');
       * ```
       */
      retrieve(skillID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.get(path3`/v1/skills/${skillID}?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "skills-2025-10-02"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * List Skills
       *
       * @example
       * ```ts
       * // Automatically fetches more pages as needed.
       * for await (const skillListResponse of client.beta.skills.list()) {
       *   // ...
       * }
       * ```
       */
      list(params = {}, options) {
        const { betas, ...query } = params ?? {};
        return this._client.getAPIList("/v1/skills?beta=true", PageCursor, {
          query,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "skills-2025-10-02"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Delete Skill
       *
       * @example
       * ```ts
       * const skill = await client.beta.skills.delete('skill_id');
       * ```
       */
      delete(skillID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.delete(path3`/v1/skills/${skillID}?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "skills-2025-10-02"].toString() },
            options?.headers
          ])
        });
      }
    };
    Skills.Versions = Versions2;
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/vaults/credentials.mjs
var Credentials;
var init_credentials = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/vaults/credentials.mjs"() {
    init_resource();
    init_pagination();
    init_headers();
    init_path();
    Credentials = class extends APIResource {
      /**
       * Create Credential
       *
       * @example
       * ```ts
       * const betaManagedAgentsCredential =
       *   await client.beta.vaults.credentials.create(
       *     'vlt_011CZkZDLs7fYzm1hXNPeRjv',
       *     {
       *       auth: {
       *         token: 'bearer_exampletoken',
       *         mcp_server_url:
       *           'https://example-server.modelcontextprotocol.io/sse',
       *         type: 'static_bearer',
       *       },
       *     },
       *   );
       * ```
       */
      create(vaultID, params, options) {
        const { betas, ...body } = params;
        return this._client.post(path3`/v1/vaults/${vaultID}/credentials?beta=true`, {
          body,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Get Credential
       *
       * @example
       * ```ts
       * const betaManagedAgentsCredential =
       *   await client.beta.vaults.credentials.retrieve(
       *     'vcrd_011CZkZEMt8gZan2iYOQfSkw',
       *     { vault_id: 'vlt_011CZkZDLs7fYzm1hXNPeRjv' },
       *   );
       * ```
       */
      retrieve(credentialID, params, options) {
        const { vault_id, betas } = params;
        return this._client.get(path3`/v1/vaults/${vault_id}/credentials/${credentialID}?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Update Credential
       *
       * @example
       * ```ts
       * const betaManagedAgentsCredential =
       *   await client.beta.vaults.credentials.update(
       *     'vcrd_011CZkZEMt8gZan2iYOQfSkw',
       *     { vault_id: 'vlt_011CZkZDLs7fYzm1hXNPeRjv' },
       *   );
       * ```
       */
      update(credentialID, params, options) {
        const { vault_id, betas, ...body } = params;
        return this._client.post(path3`/v1/vaults/${vault_id}/credentials/${credentialID}?beta=true`, {
          body,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * List Credentials
       *
       * @example
       * ```ts
       * // Automatically fetches more pages as needed.
       * for await (const betaManagedAgentsCredential of client.beta.vaults.credentials.list(
       *   'vlt_011CZkZDLs7fYzm1hXNPeRjv',
       * )) {
       *   // ...
       * }
       * ```
       */
      list(vaultID, params = {}, options) {
        const { betas, ...query } = params ?? {};
        return this._client.getAPIList(path3`/v1/vaults/${vaultID}/credentials?beta=true`, PageCursor, {
          query,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Delete Credential
       *
       * @example
       * ```ts
       * const betaManagedAgentsDeletedCredential =
       *   await client.beta.vaults.credentials.delete(
       *     'vcrd_011CZkZEMt8gZan2iYOQfSkw',
       *     { vault_id: 'vlt_011CZkZDLs7fYzm1hXNPeRjv' },
       *   );
       * ```
       */
      delete(credentialID, params, options) {
        const { vault_id, betas } = params;
        return this._client.delete(path3`/v1/vaults/${vault_id}/credentials/${credentialID}?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Archive Credential
       *
       * @example
       * ```ts
       * const betaManagedAgentsCredential =
       *   await client.beta.vaults.credentials.archive(
       *     'vcrd_011CZkZEMt8gZan2iYOQfSkw',
       *     { vault_id: 'vlt_011CZkZDLs7fYzm1hXNPeRjv' },
       *   );
       * ```
       */
      archive(credentialID, params, options) {
        const { vault_id, betas } = params;
        return this._client.post(path3`/v1/vaults/${vault_id}/credentials/${credentialID}/archive?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/vaults/vaults.mjs
var Vaults;
var init_vaults = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/vaults/vaults.mjs"() {
    init_resource();
    init_credentials();
    init_credentials();
    init_pagination();
    init_headers();
    init_path();
    Vaults = class extends APIResource {
      constructor() {
        super(...arguments);
        this.credentials = new Credentials(this._client);
      }
      /**
       * Create Vault
       *
       * @example
       * ```ts
       * const betaManagedAgentsVault =
       *   await client.beta.vaults.create({
       *     display_name: 'Example vault',
       *   });
       * ```
       */
      create(params, options) {
        const { betas, ...body } = params;
        return this._client.post("/v1/vaults?beta=true", {
          body,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Get Vault
       *
       * @example
       * ```ts
       * const betaManagedAgentsVault =
       *   await client.beta.vaults.retrieve(
       *     'vlt_011CZkZDLs7fYzm1hXNPeRjv',
       *   );
       * ```
       */
      retrieve(vaultID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.get(path3`/v1/vaults/${vaultID}?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Update Vault
       *
       * @example
       * ```ts
       * const betaManagedAgentsVault =
       *   await client.beta.vaults.update(
       *     'vlt_011CZkZDLs7fYzm1hXNPeRjv',
       *   );
       * ```
       */
      update(vaultID, params, options) {
        const { betas, ...body } = params;
        return this._client.post(path3`/v1/vaults/${vaultID}?beta=true`, {
          body,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * List Vaults
       *
       * @example
       * ```ts
       * // Automatically fetches more pages as needed.
       * for await (const betaManagedAgentsVault of client.beta.vaults.list()) {
       *   // ...
       * }
       * ```
       */
      list(params = {}, options) {
        const { betas, ...query } = params ?? {};
        return this._client.getAPIList("/v1/vaults?beta=true", PageCursor, {
          query,
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Delete Vault
       *
       * @example
       * ```ts
       * const betaManagedAgentsDeletedVault =
       *   await client.beta.vaults.delete(
       *     'vlt_011CZkZDLs7fYzm1hXNPeRjv',
       *   );
       * ```
       */
      delete(vaultID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.delete(path3`/v1/vaults/${vaultID}?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
      /**
       * Archive Vault
       *
       * @example
       * ```ts
       * const betaManagedAgentsVault =
       *   await client.beta.vaults.archive(
       *     'vlt_011CZkZDLs7fYzm1hXNPeRjv',
       *   );
       * ```
       */
      archive(vaultID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.post(path3`/v1/vaults/${vaultID}/archive?beta=true`, {
          ...options,
          headers: buildHeaders([
            { "anthropic-beta": [...betas ?? [], "managed-agents-2026-04-01"].toString() },
            options?.headers
          ])
        });
      }
    };
    Vaults.Credentials = Credentials;
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/beta.mjs
var Beta;
var init_beta = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/beta/beta.mjs"() {
    init_resource();
    init_environments();
    init_environments();
    init_files();
    init_files();
    init_models();
    init_models();
    init_user_profiles();
    init_user_profiles();
    init_agents();
    init_agents();
    init_messages();
    init_messages();
    init_sessions();
    init_sessions();
    init_skills();
    init_skills();
    init_vaults();
    init_vaults();
    Beta = class extends APIResource {
      constructor() {
        super(...arguments);
        this.models = new Models(this._client);
        this.messages = new Messages(this._client);
        this.agents = new Agents(this._client);
        this.environments = new Environments(this._client);
        this.sessions = new Sessions(this._client);
        this.vaults = new Vaults(this._client);
        this.files = new Files(this._client);
        this.skills = new Skills(this._client);
        this.userProfiles = new UserProfiles(this._client);
      }
    };
    Beta.Models = Models;
    Beta.Messages = Messages;
    Beta.Agents = Agents;
    Beta.Environments = Environments;
    Beta.Sessions = Sessions;
    Beta.Vaults = Vaults;
    Beta.Files = Files;
    Beta.Skills = Skills;
    Beta.UserProfiles = UserProfiles;
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/completions.mjs
var Completions;
var init_completions = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/completions.mjs"() {
    init_resource();
    init_headers();
    Completions = class extends APIResource {
      create(params, options) {
        const { betas, ...body } = params;
        return this._client.post("/v1/complete", {
          body,
          timeout: this._client._options.timeout ?? 6e5,
          ...options,
          headers: buildHeaders([
            { ...betas?.toString() != null ? { "anthropic-beta": betas?.toString() } : void 0 },
            options?.headers
          ]),
          stream: params.stream ?? false
        });
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/lib/parser.mjs
function getOutputFormat2(params) {
  return params?.output_config?.format;
}
function maybeParseMessage(message, params, opts) {
  const outputFormat = getOutputFormat2(params);
  if (!params || !("parse" in (outputFormat ?? {}))) {
    return {
      ...message,
      content: message.content.map((block) => {
        if (block.type === "text") {
          const parsedBlock = Object.defineProperty({ ...block }, "parsed_output", {
            value: null,
            enumerable: false
          });
          return parsedBlock;
        }
        return block;
      }),
      parsed_output: null
    };
  }
  return parseMessage(message, params, opts);
}
function parseMessage(message, params, opts) {
  let firstParsedOutput = null;
  const content = message.content.map((block) => {
    if (block.type === "text") {
      const parsedOutput = parseOutputFormat(params, block.text);
      if (firstParsedOutput === null) {
        firstParsedOutput = parsedOutput;
      }
      const parsedBlock = Object.defineProperty({ ...block }, "parsed_output", {
        value: parsedOutput,
        enumerable: false
      });
      return parsedBlock;
    }
    return block;
  });
  return {
    ...message,
    content,
    parsed_output: firstParsedOutput
  };
}
function parseOutputFormat(params, content) {
  const outputFormat = getOutputFormat2(params);
  if (outputFormat?.type !== "json_schema") {
    return null;
  }
  try {
    if ("parse" in outputFormat) {
      return outputFormat.parse(content);
    }
    return JSON.parse(content);
  } catch (error) {
    throw new AnthropicError(`Failed to parse structured output: ${error}`);
  }
}
var init_parser2 = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/lib/parser.mjs"() {
    init_error();
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/lib/MessageStream.mjs
function tracksToolInput2(content) {
  return content.type === "tool_use" || content.type === "server_tool_use";
}
function checkNever2(x) {
}
var _MessageStream_instances, _MessageStream_currentMessageSnapshot, _MessageStream_params, _MessageStream_connectedPromise, _MessageStream_resolveConnectedPromise, _MessageStream_rejectConnectedPromise, _MessageStream_endPromise, _MessageStream_resolveEndPromise, _MessageStream_rejectEndPromise, _MessageStream_listeners, _MessageStream_ended, _MessageStream_errored, _MessageStream_aborted, _MessageStream_catchingPromiseCreated, _MessageStream_response, _MessageStream_request_id, _MessageStream_logger, _MessageStream_getFinalMessage, _MessageStream_getFinalText, _MessageStream_handleError, _MessageStream_beginRequest, _MessageStream_addStreamEvent, _MessageStream_endRequest, _MessageStream_accumulateMessage, JSON_BUF_PROPERTY2, MessageStream;
var init_MessageStream = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/lib/MessageStream.mjs"() {
    init_tslib();
    init_errors();
    init_error2();
    init_streaming2();
    init_parser();
    init_parser2();
    JSON_BUF_PROPERTY2 = "__json_buf";
    MessageStream = class _MessageStream {
      constructor(params, opts) {
        _MessageStream_instances.add(this);
        this.messages = [];
        this.receivedMessages = [];
        _MessageStream_currentMessageSnapshot.set(this, void 0);
        _MessageStream_params.set(this, null);
        this.controller = new AbortController();
        _MessageStream_connectedPromise.set(this, void 0);
        _MessageStream_resolveConnectedPromise.set(this, () => {
        });
        _MessageStream_rejectConnectedPromise.set(this, () => {
        });
        _MessageStream_endPromise.set(this, void 0);
        _MessageStream_resolveEndPromise.set(this, () => {
        });
        _MessageStream_rejectEndPromise.set(this, () => {
        });
        _MessageStream_listeners.set(this, {});
        _MessageStream_ended.set(this, false);
        _MessageStream_errored.set(this, false);
        _MessageStream_aborted.set(this, false);
        _MessageStream_catchingPromiseCreated.set(this, false);
        _MessageStream_response.set(this, void 0);
        _MessageStream_request_id.set(this, void 0);
        _MessageStream_logger.set(this, void 0);
        _MessageStream_handleError.set(this, (error) => {
          __classPrivateFieldSet(this, _MessageStream_errored, true, "f");
          if (isAbortError(error)) {
            error = new APIUserAbortError();
          }
          if (error instanceof APIUserAbortError) {
            __classPrivateFieldSet(this, _MessageStream_aborted, true, "f");
            return this._emit("abort", error);
          }
          if (error instanceof AnthropicError) {
            return this._emit("error", error);
          }
          if (error instanceof Error) {
            const anthropicError = new AnthropicError(error.message);
            anthropicError.cause = error;
            return this._emit("error", anthropicError);
          }
          return this._emit("error", new AnthropicError(String(error)));
        });
        __classPrivateFieldSet(this, _MessageStream_connectedPromise, new Promise((resolve4, reject) => {
          __classPrivateFieldSet(this, _MessageStream_resolveConnectedPromise, resolve4, "f");
          __classPrivateFieldSet(this, _MessageStream_rejectConnectedPromise, reject, "f");
        }), "f");
        __classPrivateFieldSet(this, _MessageStream_endPromise, new Promise((resolve4, reject) => {
          __classPrivateFieldSet(this, _MessageStream_resolveEndPromise, resolve4, "f");
          __classPrivateFieldSet(this, _MessageStream_rejectEndPromise, reject, "f");
        }), "f");
        __classPrivateFieldGet(this, _MessageStream_connectedPromise, "f").catch(() => {
        });
        __classPrivateFieldGet(this, _MessageStream_endPromise, "f").catch(() => {
        });
        __classPrivateFieldSet(this, _MessageStream_params, params, "f");
        __classPrivateFieldSet(this, _MessageStream_logger, opts?.logger ?? console, "f");
      }
      get response() {
        return __classPrivateFieldGet(this, _MessageStream_response, "f");
      }
      get request_id() {
        return __classPrivateFieldGet(this, _MessageStream_request_id, "f");
      }
      /**
       * Returns the `MessageStream` data, the raw `Response` instance and the ID of the request,
       * returned vie the `request-id` header which is useful for debugging requests and resporting
       * issues to Anthropic.
       *
       * This is the same as the `APIPromise.withResponse()` method.
       *
       * This method will raise an error if you created the stream using `MessageStream.fromReadableStream`
       * as no `Response` is available.
       */
      async withResponse() {
        __classPrivateFieldSet(this, _MessageStream_catchingPromiseCreated, true, "f");
        const response = await __classPrivateFieldGet(this, _MessageStream_connectedPromise, "f");
        if (!response) {
          throw new Error("Could not resolve a `Response` object");
        }
        return {
          data: this,
          response,
          request_id: response.headers.get("request-id")
        };
      }
      /**
       * Intended for use on the frontend, consuming a stream produced with
       * `.toReadableStream()` on the backend.
       *
       * Note that messages sent to the model do not appear in `.on('message')`
       * in this context.
       */
      static fromReadableStream(stream) {
        const runner = new _MessageStream(null);
        runner._run(() => runner._fromReadableStream(stream));
        return runner;
      }
      static createMessage(messages, params, options, { logger } = {}) {
        const runner = new _MessageStream(params, { logger });
        for (const message of params.messages) {
          runner._addMessageParam(message);
        }
        __classPrivateFieldSet(runner, _MessageStream_params, { ...params, stream: true }, "f");
        runner._run(() => runner._createMessage(messages, { ...params, stream: true }, { ...options, headers: { ...options?.headers, "X-Stainless-Helper-Method": "stream" } }));
        return runner;
      }
      _run(executor) {
        executor().then(() => {
          this._emitFinal();
          this._emit("end");
        }, __classPrivateFieldGet(this, _MessageStream_handleError, "f"));
      }
      _addMessageParam(message) {
        this.messages.push(message);
      }
      _addMessage(message, emit = true) {
        this.receivedMessages.push(message);
        if (emit) {
          this._emit("message", message);
        }
      }
      async _createMessage(messages, params, options) {
        const signal = options?.signal;
        let abortHandler;
        if (signal) {
          if (signal.aborted)
            this.controller.abort();
          abortHandler = this.controller.abort.bind(this.controller);
          signal.addEventListener("abort", abortHandler);
        }
        try {
          __classPrivateFieldGet(this, _MessageStream_instances, "m", _MessageStream_beginRequest).call(this);
          const { response, data: stream } = await messages.create({ ...params, stream: true }, { ...options, signal: this.controller.signal }).withResponse();
          this._connected(response);
          for await (const event of stream) {
            __classPrivateFieldGet(this, _MessageStream_instances, "m", _MessageStream_addStreamEvent).call(this, event);
          }
          if (stream.controller.signal?.aborted) {
            throw new APIUserAbortError();
          }
          __classPrivateFieldGet(this, _MessageStream_instances, "m", _MessageStream_endRequest).call(this);
        } finally {
          if (signal && abortHandler) {
            signal.removeEventListener("abort", abortHandler);
          }
        }
      }
      _connected(response) {
        if (this.ended)
          return;
        __classPrivateFieldSet(this, _MessageStream_response, response, "f");
        __classPrivateFieldSet(this, _MessageStream_request_id, response?.headers.get("request-id"), "f");
        __classPrivateFieldGet(this, _MessageStream_resolveConnectedPromise, "f").call(this, response);
        this._emit("connect");
      }
      get ended() {
        return __classPrivateFieldGet(this, _MessageStream_ended, "f");
      }
      get errored() {
        return __classPrivateFieldGet(this, _MessageStream_errored, "f");
      }
      get aborted() {
        return __classPrivateFieldGet(this, _MessageStream_aborted, "f");
      }
      abort() {
        this.controller.abort();
      }
      /**
       * Adds the listener function to the end of the listeners array for the event.
       * No checks are made to see if the listener has already been added. Multiple calls passing
       * the same combination of event and listener will result in the listener being added, and
       * called, multiple times.
       * @returns this MessageStream, so that calls can be chained
       */
      on(event, listener) {
        const listeners = __classPrivateFieldGet(this, _MessageStream_listeners, "f")[event] || (__classPrivateFieldGet(this, _MessageStream_listeners, "f")[event] = []);
        listeners.push({ listener });
        return this;
      }
      /**
       * Removes the specified listener from the listener array for the event.
       * off() will remove, at most, one instance of a listener from the listener array. If any single
       * listener has been added multiple times to the listener array for the specified event, then
       * off() must be called multiple times to remove each instance.
       * @returns this MessageStream, so that calls can be chained
       */
      off(event, listener) {
        const listeners = __classPrivateFieldGet(this, _MessageStream_listeners, "f")[event];
        if (!listeners)
          return this;
        const index = listeners.findIndex((l) => l.listener === listener);
        if (index >= 0)
          listeners.splice(index, 1);
        return this;
      }
      /**
       * Adds a one-time listener function for the event. The next time the event is triggered,
       * this listener is removed and then invoked.
       * @returns this MessageStream, so that calls can be chained
       */
      once(event, listener) {
        const listeners = __classPrivateFieldGet(this, _MessageStream_listeners, "f")[event] || (__classPrivateFieldGet(this, _MessageStream_listeners, "f")[event] = []);
        listeners.push({ listener, once: true });
        return this;
      }
      /**
       * This is similar to `.once()`, but returns a Promise that resolves the next time
       * the event is triggered, instead of calling a listener callback.
       * @returns a Promise that resolves the next time given event is triggered,
       * or rejects if an error is emitted.  (If you request the 'error' event,
       * returns a promise that resolves with the error).
       *
       * Example:
       *
       *   const message = await stream.emitted('message') // rejects if the stream errors
       */
      emitted(event) {
        return new Promise((resolve4, reject) => {
          __classPrivateFieldSet(this, _MessageStream_catchingPromiseCreated, true, "f");
          if (event !== "error")
            this.once("error", reject);
          this.once(event, resolve4);
        });
      }
      async done() {
        __classPrivateFieldSet(this, _MessageStream_catchingPromiseCreated, true, "f");
        await __classPrivateFieldGet(this, _MessageStream_endPromise, "f");
      }
      get currentMessage() {
        return __classPrivateFieldGet(this, _MessageStream_currentMessageSnapshot, "f");
      }
      /**
       * @returns a promise that resolves with the the final assistant Message response,
       * or rejects if an error occurred or the stream ended prematurely without producing a Message.
       * If structured outputs were used, this will be a ParsedMessage with a `parsed_output` field.
       */
      async finalMessage() {
        await this.done();
        return __classPrivateFieldGet(this, _MessageStream_instances, "m", _MessageStream_getFinalMessage).call(this);
      }
      /**
       * @returns a promise that resolves with the the final assistant Message's text response, concatenated
       * together if there are more than one text blocks.
       * Rejects if an error occurred or the stream ended prematurely without producing a Message.
       */
      async finalText() {
        await this.done();
        return __classPrivateFieldGet(this, _MessageStream_instances, "m", _MessageStream_getFinalText).call(this);
      }
      _emit(event, ...args) {
        if (__classPrivateFieldGet(this, _MessageStream_ended, "f"))
          return;
        if (event === "end") {
          __classPrivateFieldSet(this, _MessageStream_ended, true, "f");
          __classPrivateFieldGet(this, _MessageStream_resolveEndPromise, "f").call(this);
        }
        const listeners = __classPrivateFieldGet(this, _MessageStream_listeners, "f")[event];
        if (listeners) {
          __classPrivateFieldGet(this, _MessageStream_listeners, "f")[event] = listeners.filter((l) => !l.once);
          listeners.forEach(({ listener }) => listener(...args));
        }
        if (event === "abort") {
          const error = args[0];
          if (!__classPrivateFieldGet(this, _MessageStream_catchingPromiseCreated, "f") && !listeners?.length) {
            Promise.reject(error);
          }
          __classPrivateFieldGet(this, _MessageStream_rejectConnectedPromise, "f").call(this, error);
          __classPrivateFieldGet(this, _MessageStream_rejectEndPromise, "f").call(this, error);
          this._emit("end");
          return;
        }
        if (event === "error") {
          const error = args[0];
          if (!__classPrivateFieldGet(this, _MessageStream_catchingPromiseCreated, "f") && !listeners?.length) {
            Promise.reject(error);
          }
          __classPrivateFieldGet(this, _MessageStream_rejectConnectedPromise, "f").call(this, error);
          __classPrivateFieldGet(this, _MessageStream_rejectEndPromise, "f").call(this, error);
          this._emit("end");
        }
      }
      _emitFinal() {
        const finalMessage = this.receivedMessages.at(-1);
        if (finalMessage) {
          this._emit("finalMessage", __classPrivateFieldGet(this, _MessageStream_instances, "m", _MessageStream_getFinalMessage).call(this));
        }
      }
      async _fromReadableStream(readableStream, options) {
        const signal = options?.signal;
        let abortHandler;
        if (signal) {
          if (signal.aborted)
            this.controller.abort();
          abortHandler = this.controller.abort.bind(this.controller);
          signal.addEventListener("abort", abortHandler);
        }
        try {
          __classPrivateFieldGet(this, _MessageStream_instances, "m", _MessageStream_beginRequest).call(this);
          this._connected(null);
          const stream = Stream.fromReadableStream(readableStream, this.controller);
          for await (const event of stream) {
            __classPrivateFieldGet(this, _MessageStream_instances, "m", _MessageStream_addStreamEvent).call(this, event);
          }
          if (stream.controller.signal?.aborted) {
            throw new APIUserAbortError();
          }
          __classPrivateFieldGet(this, _MessageStream_instances, "m", _MessageStream_endRequest).call(this);
        } finally {
          if (signal && abortHandler) {
            signal.removeEventListener("abort", abortHandler);
          }
        }
      }
      [(_MessageStream_currentMessageSnapshot = /* @__PURE__ */ new WeakMap(), _MessageStream_params = /* @__PURE__ */ new WeakMap(), _MessageStream_connectedPromise = /* @__PURE__ */ new WeakMap(), _MessageStream_resolveConnectedPromise = /* @__PURE__ */ new WeakMap(), _MessageStream_rejectConnectedPromise = /* @__PURE__ */ new WeakMap(), _MessageStream_endPromise = /* @__PURE__ */ new WeakMap(), _MessageStream_resolveEndPromise = /* @__PURE__ */ new WeakMap(), _MessageStream_rejectEndPromise = /* @__PURE__ */ new WeakMap(), _MessageStream_listeners = /* @__PURE__ */ new WeakMap(), _MessageStream_ended = /* @__PURE__ */ new WeakMap(), _MessageStream_errored = /* @__PURE__ */ new WeakMap(), _MessageStream_aborted = /* @__PURE__ */ new WeakMap(), _MessageStream_catchingPromiseCreated = /* @__PURE__ */ new WeakMap(), _MessageStream_response = /* @__PURE__ */ new WeakMap(), _MessageStream_request_id = /* @__PURE__ */ new WeakMap(), _MessageStream_logger = /* @__PURE__ */ new WeakMap(), _MessageStream_handleError = /* @__PURE__ */ new WeakMap(), _MessageStream_instances = /* @__PURE__ */ new WeakSet(), _MessageStream_getFinalMessage = function _MessageStream_getFinalMessage2() {
        if (this.receivedMessages.length === 0) {
          throw new AnthropicError("stream ended without producing a Message with role=assistant");
        }
        return this.receivedMessages.at(-1);
      }, _MessageStream_getFinalText = function _MessageStream_getFinalText2() {
        if (this.receivedMessages.length === 0) {
          throw new AnthropicError("stream ended without producing a Message with role=assistant");
        }
        const textBlocks = this.receivedMessages.at(-1).content.filter((block) => block.type === "text").map((block) => block.text);
        if (textBlocks.length === 0) {
          throw new AnthropicError("stream ended without producing a content block with type=text");
        }
        return textBlocks.join(" ");
      }, _MessageStream_beginRequest = function _MessageStream_beginRequest2() {
        if (this.ended)
          return;
        __classPrivateFieldSet(this, _MessageStream_currentMessageSnapshot, void 0, "f");
      }, _MessageStream_addStreamEvent = function _MessageStream_addStreamEvent2(event) {
        if (this.ended)
          return;
        const messageSnapshot = __classPrivateFieldGet(this, _MessageStream_instances, "m", _MessageStream_accumulateMessage).call(this, event);
        this._emit("streamEvent", event, messageSnapshot);
        switch (event.type) {
          case "content_block_delta": {
            const content = messageSnapshot.content.at(-1);
            switch (event.delta.type) {
              case "text_delta": {
                if (content.type === "text") {
                  this._emit("text", event.delta.text, content.text || "");
                }
                break;
              }
              case "citations_delta": {
                if (content.type === "text") {
                  this._emit("citation", event.delta.citation, content.citations ?? []);
                }
                break;
              }
              case "input_json_delta": {
                if (tracksToolInput2(content) && content.input) {
                  this._emit("inputJson", event.delta.partial_json, content.input);
                }
                break;
              }
              case "thinking_delta": {
                if (content.type === "thinking") {
                  this._emit("thinking", event.delta.thinking, content.thinking);
                }
                break;
              }
              case "signature_delta": {
                if (content.type === "thinking") {
                  this._emit("signature", content.signature);
                }
                break;
              }
              default:
                checkNever2(event.delta);
            }
            break;
          }
          case "message_stop": {
            this._addMessageParam(messageSnapshot);
            this._addMessage(maybeParseMessage(messageSnapshot, __classPrivateFieldGet(this, _MessageStream_params, "f"), { logger: __classPrivateFieldGet(this, _MessageStream_logger, "f") }), true);
            break;
          }
          case "content_block_stop": {
            this._emit("contentBlock", messageSnapshot.content.at(-1));
            break;
          }
          case "message_start": {
            __classPrivateFieldSet(this, _MessageStream_currentMessageSnapshot, messageSnapshot, "f");
            break;
          }
          case "content_block_start":
          case "message_delta":
            break;
        }
      }, _MessageStream_endRequest = function _MessageStream_endRequest2() {
        if (this.ended) {
          throw new AnthropicError(`stream has ended, this shouldn't happen`);
        }
        const snapshot = __classPrivateFieldGet(this, _MessageStream_currentMessageSnapshot, "f");
        if (!snapshot) {
          throw new AnthropicError(`request ended without sending any chunks`);
        }
        __classPrivateFieldSet(this, _MessageStream_currentMessageSnapshot, void 0, "f");
        return maybeParseMessage(snapshot, __classPrivateFieldGet(this, _MessageStream_params, "f"), { logger: __classPrivateFieldGet(this, _MessageStream_logger, "f") });
      }, _MessageStream_accumulateMessage = function _MessageStream_accumulateMessage2(event) {
        let snapshot = __classPrivateFieldGet(this, _MessageStream_currentMessageSnapshot, "f");
        if (event.type === "message_start") {
          if (snapshot) {
            throw new AnthropicError(`Unexpected event order, got ${event.type} before receiving "message_stop"`);
          }
          return event.message;
        }
        if (!snapshot) {
          throw new AnthropicError(`Unexpected event order, got ${event.type} before "message_start"`);
        }
        switch (event.type) {
          case "message_stop":
            return snapshot;
          case "message_delta":
            snapshot.stop_reason = event.delta.stop_reason;
            snapshot.stop_sequence = event.delta.stop_sequence;
            snapshot.usage.output_tokens = event.usage.output_tokens;
            if (event.usage.input_tokens != null) {
              snapshot.usage.input_tokens = event.usage.input_tokens;
            }
            if (event.usage.cache_creation_input_tokens != null) {
              snapshot.usage.cache_creation_input_tokens = event.usage.cache_creation_input_tokens;
            }
            if (event.usage.cache_read_input_tokens != null) {
              snapshot.usage.cache_read_input_tokens = event.usage.cache_read_input_tokens;
            }
            if (event.usage.server_tool_use != null) {
              snapshot.usage.server_tool_use = event.usage.server_tool_use;
            }
            return snapshot;
          case "content_block_start":
            snapshot.content.push({ ...event.content_block });
            return snapshot;
          case "content_block_delta": {
            const snapshotContent = snapshot.content.at(event.index);
            switch (event.delta.type) {
              case "text_delta": {
                if (snapshotContent?.type === "text") {
                  snapshot.content[event.index] = {
                    ...snapshotContent,
                    text: (snapshotContent.text || "") + event.delta.text
                  };
                }
                break;
              }
              case "citations_delta": {
                if (snapshotContent?.type === "text") {
                  snapshot.content[event.index] = {
                    ...snapshotContent,
                    citations: [...snapshotContent.citations ?? [], event.delta.citation]
                  };
                }
                break;
              }
              case "input_json_delta": {
                if (snapshotContent && tracksToolInput2(snapshotContent)) {
                  let jsonBuf = snapshotContent[JSON_BUF_PROPERTY2] || "";
                  jsonBuf += event.delta.partial_json;
                  const newContent = { ...snapshotContent };
                  Object.defineProperty(newContent, JSON_BUF_PROPERTY2, {
                    value: jsonBuf,
                    enumerable: false,
                    writable: true
                  });
                  if (jsonBuf) {
                    newContent.input = partialParse(jsonBuf);
                  }
                  snapshot.content[event.index] = newContent;
                }
                break;
              }
              case "thinking_delta": {
                if (snapshotContent?.type === "thinking") {
                  snapshot.content[event.index] = {
                    ...snapshotContent,
                    thinking: snapshotContent.thinking + event.delta.thinking
                  };
                }
                break;
              }
              case "signature_delta": {
                if (snapshotContent?.type === "thinking") {
                  snapshot.content[event.index] = {
                    ...snapshotContent,
                    signature: event.delta.signature
                  };
                }
                break;
              }
              default:
                checkNever2(event.delta);
            }
            return snapshot;
          }
          case "content_block_stop":
            return snapshot;
        }
      }, Symbol.asyncIterator)]() {
        const pushQueue = [];
        const readQueue = [];
        let done = false;
        this.on("streamEvent", (event) => {
          const reader = readQueue.shift();
          if (reader) {
            reader.resolve(event);
          } else {
            pushQueue.push(event);
          }
        });
        this.on("end", () => {
          done = true;
          for (const reader of readQueue) {
            reader.resolve(void 0);
          }
          readQueue.length = 0;
        });
        this.on("abort", (err) => {
          done = true;
          for (const reader of readQueue) {
            reader.reject(err);
          }
          readQueue.length = 0;
        });
        this.on("error", (err) => {
          done = true;
          for (const reader of readQueue) {
            reader.reject(err);
          }
          readQueue.length = 0;
        });
        return {
          next: async () => {
            if (!pushQueue.length) {
              if (done) {
                return { value: void 0, done: true };
              }
              return new Promise((resolve4, reject) => readQueue.push({ resolve: resolve4, reject })).then((chunk2) => chunk2 ? { value: chunk2, done: false } : { value: void 0, done: true });
            }
            const chunk = pushQueue.shift();
            return { value: chunk, done: false };
          },
          return: async () => {
            this.abort();
            return { value: void 0, done: true };
          }
        };
      }
      toReadableStream() {
        const stream = new Stream(this[Symbol.asyncIterator].bind(this), this.controller);
        return stream.toReadableStream();
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/messages/batches.mjs
var Batches2;
var init_batches2 = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/messages/batches.mjs"() {
    init_resource();
    init_pagination();
    init_headers();
    init_jsonl();
    init_error2();
    init_path();
    Batches2 = class extends APIResource {
      /**
       * Send a batch of Message creation requests.
       *
       * The Message Batches API can be used to process multiple Messages API requests at
       * once. Once a Message Batch is created, it begins processing immediately. Batches
       * can take up to 24 hours to complete.
       *
       * Learn more about the Message Batches API in our
       * [user guide](https://docs.claude.com/en/docs/build-with-claude/batch-processing)
       *
       * @example
       * ```ts
       * const messageBatch = await client.messages.batches.create({
       *   requests: [
       *     {
       *       custom_id: 'my-custom-id-1',
       *       params: {
       *         max_tokens: 1024,
       *         messages: [
       *           { content: 'Hello, world', role: 'user' },
       *         ],
       *         model: 'claude-opus-4-6',
       *       },
       *     },
       *   ],
       * });
       * ```
       */
      create(body, options) {
        return this._client.post("/v1/messages/batches", { body, ...options });
      }
      /**
       * This endpoint is idempotent and can be used to poll for Message Batch
       * completion. To access the results of a Message Batch, make a request to the
       * `results_url` field in the response.
       *
       * Learn more about the Message Batches API in our
       * [user guide](https://docs.claude.com/en/docs/build-with-claude/batch-processing)
       *
       * @example
       * ```ts
       * const messageBatch = await client.messages.batches.retrieve(
       *   'message_batch_id',
       * );
       * ```
       */
      retrieve(messageBatchID, options) {
        return this._client.get(path3`/v1/messages/batches/${messageBatchID}`, options);
      }
      /**
       * List all Message Batches within a Workspace. Most recently created batches are
       * returned first.
       *
       * Learn more about the Message Batches API in our
       * [user guide](https://docs.claude.com/en/docs/build-with-claude/batch-processing)
       *
       * @example
       * ```ts
       * // Automatically fetches more pages as needed.
       * for await (const messageBatch of client.messages.batches.list()) {
       *   // ...
       * }
       * ```
       */
      list(query = {}, options) {
        return this._client.getAPIList("/v1/messages/batches", Page, { query, ...options });
      }
      /**
       * Delete a Message Batch.
       *
       * Message Batches can only be deleted once they've finished processing. If you'd
       * like to delete an in-progress batch, you must first cancel it.
       *
       * Learn more about the Message Batches API in our
       * [user guide](https://docs.claude.com/en/docs/build-with-claude/batch-processing)
       *
       * @example
       * ```ts
       * const deletedMessageBatch =
       *   await client.messages.batches.delete('message_batch_id');
       * ```
       */
      delete(messageBatchID, options) {
        return this._client.delete(path3`/v1/messages/batches/${messageBatchID}`, options);
      }
      /**
       * Batches may be canceled any time before processing ends. Once cancellation is
       * initiated, the batch enters a `canceling` state, at which time the system may
       * complete any in-progress, non-interruptible requests before finalizing
       * cancellation.
       *
       * The number of canceled requests is specified in `request_counts`. To determine
       * which requests were canceled, check the individual results within the batch.
       * Note that cancellation may not result in any canceled requests if they were
       * non-interruptible.
       *
       * Learn more about the Message Batches API in our
       * [user guide](https://docs.claude.com/en/docs/build-with-claude/batch-processing)
       *
       * @example
       * ```ts
       * const messageBatch = await client.messages.batches.cancel(
       *   'message_batch_id',
       * );
       * ```
       */
      cancel(messageBatchID, options) {
        return this._client.post(path3`/v1/messages/batches/${messageBatchID}/cancel`, options);
      }
      /**
       * Streams the results of a Message Batch as a `.jsonl` file.
       *
       * Each line in the file is a JSON object containing the result of a single request
       * in the Message Batch. Results are not guaranteed to be in the same order as
       * requests. Use the `custom_id` field to match results to requests.
       *
       * Learn more about the Message Batches API in our
       * [user guide](https://docs.claude.com/en/docs/build-with-claude/batch-processing)
       *
       * @example
       * ```ts
       * const messageBatchIndividualResponse =
       *   await client.messages.batches.results('message_batch_id');
       * ```
       */
      async results(messageBatchID, options) {
        const batch = await this.retrieve(messageBatchID);
        if (!batch.results_url) {
          throw new AnthropicError(`No batch \`results_url\`; Has it finished processing? ${batch.processing_status} - ${batch.id}`);
        }
        return this._client.get(batch.results_url, {
          ...options,
          headers: buildHeaders([{ Accept: "application/binary" }, options?.headers]),
          stream: true,
          __binaryResponse: true
        })._thenUnwrap((_, props) => JSONLDecoder.fromResponse(props.response, props.controller));
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/messages/messages.mjs
var Messages2, DEPRECATED_MODELS2, MODELS_TO_WARN_WITH_THINKING_ENABLED2;
var init_messages2 = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/messages/messages.mjs"() {
    init_resource();
    init_headers();
    init_stainless_helper_header();
    init_MessageStream();
    init_parser2();
    init_batches2();
    init_batches2();
    init_constants();
    Messages2 = class extends APIResource {
      constructor() {
        super(...arguments);
        this.batches = new Batches2(this._client);
      }
      create(body, options) {
        if (body.model in DEPRECATED_MODELS2) {
          console.warn(`The model '${body.model}' is deprecated and will reach end-of-life on ${DEPRECATED_MODELS2[body.model]}
Please migrate to a newer model. Visit https://docs.anthropic.com/en/docs/resources/model-deprecations for more information.`);
        }
        if (MODELS_TO_WARN_WITH_THINKING_ENABLED2.includes(body.model) && body.thinking && body.thinking.type === "enabled") {
          console.warn(`Using Claude with ${body.model} and 'thinking.type=enabled' is deprecated. Use 'thinking.type=adaptive' instead which results in better model performance in our testing: https://platform.claude.com/docs/en/build-with-claude/adaptive-thinking`);
        }
        let timeout = this._client._options.timeout;
        if (!body.stream && timeout == null) {
          const maxNonstreamingTokens = MODEL_NONSTREAMING_TOKENS[body.model] ?? void 0;
          timeout = this._client.calculateNonstreamingTimeout(body.max_tokens, maxNonstreamingTokens);
        }
        const helperHeader = stainlessHelperHeader(body.tools, body.messages);
        return this._client.post("/v1/messages", {
          body,
          timeout: timeout ?? 6e5,
          ...options,
          headers: buildHeaders([helperHeader, options?.headers]),
          stream: body.stream ?? false
        });
      }
      /**
       * Send a structured list of input messages with text and/or image content, along with an expected `output_config.format` and
       * the response will be automatically parsed and available in the `parsed_output` property of the message.
       *
       * @example
       * ```ts
       * const message = await client.messages.parse({
       *   model: 'claude-sonnet-4-5-20250929',
       *   max_tokens: 1024,
       *   messages: [{ role: 'user', content: 'What is 2+2?' }],
       *   output_config: {
       *     format: zodOutputFormat(z.object({ answer: z.number() })),
       *   },
       * });
       *
       * console.log(message.parsed_output?.answer); // 4
       * ```
       */
      parse(params, options) {
        return this.create(params, options).then((message) => parseMessage(message, params, { logger: this._client.logger ?? console }));
      }
      /**
       * Create a Message stream.
       *
       * If `output_config.format` is provided with a parseable format (like `zodOutputFormat()`),
       * the final message will include a `parsed_output` property with the parsed content.
       *
       * @example
       * ```ts
       * const stream = client.messages.stream({
       *   model: 'claude-sonnet-4-5-20250929',
       *   max_tokens: 1024,
       *   messages: [{ role: 'user', content: 'What is 2+2?' }],
       *   output_config: {
       *     format: zodOutputFormat(z.object({ answer: z.number() })),
       *   },
       * });
       *
       * const message = await stream.finalMessage();
       * console.log(message.parsed_output?.answer); // 4
       * ```
       */
      stream(body, options) {
        return MessageStream.createMessage(this, body, options, { logger: this._client.logger ?? console });
      }
      /**
       * Count the number of tokens in a Message.
       *
       * The Token Count API can be used to count the number of tokens in a Message,
       * including tools, images, and documents, without creating it.
       *
       * Learn more about token counting in our
       * [user guide](https://docs.claude.com/en/docs/build-with-claude/token-counting)
       *
       * @example
       * ```ts
       * const messageTokensCount =
       *   await client.messages.countTokens({
       *     messages: [{ content: 'Hello, world', role: 'user' }],
       *     model: 'claude-opus-4-6',
       *   });
       * ```
       */
      countTokens(body, options) {
        return this._client.post("/v1/messages/count_tokens", { body, ...options });
      }
    };
    DEPRECATED_MODELS2 = {
      "claude-1.3": "November 6th, 2024",
      "claude-1.3-100k": "November 6th, 2024",
      "claude-instant-1.1": "November 6th, 2024",
      "claude-instant-1.1-100k": "November 6th, 2024",
      "claude-instant-1.2": "November 6th, 2024",
      "claude-3-sonnet-20240229": "July 21st, 2025",
      "claude-3-opus-20240229": "January 5th, 2026",
      "claude-2.1": "July 21st, 2025",
      "claude-2.0": "July 21st, 2025",
      "claude-3-7-sonnet-latest": "February 19th, 2026",
      "claude-3-7-sonnet-20250219": "February 19th, 2026",
      "claude-3-5-haiku-latest": "February 19th, 2026",
      "claude-3-5-haiku-20241022": "February 19th, 2026",
      "claude-opus-4-0": "June 15th, 2026",
      "claude-opus-4-20250514": "June 15th, 2026",
      "claude-sonnet-4-0": "June 15th, 2026",
      "claude-sonnet-4-20250514": "June 15th, 2026"
    };
    MODELS_TO_WARN_WITH_THINKING_ENABLED2 = ["claude-mythos-preview", "claude-opus-4-6"];
    Messages2.Batches = Batches2;
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/models.mjs
var Models2;
var init_models2 = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/models.mjs"() {
    init_resource();
    init_pagination();
    init_headers();
    init_path();
    Models2 = class extends APIResource {
      /**
       * Get a specific model.
       *
       * The Models API response can be used to determine information about a specific
       * model or resolve a model alias to a model ID.
       */
      retrieve(modelID, params = {}, options) {
        const { betas } = params ?? {};
        return this._client.get(path3`/v1/models/${modelID}`, {
          ...options,
          headers: buildHeaders([
            { ...betas?.toString() != null ? { "anthropic-beta": betas?.toString() } : void 0 },
            options?.headers
          ])
        });
      }
      /**
       * List available models.
       *
       * The Models API response can be used to determine which models are available for
       * use in the API. More recently released models are listed first.
       */
      list(params = {}, options) {
        const { betas, ...query } = params ?? {};
        return this._client.getAPIList("/v1/models", Page, {
          query,
          ...options,
          headers: buildHeaders([
            { ...betas?.toString() != null ? { "anthropic-beta": betas?.toString() } : void 0 },
            options?.headers
          ])
        });
      }
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/index.mjs
var init_resources2 = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/resources/index.mjs"() {
    init_shared();
    init_beta();
    init_completions();
    init_messages2();
    init_models2();
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/utils/env.mjs
var readEnv;
var init_env = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/internal/utils/env.mjs"() {
    readEnv = (env) => {
      if (typeof globalThis.process !== "undefined") {
        return globalThis.process.env?.[env]?.trim() || void 0;
      }
      if (typeof globalThis.Deno !== "undefined") {
        return globalThis.Deno.env?.get?.(env)?.trim() || void 0;
      }
      return void 0;
    };
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/client.mjs
var _BaseAnthropic_instances, _a, _BaseAnthropic_encoder, _BaseAnthropic_baseURLOverridden, HUMAN_PROMPT, AI_PROMPT, BaseAnthropic, Anthropic;
var init_client = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/client.mjs"() {
    init_tslib();
    init_uuid();
    init_values();
    init_sleep();
    init_errors();
    init_detect_platform();
    init_shims();
    init_request_options();
    init_query();
    init_version();
    init_error();
    init_pagination();
    init_uploads2();
    init_resources2();
    init_api_promise();
    init_completions();
    init_models2();
    init_beta();
    init_messages2();
    init_detect_platform();
    init_headers();
    init_env();
    init_log();
    init_values();
    HUMAN_PROMPT = "\\n\\nHuman:";
    AI_PROMPT = "\\n\\nAssistant:";
    BaseAnthropic = class {
      /**
       * API Client for interfacing with the Anthropic API.
       *
       * @param {string | null | undefined} [opts.apiKey=process.env['ANTHROPIC_API_KEY'] ?? null]
       * @param {string | null | undefined} [opts.authToken=process.env['ANTHROPIC_AUTH_TOKEN'] ?? null]
       * @param {string} [opts.baseURL=process.env['ANTHROPIC_BASE_URL'] ?? https://api.anthropic.com] - Override the default base URL for the API.
       * @param {number} [opts.timeout=10 minutes] - The maximum amount of time (in milliseconds) the client will wait for a response before timing out.
       * @param {MergedRequestInit} [opts.fetchOptions] - Additional `RequestInit` options to be passed to `fetch` calls.
       * @param {Fetch} [opts.fetch] - Specify a custom `fetch` function implementation.
       * @param {number} [opts.maxRetries=2] - The maximum number of times the client will retry a request.
       * @param {HeadersLike} opts.defaultHeaders - Default headers to include with every request to the API.
       * @param {Record<string, string | undefined>} opts.defaultQuery - Default query parameters to include with every request to the API.
       * @param {boolean} [opts.dangerouslyAllowBrowser=false] - By default, client-side use of this library is not allowed, as it risks exposing your secret API credentials to attackers.
       */
      constructor({ baseURL = readEnv("ANTHROPIC_BASE_URL"), apiKey = readEnv("ANTHROPIC_API_KEY") ?? null, authToken = readEnv("ANTHROPIC_AUTH_TOKEN") ?? null, ...opts } = {}) {
        _BaseAnthropic_instances.add(this);
        _BaseAnthropic_encoder.set(this, void 0);
        const options = {
          apiKey,
          authToken,
          ...opts,
          baseURL: baseURL || `https://api.anthropic.com`
        };
        if (!options.dangerouslyAllowBrowser && isRunningInBrowser()) {
          throw new AnthropicError("It looks like you're running in a browser-like environment.\n\nThis is disabled by default, as it risks exposing your secret API credentials to attackers.\nIf you understand the risks and have appropriate mitigations in place,\nyou can set the `dangerouslyAllowBrowser` option to `true`, e.g.,\n\nnew Anthropic({ apiKey, dangerouslyAllowBrowser: true });\n");
        }
        this.baseURL = options.baseURL;
        this.timeout = options.timeout ?? _a.DEFAULT_TIMEOUT;
        this.logger = options.logger ?? console;
        const defaultLogLevel = "warn";
        this.logLevel = defaultLogLevel;
        this.logLevel = parseLogLevel(options.logLevel, "ClientOptions.logLevel", this) ?? parseLogLevel(readEnv("ANTHROPIC_LOG"), "process.env['ANTHROPIC_LOG']", this) ?? defaultLogLevel;
        this.fetchOptions = options.fetchOptions;
        this.maxRetries = options.maxRetries ?? 2;
        this.fetch = options.fetch ?? getDefaultFetch();
        __classPrivateFieldSet(this, _BaseAnthropic_encoder, FallbackEncoder, "f");
        this._options = options;
        this.apiKey = typeof apiKey === "string" ? apiKey : null;
        this.authToken = authToken;
      }
      /**
       * Create a new client instance re-using the same options given to the current client with optional overriding.
       */
      withOptions(options) {
        const client = new this.constructor({
          ...this._options,
          baseURL: this.baseURL,
          maxRetries: this.maxRetries,
          timeout: this.timeout,
          logger: this.logger,
          logLevel: this.logLevel,
          fetch: this.fetch,
          fetchOptions: this.fetchOptions,
          apiKey: this.apiKey,
          authToken: this.authToken,
          ...options
        });
        return client;
      }
      defaultQuery() {
        return this._options.defaultQuery;
      }
      validateHeaders({ values, nulls }) {
        if (values.get("x-api-key") || values.get("authorization")) {
          return;
        }
        if (this.apiKey && values.get("x-api-key")) {
          return;
        }
        if (nulls.has("x-api-key")) {
          return;
        }
        if (this.authToken && values.get("authorization")) {
          return;
        }
        if (nulls.has("authorization")) {
          return;
        }
        throw new Error('Could not resolve authentication method. Expected either apiKey or authToken to be set. Or for one of the "X-Api-Key" or "Authorization" headers to be explicitly omitted');
      }
      async authHeaders(opts) {
        return buildHeaders([await this.apiKeyAuth(opts), await this.bearerAuth(opts)]);
      }
      async apiKeyAuth(opts) {
        if (this.apiKey == null) {
          return void 0;
        }
        return buildHeaders([{ "X-Api-Key": this.apiKey }]);
      }
      async bearerAuth(opts) {
        if (this.authToken == null) {
          return void 0;
        }
        return buildHeaders([{ Authorization: `Bearer ${this.authToken}` }]);
      }
      /**
       * Basic re-implementation of `qs.stringify` for primitive types.
       */
      stringifyQuery(query) {
        return stringifyQuery(query);
      }
      getUserAgent() {
        return `${this.constructor.name}/JS ${VERSION}`;
      }
      defaultIdempotencyKey() {
        return `stainless-node-retry-${uuid4()}`;
      }
      makeStatusError(status, error, message, headers) {
        return APIError.generate(status, error, message, headers);
      }
      buildURL(path7, query, defaultBaseURL) {
        const baseURL = !__classPrivateFieldGet(this, _BaseAnthropic_instances, "m", _BaseAnthropic_baseURLOverridden).call(this) && defaultBaseURL || this.baseURL;
        const url = isAbsoluteURL(path7) ? new URL(path7) : new URL(baseURL + (baseURL.endsWith("/") && path7.startsWith("/") ? path7.slice(1) : path7));
        const defaultQuery = this.defaultQuery();
        const pathQuery = Object.fromEntries(url.searchParams);
        if (!isEmptyObj(defaultQuery) || !isEmptyObj(pathQuery)) {
          query = { ...pathQuery, ...defaultQuery, ...query };
        }
        if (typeof query === "object" && query && !Array.isArray(query)) {
          url.search = this.stringifyQuery(query);
        }
        return url.toString();
      }
      _calculateNonstreamingTimeout(maxTokens) {
        const defaultTimeout = 10 * 60;
        const expectedTimeout = 60 * 60 * maxTokens / 128e3;
        if (expectedTimeout > defaultTimeout) {
          throw new AnthropicError("Streaming is required for operations that may take longer than 10 minutes. See https://github.com/anthropics/anthropic-sdk-typescript#streaming-responses for more details");
        }
        return defaultTimeout * 1e3;
      }
      /**
       * Used as a callback for mutating the given `FinalRequestOptions` object.
       */
      async prepareOptions(options) {
      }
      /**
       * Used as a callback for mutating the given `RequestInit` object.
       *
       * This is useful for cases where you want to add certain headers based off of
       * the request properties, e.g. `method` or `url`.
       */
      async prepareRequest(request, { url, options }) {
      }
      get(path7, opts) {
        return this.methodRequest("get", path7, opts);
      }
      post(path7, opts) {
        return this.methodRequest("post", path7, opts);
      }
      patch(path7, opts) {
        return this.methodRequest("patch", path7, opts);
      }
      put(path7, opts) {
        return this.methodRequest("put", path7, opts);
      }
      delete(path7, opts) {
        return this.methodRequest("delete", path7, opts);
      }
      methodRequest(method, path7, opts) {
        return this.request(Promise.resolve(opts).then((opts2) => {
          return { method, path: path7, ...opts2 };
        }));
      }
      request(options, remainingRetries = null) {
        return new APIPromise(this, this.makeRequest(options, remainingRetries, void 0));
      }
      async makeRequest(optionsInput, retriesRemaining, retryOfRequestLogID) {
        const options = await optionsInput;
        const maxRetries = options.maxRetries ?? this.maxRetries;
        if (retriesRemaining == null) {
          retriesRemaining = maxRetries;
        }
        await this.prepareOptions(options);
        const { req, url, timeout } = await this.buildRequest(options, {
          retryCount: maxRetries - retriesRemaining
        });
        await this.prepareRequest(req, { url, options });
        const requestLogID = "log_" + (Math.random() * (1 << 24) | 0).toString(16).padStart(6, "0");
        const retryLogStr = retryOfRequestLogID === void 0 ? "" : `, retryOf: ${retryOfRequestLogID}`;
        const startTime = Date.now();
        loggerFor(this).debug(`[${requestLogID}] sending request`, formatRequestDetails({
          retryOfRequestLogID,
          method: options.method,
          url,
          options,
          headers: req.headers
        }));
        if (options.signal?.aborted) {
          throw new APIUserAbortError();
        }
        const controller = new AbortController();
        const response = await this.fetchWithTimeout(url, req, timeout, controller).catch(castToError);
        const headersTime = Date.now();
        if (response instanceof globalThis.Error) {
          const retryMessage = `retrying, ${retriesRemaining} attempts remaining`;
          if (options.signal?.aborted) {
            throw new APIUserAbortError();
          }
          const isTimeout = isAbortError(response) || /timed? ?out/i.test(String(response) + ("cause" in response ? String(response.cause) : ""));
          if (retriesRemaining) {
            loggerFor(this).info(`[${requestLogID}] connection ${isTimeout ? "timed out" : "failed"} - ${retryMessage}`);
            loggerFor(this).debug(`[${requestLogID}] connection ${isTimeout ? "timed out" : "failed"} (${retryMessage})`, formatRequestDetails({
              retryOfRequestLogID,
              url,
              durationMs: headersTime - startTime,
              message: response.message
            }));
            return this.retryRequest(options, retriesRemaining, retryOfRequestLogID ?? requestLogID);
          }
          loggerFor(this).info(`[${requestLogID}] connection ${isTimeout ? "timed out" : "failed"} - error; no more retries left`);
          loggerFor(this).debug(`[${requestLogID}] connection ${isTimeout ? "timed out" : "failed"} (error; no more retries left)`, formatRequestDetails({
            retryOfRequestLogID,
            url,
            durationMs: headersTime - startTime,
            message: response.message
          }));
          if (isTimeout) {
            throw new APIConnectionTimeoutError();
          }
          throw new APIConnectionError({ cause: response });
        }
        const specialHeaders = [...response.headers.entries()].filter(([name]) => name === "request-id").map(([name, value]) => ", " + name + ": " + JSON.stringify(value)).join("");
        const responseInfo = `[${requestLogID}${retryLogStr}${specialHeaders}] ${req.method} ${url} ${response.ok ? "succeeded" : "failed"} with status ${response.status} in ${headersTime - startTime}ms`;
        if (!response.ok) {
          const shouldRetry = await this.shouldRetry(response);
          if (retriesRemaining && shouldRetry) {
            const retryMessage2 = `retrying, ${retriesRemaining} attempts remaining`;
            await CancelReadableStream(response.body);
            loggerFor(this).info(`${responseInfo} - ${retryMessage2}`);
            loggerFor(this).debug(`[${requestLogID}] response error (${retryMessage2})`, formatRequestDetails({
              retryOfRequestLogID,
              url: response.url,
              status: response.status,
              headers: response.headers,
              durationMs: headersTime - startTime
            }));
            return this.retryRequest(options, retriesRemaining, retryOfRequestLogID ?? requestLogID, response.headers);
          }
          const retryMessage = shouldRetry ? `error; no more retries left` : `error; not retryable`;
          loggerFor(this).info(`${responseInfo} - ${retryMessage}`);
          const errText = await response.text().catch((err2) => castToError(err2).message);
          const errJSON = safeJSON(errText);
          const errMessage = errJSON ? void 0 : errText;
          loggerFor(this).debug(`[${requestLogID}] response error (${retryMessage})`, formatRequestDetails({
            retryOfRequestLogID,
            url: response.url,
            status: response.status,
            headers: response.headers,
            message: errMessage,
            durationMs: Date.now() - startTime
          }));
          const err = this.makeStatusError(response.status, errJSON, errMessage, response.headers);
          throw err;
        }
        loggerFor(this).info(responseInfo);
        loggerFor(this).debug(`[${requestLogID}] response start`, formatRequestDetails({
          retryOfRequestLogID,
          url: response.url,
          status: response.status,
          headers: response.headers,
          durationMs: headersTime - startTime
        }));
        return { response, options, controller, requestLogID, retryOfRequestLogID, startTime };
      }
      getAPIList(path7, Page2, opts) {
        return this.requestAPIList(Page2, opts && "then" in opts ? opts.then((opts2) => ({ method: "get", path: path7, ...opts2 })) : { method: "get", path: path7, ...opts });
      }
      requestAPIList(Page2, options) {
        const request = this.makeRequest(options, null, void 0);
        return new PagePromise(this, request, Page2);
      }
      async fetchWithTimeout(url, init, ms, controller) {
        const { signal, method, ...options } = init || {};
        const abort = this._makeAbort(controller);
        if (signal)
          signal.addEventListener("abort", abort, { once: true });
        const timeout = setTimeout(abort, ms);
        const isReadableBody = globalThis.ReadableStream && options.body instanceof globalThis.ReadableStream || typeof options.body === "object" && options.body !== null && Symbol.asyncIterator in options.body;
        const fetchOptions = {
          signal: controller.signal,
          ...isReadableBody ? { duplex: "half" } : {},
          method: "GET",
          ...options
        };
        if (method) {
          fetchOptions.method = method.toUpperCase();
        }
        try {
          return await this.fetch.call(void 0, url, fetchOptions);
        } finally {
          clearTimeout(timeout);
        }
      }
      async shouldRetry(response) {
        const shouldRetryHeader = response.headers.get("x-should-retry");
        if (shouldRetryHeader === "true")
          return true;
        if (shouldRetryHeader === "false")
          return false;
        if (response.status === 408)
          return true;
        if (response.status === 409)
          return true;
        if (response.status === 429)
          return true;
        if (response.status >= 500)
          return true;
        return false;
      }
      async retryRequest(options, retriesRemaining, requestLogID, responseHeaders) {
        let timeoutMillis;
        const retryAfterMillisHeader = responseHeaders?.get("retry-after-ms");
        if (retryAfterMillisHeader) {
          const timeoutMs = parseFloat(retryAfterMillisHeader);
          if (!Number.isNaN(timeoutMs)) {
            timeoutMillis = timeoutMs;
          }
        }
        const retryAfterHeader = responseHeaders?.get("retry-after");
        if (retryAfterHeader && !timeoutMillis) {
          const timeoutSeconds = parseFloat(retryAfterHeader);
          if (!Number.isNaN(timeoutSeconds)) {
            timeoutMillis = timeoutSeconds * 1e3;
          } else {
            timeoutMillis = Date.parse(retryAfterHeader) - Date.now();
          }
        }
        if (timeoutMillis === void 0) {
          const maxRetries = options.maxRetries ?? this.maxRetries;
          timeoutMillis = this.calculateDefaultRetryTimeoutMillis(retriesRemaining, maxRetries);
        }
        await sleep(timeoutMillis);
        return this.makeRequest(options, retriesRemaining - 1, requestLogID);
      }
      calculateDefaultRetryTimeoutMillis(retriesRemaining, maxRetries) {
        const initialRetryDelay = 0.5;
        const maxRetryDelay = 8;
        const numRetries = maxRetries - retriesRemaining;
        const sleepSeconds = Math.min(initialRetryDelay * Math.pow(2, numRetries), maxRetryDelay);
        const jitter = 1 - Math.random() * 0.25;
        return sleepSeconds * jitter * 1e3;
      }
      calculateNonstreamingTimeout(maxTokens, maxNonstreamingTokens) {
        const maxTime = 60 * 60 * 1e3;
        const defaultTime = 60 * 10 * 1e3;
        const expectedTime = maxTime * maxTokens / 128e3;
        if (expectedTime > defaultTime || maxNonstreamingTokens != null && maxTokens > maxNonstreamingTokens) {
          throw new AnthropicError("Streaming is required for operations that may take longer than 10 minutes. See https://github.com/anthropics/anthropic-sdk-typescript#long-requests for more details");
        }
        return defaultTime;
      }
      async buildRequest(inputOptions, { retryCount = 0 } = {}) {
        const options = { ...inputOptions };
        const { method, path: path7, query, defaultBaseURL } = options;
        const url = this.buildURL(path7, query, defaultBaseURL);
        if ("timeout" in options)
          validatePositiveInteger("timeout", options.timeout);
        options.timeout = options.timeout ?? this.timeout;
        const { bodyHeaders, body } = this.buildBody({ options });
        const reqHeaders = await this.buildHeaders({ options: inputOptions, method, bodyHeaders, retryCount });
        const req = {
          method,
          headers: reqHeaders,
          ...options.signal && { signal: options.signal },
          ...globalThis.ReadableStream && body instanceof globalThis.ReadableStream && { duplex: "half" },
          ...body && { body },
          ...this.fetchOptions ?? {},
          ...options.fetchOptions ?? {}
        };
        return { req, url, timeout: options.timeout };
      }
      async buildHeaders({ options, method, bodyHeaders, retryCount }) {
        let idempotencyHeaders = {};
        if (this.idempotencyHeader && method !== "get") {
          if (!options.idempotencyKey)
            options.idempotencyKey = this.defaultIdempotencyKey();
          idempotencyHeaders[this.idempotencyHeader] = options.idempotencyKey;
        }
        const headers = buildHeaders([
          idempotencyHeaders,
          {
            Accept: "application/json",
            "User-Agent": this.getUserAgent(),
            "X-Stainless-Retry-Count": String(retryCount),
            ...options.timeout ? { "X-Stainless-Timeout": String(Math.trunc(options.timeout / 1e3)) } : {},
            ...getPlatformHeaders(),
            ...this._options.dangerouslyAllowBrowser ? { "anthropic-dangerous-direct-browser-access": "true" } : void 0,
            "anthropic-version": "2023-06-01"
          },
          await this.authHeaders(options),
          this._options.defaultHeaders,
          bodyHeaders,
          options.headers
        ]);
        this.validateHeaders(headers);
        return headers.values;
      }
      _makeAbort(controller) {
        return () => controller.abort();
      }
      buildBody({ options: { body, headers: rawHeaders } }) {
        if (!body) {
          return { bodyHeaders: void 0, body: void 0 };
        }
        const headers = buildHeaders([rawHeaders]);
        if (
          // Pass raw type verbatim
          ArrayBuffer.isView(body) || body instanceof ArrayBuffer || body instanceof DataView || typeof body === "string" && // Preserve legacy string encoding behavior for now
          headers.values.has("content-type") || // `Blob` is superset of `File`
          globalThis.Blob && body instanceof globalThis.Blob || // `FormData` -> `multipart/form-data`
          body instanceof FormData || // `URLSearchParams` -> `application/x-www-form-urlencoded`
          body instanceof URLSearchParams || // Send chunked stream (each chunk has own `length`)
          globalThis.ReadableStream && body instanceof globalThis.ReadableStream
        ) {
          return { bodyHeaders: void 0, body };
        } else if (typeof body === "object" && (Symbol.asyncIterator in body || Symbol.iterator in body && "next" in body && typeof body.next === "function")) {
          return { bodyHeaders: void 0, body: ReadableStreamFrom(body) };
        } else if (typeof body === "object" && headers.values.get("content-type") === "application/x-www-form-urlencoded") {
          return {
            bodyHeaders: { "content-type": "application/x-www-form-urlencoded" },
            body: this.stringifyQuery(body)
          };
        } else {
          return __classPrivateFieldGet(this, _BaseAnthropic_encoder, "f").call(this, { body, headers });
        }
      }
    };
    _a = BaseAnthropic, _BaseAnthropic_encoder = /* @__PURE__ */ new WeakMap(), _BaseAnthropic_instances = /* @__PURE__ */ new WeakSet(), _BaseAnthropic_baseURLOverridden = function _BaseAnthropic_baseURLOverridden2() {
      return this.baseURL !== "https://api.anthropic.com";
    };
    BaseAnthropic.Anthropic = _a;
    BaseAnthropic.HUMAN_PROMPT = HUMAN_PROMPT;
    BaseAnthropic.AI_PROMPT = AI_PROMPT;
    BaseAnthropic.DEFAULT_TIMEOUT = 6e5;
    BaseAnthropic.AnthropicError = AnthropicError;
    BaseAnthropic.APIError = APIError;
    BaseAnthropic.APIConnectionError = APIConnectionError;
    BaseAnthropic.APIConnectionTimeoutError = APIConnectionTimeoutError;
    BaseAnthropic.APIUserAbortError = APIUserAbortError;
    BaseAnthropic.NotFoundError = NotFoundError;
    BaseAnthropic.ConflictError = ConflictError;
    BaseAnthropic.RateLimitError = RateLimitError;
    BaseAnthropic.BadRequestError = BadRequestError;
    BaseAnthropic.AuthenticationError = AuthenticationError;
    BaseAnthropic.InternalServerError = InternalServerError;
    BaseAnthropic.PermissionDeniedError = PermissionDeniedError;
    BaseAnthropic.UnprocessableEntityError = UnprocessableEntityError;
    BaseAnthropic.toFile = toFile;
    Anthropic = class extends BaseAnthropic {
      constructor() {
        super(...arguments);
        this.completions = new Completions(this);
        this.messages = new Messages2(this);
        this.models = new Models2(this);
        this.beta = new Beta(this);
      }
    };
    Anthropic.Completions = Completions;
    Anthropic.Messages = Messages2;
    Anthropic.Models = Models2;
    Anthropic.Beta = Beta;
  }
});

// node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/index.mjs
var init_sdk = __esm({
  "node_modules/.pnpm/@anthropic-ai+sdk@0.90.0/node_modules/@anthropic-ai/sdk/index.mjs"() {
    init_client();
    init_uploads2();
    init_api_promise();
    init_client();
    init_pagination();
    init_error();
  }
});

// src/main/llm/anthropic.ts
function trimSlash(url) {
  return url.replace(/\/+$/, "");
}
function splitAnthropicMessages(messages) {
  const systemParts = messages.filter((m) => m.role === "system").map((m) => m.content);
  const system = systemParts.length ? systemParts.join("\n\n") : void 0;
  const out = [];
  for (const m of messages) {
    if (m.role === "system") continue;
    if (m.role === "tool") {
      out.push({ role: "user", content: `[tool]
${m.content}` });
      continue;
    }
    out.push({ role: m.role, content: m.content });
  }
  return { system, messages: out };
}
function toAnthropicTools(tools) {
  return tools.map((t) => ({
    name: t.name,
    description: t.description,
    input_schema: t.parameters
  }));
}
async function* mapAnthropicStream(stream, signal) {
  const toolJsonByIndex = /* @__PURE__ */ new Map();
  const toolNameByIndex = /* @__PURE__ */ new Map();
  for await (const evt of stream) {
    if (signal?.aborted) break;
    if (evt.type === "content_block_start") {
      const block = evt.content_block;
      if (block.type === "tool_use") {
        toolNameByIndex.set(evt.index, block.name);
        toolJsonByIndex.set(evt.index, "");
      }
    }
    if (evt.type === "content_block_delta") {
      const d = evt.delta;
      if (d.type === "text_delta") {
        yield { text: d.text };
      }
      if (d.type === "input_json_delta") {
        const cur = toolJsonByIndex.get(evt.index) ?? "";
        toolJsonByIndex.set(evt.index, cur + d.partial_json);
      }
    }
    if (evt.type === "content_block_stop") {
      const name = toolNameByIndex.get(evt.index);
      if (name === void 0) continue;
      const jsonStr = toolJsonByIndex.get(evt.index) ?? "";
      let args = {};
      if (jsonStr.trim()) {
        try {
          args = JSON.parse(jsonStr);
        } catch {
          args = jsonStr;
        }
      }
      yield { toolCall: { name, args } };
      toolNameByIndex.delete(evt.index);
      toolJsonByIndex.delete(evt.index);
    }
  }
}
var AnthropicProvider;
var init_anthropic = __esm({
  "src/main/llm/anthropic.ts"() {
    "use strict";
    init_sdk();
    AnthropicProvider = class {
      constructor(apiKey, model, baseURL) {
        this.apiKey = apiKey;
        this.model = model;
        this.apiBase = trimSlash(baseURL ?? "https://api.anthropic.com");
        this.client = new Anthropic({
          apiKey,
          baseURL: this.apiBase
        });
      }
      apiKey;
      model;
      name = "anthropic";
      client;
      apiBase;
      async chat(messages, tools, options) {
        const { system, messages: mapped } = splitAnthropicMessages(messages);
        const params = {
          model: this.model,
          max_tokens: 4096,
          messages: mapped,
          stream: true
        };
        if (system) params.system = system;
        if (tools?.length) {
          params.tools = toAnthropicTools(tools);
        }
        const stream = await this.client.messages.create(params, { signal: options?.signal });
        return mapAnthropicStream(stream, options?.signal);
      }
      async isAvailable() {
        if (!this.apiKey.trim()) return false;
        try {
          const res = await fetch(`${this.apiBase}/v1/models`, {
            method: "GET",
            headers: {
              "x-api-key": this.apiKey,
              "anthropic-version": "2023-06-01"
            },
            signal: AbortSignal.timeout(4e3)
          });
          return res.ok;
        } catch {
          return false;
        }
      }
    };
  }
});

// src/main/llm/configStore.ts
function readRawConfig() {
  if (configCache) return configCache;
  if (!(0, import_node_fs8.existsSync)(OPENRAY_CONFIG_PATH)) {
    configCache = {};
    return configCache;
  }
  try {
    const raw = (0, import_node_fs8.readFileSync)(OPENRAY_CONFIG_PATH, "utf-8");
    configCache = JSON.parse(raw);
    return configCache;
  } catch {
    configCache = {};
    return configCache;
  }
}
function flushConfig() {
  if (!configCache || !writeTimeout) return;
  try {
    (0, import_node_fs8.mkdirSync)((0, import_node_path8.dirname)(OPENRAY_CONFIG_PATH), { recursive: true });
    (0, import_node_fs8.writeFileSync)(OPENRAY_CONFIG_PATH, `${JSON.stringify(configCache, null, 2)}
`, "utf-8");
    if (writeTimeout) {
      clearTimeout(writeTimeout);
      writeTimeout = null;
    }
  } catch (err) {
    console.error("Failed to write config:", err);
  }
}
function writeConfigPatch(patch) {
  const current = readRawConfig();
  configCache = { ...current, ...patch };
  if (writeTimeout) clearTimeout(writeTimeout);
  writeTimeout = setTimeout(() => {
    flushConfig();
    writeTimeout = null;
  }, 1e3);
}
function getUiStateRetentionMs() {
  const raw = readRawConfig();
  const v = raw.uiStateRetentionMs;
  if (typeof v === "number" && Number.isFinite(v) && v >= 0) {
    return v;
  }
  return 6e4;
}
function getSafetyDryRun() {
  const raw = readRawConfig();
  return raw.safetyDryRun === true;
}
function setSafetyDryRun(value) {
  writeConfigPatch({ safetyDryRun: value });
}
function getAgentAlwaysAllowedCommands() {
  const value = readRawConfig().agentAlwaysAllowedCommands;
  if (!Array.isArray(value)) return [];
  return Array.from(
    new Set(
      value.filter(
        (entry) => typeof entry === "string" && /^[a-z0-9][a-z0-9._+-]{0,63}$/i.test(entry)
      )
    )
  );
}
function addAgentAlwaysAllowedCommand(command) {
  if (!/^[a-z0-9][a-z0-9._+-]{0,63}$/i.test(command)) return;
  writeConfigPatch({
    agentAlwaysAllowedCommands: Array.from(
      /* @__PURE__ */ new Set([...getAgentAlwaysAllowedCommands(), command.toLowerCase()])
    )
  });
}
var import_node_fs8, import_node_os4, import_node_path8, OPENRAY_CONFIG_DIR, OPENRAY_CONFIG_PATH, configCache, writeTimeout;
var init_configStore = __esm({
  "src/main/llm/configStore.ts"() {
    "use strict";
    import_node_fs8 = require("node:fs");
    import_node_os4 = require("node:os");
    import_node_path8 = require("node:path");
    OPENRAY_CONFIG_DIR = (0, import_node_path8.join)((0, import_node_os4.homedir)(), ".openray");
    OPENRAY_CONFIG_PATH = (0, import_node_path8.join)(OPENRAY_CONFIG_DIR, "config.json");
    configCache = null;
    writeTimeout = null;
  }
});

// src/main/llm/githubCopilotAuth.ts
function clearDeviceSession() {
  deviceSession = null;
}
async function startGithubDeviceFlow(clientId) {
  const body = new URLSearchParams({
    client_id: clientId,
    scope: "read:user user:email"
  });
  const res = await fetch("https://github.com/login/device/code", {
    method: "POST",
    headers: {
      Accept: "application/json",
      "Content-Type": "application/x-www-form-urlencoded"
    },
    body: body.toString()
  });
  if (!res.ok) {
    const t = await res.text();
    throw new Error(`GitHub device code failed: ${res.status} ${t.slice(0, 200)}`);
  }
  const json = await res.json();
  if (!json.device_code || !json.user_code || !json.verification_uri) {
    throw new Error("GitHub device code: malformed response");
  }
  deviceSession = {
    device_code: json.device_code,
    interval: Math.max(5, json.interval ?? 5),
    client_id: clientId
  };
  return json;
}
async function pollGithubDeviceFlow() {
  if (!deviceSession) {
    return { status: "error", error: "No device session. Start sign-in again." };
  }
  const body = new URLSearchParams({
    client_id: deviceSession.client_id,
    device_code: deviceSession.device_code,
    grant_type: "urn:ietf:params:oauth:grant-type:device_code"
  });
  const res = await fetch("https://github.com/login/oauth/access_token", {
    method: "POST",
    headers: {
      Accept: "application/json",
      "Content-Type": "application/x-www-form-urlencoded"
    },
    body: body.toString()
  });
  const json = await res.json();
  const err = typeof json.error === "string" ? json.error : "";
  if (err === "authorization_pending") {
    return { status: "authorization_pending" };
  }
  if (err === "slow_down") {
    return { status: "slow_down" };
  }
  if (err && err !== "") {
    deviceSession = null;
    return { status: "error", error: typeof json.error_description === "string" ? json.error_description : err };
  }
  const access_token = typeof json.access_token === "string" ? json.access_token : "";
  if (!access_token) {
    return { status: "error", error: "No access_token in response" };
  }
  const refresh_token = typeof json.refresh_token === "string" ? json.refresh_token : void 0;
  const expires_in = typeof json.expires_in === "number" ? json.expires_in : void 0;
  deviceSession = null;
  return { status: "success", access_token, refresh_token, expires_in };
}
async function refreshGithubAccessToken(refreshToken, clientId, signal) {
  const body = new URLSearchParams({
    client_id: clientId,
    grant_type: "refresh_token",
    refresh_token: refreshToken
  });
  const res = await fetch("https://github.com/login/oauth/access_token", {
    method: "POST",
    headers: {
      Accept: "application/json",
      "Content-Type": "application/x-www-form-urlencoded"
    },
    body: body.toString(),
    signal
  });
  if (!res.ok) {
    const t = await res.text();
    throw new Error(`Token refresh failed: ${res.status} ${t.slice(0, 200)}`);
  }
  const json = await res.json();
  const access_token = typeof json.access_token === "string" ? json.access_token : "";
  if (!access_token) {
    throw new Error("Token refresh: missing access_token");
  }
  return {
    access_token,
    refresh_token: typeof json.refresh_token === "string" ? json.refresh_token : refreshToken,
    expires_in: typeof json.expires_in === "number" ? json.expires_in : void 0
  };
}
function persistCopilotTokens(accessToken, refreshToken, expiresInSec) {
  const patch = {
    copilotGithubToken: accessToken
  };
  if (refreshToken) patch.copilotRefreshToken = refreshToken;
  if (expiresInSec !== void 0) {
    patch.copilotExpiresAt = Date.now() + expiresInSec * 1e3;
  }
  writeConfigPatch(patch);
}
async function copilotApiPing(token) {
  try {
    const res = await fetch(`${COPILOT_API}/models`, {
      method: "GET",
      headers: {
        Authorization: `Bearer ${token}`,
        "Editor-Version": "Tezbar/0.1.0",
        "Copilot-Integration-Id": "vscode-chat",
        Accept: "application/json"
      },
      signal: AbortSignal.timeout(5e3)
    });
    return res.ok;
  } catch {
    return false;
  }
}
var COPILOT_API, deviceSession;
var init_githubCopilotAuth = __esm({
  "src/main/llm/githubCopilotAuth.ts"() {
    "use strict";
    init_configStore();
    COPILOT_API = "https://api.githubcopilot.com";
    deviceSession = null;
  }
});

// src/main/llm/openaiSse.ts
async function* parseOpenAISSE(res, signal) {
  const reader = res.body?.getReader();
  if (!reader) throw new Error("OpenAI-compatible: empty response body");
  const decoder = new TextDecoder();
  let buffer = "";
  const toolParts = /* @__PURE__ */ new Map();
  const processPayload = function* (json) {
    const root = json;
    const choice = root.choices?.[0];
    if (!choice) return;
    const delta = choice.delta;
    if (delta?.content) {
      yield { text: delta.content };
    }
    if (Array.isArray(delta?.tool_calls)) {
      for (const tc of delta.tool_calls) {
        const idx = typeof tc.index === "number" ? tc.index : 0;
        const cur = toolParts.get(idx) ?? { args: "" };
        if (tc.id) cur.id = tc.id;
        if (tc.function?.name) cur.name = tc.function.name;
        if (tc.function?.arguments) cur.args += tc.function.arguments;
        toolParts.set(idx, cur);
      }
    }
    if (choice.finish_reason === "tool_calls") {
      const ordered = Array.from(toolParts.entries()).sort((a, b) => a[0] - b[0]);
      for (const [, tc] of ordered) {
        if (!tc.name) continue;
        let args = {};
        if (tc.args.trim()) {
          try {
            args = JSON.parse(tc.args);
          } catch {
            args = tc.args;
          }
        }
        yield { toolCall: { name: tc.name, args } };
      }
      toolParts.clear();
    }
  };
  while (true) {
    if (signal?.aborted) break;
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    let newline;
    while ((newline = buffer.indexOf("\n")) !== -1) {
      const rawLine = buffer.slice(0, newline);
      buffer = buffer.slice(newline + 1);
      const line = rawLine.trimEnd();
      if (!line.startsWith("data:")) continue;
      const data = line.slice(5).trimStart();
      if (data === "[DONE]") continue;
      let json;
      try {
        json = JSON.parse(data);
      } catch {
        continue;
      }
      yield* processPayload(json);
    }
  }
  if (toolParts.size > 0) {
    for (const [, tc] of Array.from(toolParts.entries()).sort((a, b) => a[0] - b[0])) {
      if (!tc.name) continue;
      let args = {};
      if (tc.args.trim()) {
        try {
          args = JSON.parse(tc.args);
        } catch {
          args = tc.args;
        }
      }
      yield { toolCall: { name: tc.name, args } };
    }
    toolParts.clear();
  }
}
var init_openaiSse = __esm({
  "src/main/llm/openaiSse.ts"() {
    "use strict";
  }
});

// src/main/llm/copilot.ts
function toOpenAIMessages(messages) {
  return messages.map((m) => ({ role: m.role, content: m.content }));
}
function toOpenAITools(tools) {
  return tools.map((t) => ({
    type: "function",
    function: {
      name: t.name,
      description: t.description,
      parameters: t.parameters
    }
  }));
}
var COPILOT_CHAT, CopilotProvider;
var init_copilot = __esm({
  "src/main/llm/copilot.ts"() {
    "use strict";
    init_configStore();
    init_githubCopilotAuth();
    init_openaiSse();
    COPILOT_CHAT = "https://api.githubcopilot.com/chat/completions";
    CopilotProvider = class {
      constructor(model) {
        this.model = model;
      }
      model;
      name = "copilot";
      readTokens() {
        const c = readRawConfig();
        return {
          access: (typeof c.copilotGithubToken === "string" ? c.copilotGithubToken : "") || (typeof c.apiKey === "string" ? c.apiKey : ""),
          refresh: typeof c.copilotRefreshToken === "string" ? c.copilotRefreshToken : void 0,
          expiresAt: typeof c.copilotExpiresAt === "number" ? c.copilotExpiresAt : 0,
          clientId: typeof c.githubOAuthClientId === "string" ? c.githubOAuthClientId : ""
        };
      }
      async refreshIfNeeded(signal) {
        const { access, refresh, expiresAt, clientId } = this.readTokens();
        if (!refresh || !clientId) {
          return access;
        }
        const stale = expiresAt > 0 && Date.now() > expiresAt - 12e4;
        if (!stale) {
          return access;
        }
        const next = await refreshGithubAccessToken(refresh, clientId, signal);
        const expires_in = next.expires_in;
        const patch = {
          copilotGithubToken: next.access_token,
          copilotRefreshToken: next.refresh_token ?? refresh
        };
        if (expires_in !== void 0) {
          patch.copilotExpiresAt = Date.now() + expires_in * 1e3;
        }
        writeConfigPatch(patch);
        return next.access_token;
      }
      async chat(messages, tools, options) {
        console.log("[CopilotProvider] chat request to", COPILOT_CHAT);
        const token = await this.refreshIfNeeded(options?.signal);
        if (!token.trim()) {
          console.error("[CopilotProvider] missing token");
          throw new Error("GitHub Copilot: missing token. Add a PAT or complete device sign-in in Providers.");
        }
        const body = {
          model: this.model,
          messages: toOpenAIMessages(messages),
          stream: true
        };
        if (tools?.length) {
          body.tools = toOpenAITools(tools);
        }
        console.log("[CopilotProvider] payload size:", JSON.stringify(body).length, "bytes");
        const res = await fetch(COPILOT_CHAT, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            Authorization: `Bearer ${token}`,
            "Editor-Version": "Tezbar/0.1.0",
            "Copilot-Integration-Id": "vscode-chat"
          },
          body: JSON.stringify(body),
          signal: options?.signal
        });
        console.log("[CopilotProvider] response status:", res.status, res.statusText);
        if (!res.ok) {
          const errBody = await res.text().catch(() => "");
          console.error("[CopilotProvider] error body:", errBody);
          throw new Error(`Copilot error ${res.status}: ${errBody.slice(0, 500)}`);
        }
        return parseOpenAISSE(res, options?.signal);
      }
      async isAvailable() {
        const { access } = this.readTokens();
        if (!access.trim()) return false;
        const ping = await copilotApiPing(access);
        if (ping) return true;
        return access.length > 20;
      }
      /** Bearer token after optional OAuth refresh (reads tokens from config on disk). */
      async getAccessToken(options) {
        return this.refreshIfNeeded(options?.signal);
      }
    };
  }
});

// src/main/llm/ollama.ts
function trimSlash2(url) {
  return url.replace(/\/+$/, "");
}
function toOllamaMessages(messages) {
  return messages.map((m) => {
    if (m.role === "tool") {
      return { role: "user", content: `[tool]
${m.content}` };
    }
    return { role: m.role, content: m.content };
  });
}
function toOllamaTools(tools) {
  return tools.map((t) => ({
    type: "function",
    function: {
      name: t.name,
      description: t.description,
      parameters: t.parameters
    }
  }));
}
async function* parseOllamaStream(res, signal) {
  const reader = res.body?.getReader();
  if (!reader) throw new Error("Ollama: empty response body");
  const decoder = new TextDecoder();
  let buffer = "";
  while (true) {
    if (signal?.aborted) break;
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    let nl;
    while ((nl = buffer.indexOf("\n")) !== -1) {
      const line = buffer.slice(0, nl).trim();
      buffer = buffer.slice(nl + 1);
      if (!line) continue;
      let row;
      try {
        row = JSON.parse(line);
      } catch {
        continue;
      }
      const err = row.error;
      if (typeof err === "string" && err) {
        throw new Error(err);
      }
      const msg = row.message;
      if (msg?.role === "assistant" && typeof msg.content === "string" && msg.content.length > 0) {
        yield { text: msg.content };
      }
      const toolCalls = row.message?.tool_calls;
      if (Array.isArray(toolCalls)) {
        for (const tc of toolCalls) {
          const name = tc.function?.name;
          if (!name) continue;
          const raw = tc.function?.arguments;
          let args = raw;
          if (typeof raw === "string") {
            try {
              args = JSON.parse(raw);
            } catch {
              args = raw;
            }
          }
          yield { toolCall: { name, args } };
        }
      }
    }
  }
}
var OllamaProvider;
var init_ollama = __esm({
  "src/main/llm/ollama.ts"() {
    "use strict";
    OllamaProvider = class {
      constructor(baseURL, model) {
        this.baseURL = baseURL;
        this.model = model;
      }
      baseURL;
      model;
      name = "ollama";
      async chat(messages, tools, options) {
        const url = `${trimSlash2(this.baseURL)}/api/chat`;
        const body = {
          model: this.model,
          messages: toOllamaMessages(messages),
          stream: true
        };
        if (tools?.length) {
          body.tools = toOllamaTools(tools);
        }
        const res = await fetch(url, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(body),
          signal: options?.signal
        });
        if (!res.ok) {
          const t = await res.text().catch(() => "");
          throw new Error(`Ollama error ${res.status}: ${t.slice(0, 500)}`);
        }
        return parseOllamaStream(res, options?.signal);
      }
      async isAvailable() {
        try {
          const url = `${trimSlash2(this.baseURL)}/api/tags`;
          const res = await fetch(url, { method: "GET", signal: AbortSignal.timeout(4e3) });
          return res.ok;
        } catch {
          return false;
        }
      }
    };
  }
});

// src/shared/llmErrors.ts
function apiErrorDetail(raw) {
  const bodyStart = raw.indexOf("{");
  if (bodyStart < 0) return null;
  try {
    const parsed = JSON.parse(raw.slice(bodyStart));
    return typeof parsed.error?.message === "string" ? parsed.error.message.trim() : null;
  } catch {
    return null;
  }
}
function formatLlmErrorMessage(raw, providerLabel = "AI provider") {
  const trimmed = raw.trim();
  const detail = apiErrorDetail(trimmed);
  const message = detail || trimmed;
  const unsupportedModel = message.match(
    /supported API model names are (.+?), but you passed (.+?)(?:\.$|$)/i
  );
  if (unsupportedModel) {
    return `Model "${unsupportedModel[2]}" is not supported by this provider. Choose ${unsupportedModel[1]} and try again.`;
  }
  const missingModel = message.match(/Model\s+["']([^"']+)["']\s+not found/i);
  if (missingModel) {
    return `The selected model "${missingModel[1]}" is unavailable. Choose another model in AI settings and try again.`;
  }
  if (/pi exited before finishing/i.test(message)) {
    return "The agent stopped before it could finish. Check the selected model in AI settings and try again.";
  }
  if (!detail) return trimmed;
  const status = trimmed.match(/\berror\s+(\d{3})\b/i)?.[1];
  return `${providerLabel} request failed${status ? ` (${status})` : ""}: ${detail}`;
}
var init_llmErrors = __esm({
  "src/shared/llmErrors.ts"() {
    "use strict";
  }
});

// src/main/llm/openai.ts
function trimSlash3(url) {
  return url.replace(/\/+$/, "");
}
function chatCompletionsUrl(baseURL) {
  const base = trimSlash3(baseURL);
  return base.endsWith("/chat/completions") ? base : `${base}/chat/completions`;
}
function modelsUrl(baseURL) {
  const base = trimSlash3(baseURL);
  if (base.endsWith("/chat/completions")) {
    return `${base.slice(0, -"/chat/completions".length)}/models`;
  }
  return `${base}/models`;
}
function toOpenAIMessages2(messages) {
  return messages.map((m) => ({ role: m.role, content: m.content }));
}
function toOpenAITools2(tools) {
  return tools.map((t) => ({
    type: "function",
    function: {
      name: t.name,
      description: t.description,
      parameters: t.parameters
    }
  }));
}
var OpenAIProvider;
var init_openai = __esm({
  "src/main/llm/openai.ts"() {
    "use strict";
    init_llmErrors();
    init_openaiSse();
    OpenAIProvider = class {
      constructor(baseURL, apiKey, model, providerLabel = "OpenAI") {
        this.baseURL = baseURL;
        this.apiKey = apiKey;
        this.model = model;
        this.providerLabel = providerLabel;
      }
      baseURL;
      apiKey;
      model;
      providerLabel;
      name = "openai";
      async chat(messages, tools, options) {
        const url = chatCompletionsUrl(this.baseURL);
        const body = {
          model: this.model,
          messages: toOpenAIMessages2(messages),
          stream: true
        };
        if (tools?.length) {
          body.tools = toOpenAITools2(tools);
        }
        const res = await fetch(url, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            Authorization: `Bearer ${this.apiKey}`
          },
          body: JSON.stringify(body),
          signal: options?.signal
        });
        if (!res.ok) {
          const errBody = await res.text().catch(() => "");
          throw new Error(
            formatLlmErrorMessage(
              `${this.providerLabel} error ${res.status}: ${errBody.slice(0, 500)}`,
              this.providerLabel
            )
          );
        }
        return parseOpenAISSE(res, options?.signal);
      }
      async isAvailable() {
        if (!this.apiKey.trim()) return false;
        try {
          const url = modelsUrl(this.baseURL);
          const res = await fetch(url, {
            method: "GET",
            headers: { Authorization: `Bearer ${this.apiKey}` },
            signal: AbortSignal.timeout(4e3)
          });
          return res.ok;
        } catch {
          return false;
        }
      }
    };
  }
});

// src/main/llm/opencode.ts
var import_node_child_process6, import_node_util5, OpenCodeProvider;
var init_opencode = __esm({
  "src/main/llm/opencode.ts"() {
    "use strict";
    import_node_child_process6 = require("node:child_process");
    import_node_util5 = require("node:util");
    OpenCodeProvider = class {
      constructor(model) {
        this.model = model;
      }
      model;
      name = "opencode";
      async chat(messages, _tools, options) {
        const lastUserMessage = [...messages].reverse().find((m) => m.role === "user");
        if (!lastUserMessage) {
          throw new Error("OpenCode: no user message found");
        }
        return this.runOpenCode(lastUserMessage.content, options?.signal);
      }
      async *runOpenCode(message, signal) {
        const args = ["run", "--model", this.model, "--", message];
        const child = (0, import_node_child_process6.spawn)("opencode", args, {
          stdio: ["ignore", "pipe", "pipe"],
          env: { ...process.env, TERM: "dumb", CI: "true" }
        });
        if (signal) {
          const onAbort = () => {
            child.kill();
          };
          signal.addEventListener("abort", onAbort);
          if (signal.aborted) {
            child.kill();
          }
        }
        let output = "";
        let errorOutput = "";
        child.stdout?.on("data", (chunk) => {
          output += chunk.toString();
        });
        child.stderr?.on("data", (chunk) => {
          errorOutput += chunk.toString();
        });
        const exitCode = await new Promise((resolve4) => {
          child.on("close", (code) => resolve4(code ?? 1));
        });
        if (exitCode !== 0) {
          throw new Error(`OpenCode CLI error (exit ${exitCode}): ${errorOutput.slice(0, 500)}`);
        }
        if (output.trim()) {
          yield { text: output.trim() };
        }
      }
      async isAvailable() {
        try {
          const { execFile: execFile13 } = await import("node:child_process");
          const execFileAsync13 = (0, import_node_util5.promisify)(execFile13);
          const { stdout } = await execFileAsync13("which", ["opencode"], { timeout: 4e3 });
          return stdout.trim().length > 0;
        } catch {
          return false;
        }
      }
    };
  }
});

// src/main/llm/registry.ts
var registry_exports = {};
__export(registry_exports, {
  buildProviderForId: () => buildProviderForId,
  configForProvider: () => configForProvider,
  getProvider: () => getProvider,
  getProviderForTask: () => getProviderForTask,
  getSelectedPiModelPattern: () => getSelectedPiModelPattern,
  getSelectedPiProviderBridge: () => getSelectedPiProviderBridge,
  invalidateProviderCache: () => invalidateProviderCache,
  readLLMConfig: () => readLLMConfig
});
function normalizeCustomProviders(raw) {
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((value) => {
    if (!value || typeof value !== "object") return [];
    const entry = value;
    const id = typeof entry.id === "string" ? entry.id.trim() : "";
    const title = typeof entry.title === "string" ? entry.title.trim() : "";
    if (!id.startsWith("custom:") || !title) return [];
    return [{
      id,
      title,
      subtitle: typeof entry.subtitle === "string" ? entry.subtitle.trim() : void 0
    }];
  });
}
function providerIds(customProviders) {
  return [...Object.keys(DEFAULT_PROVIDER_MODELS), ...customProviders.map((provider) => provider.id)];
}
function normalizeProviderModels(raw, ids) {
  if (!raw || typeof raw !== "object") return void 0;
  const result = {};
  for (const provider of ids) {
    const models = raw[provider];
    if (!Array.isArray(models)) continue;
    result[provider] = normalizeProviderModelList(provider, models);
  }
  return result;
}
function normalizeProviderSelectedModels(raw, ids) {
  if (!raw || typeof raw !== "object") return void 0;
  const result = {};
  for (const provider of ids) {
    const value = raw[provider];
    if (typeof value === "string" && value.trim()) result[provider] = value.trim();
  }
  return result;
}
function normalizeProviderConfigs(raw, ids) {
  if (!raw || typeof raw !== "object") return void 0;
  const result = {};
  for (const provider of ids) {
    const value = raw[provider];
    if (!value || typeof value !== "object") continue;
    const config = value;
    result[provider] = {
      apiKey: typeof config.apiKey === "string" ? config.apiKey : void 0,
      baseURL: typeof config.baseURL === "string" ? config.baseURL : void 0,
      openaiCompatibleBaseURL: typeof config.openaiCompatibleBaseURL === "string" ? config.openaiCompatibleBaseURL : void 0,
      geminiApiKey: typeof config.geminiApiKey === "string" ? config.geminiApiKey : void 0,
      copilotGithubToken: typeof config.copilotGithubToken === "string" ? config.copilotGithubToken : void 0,
      githubOAuthClientId: typeof config.githubOAuthClientId === "string" ? config.githubOAuthClientId : void 0
    };
  }
  return result;
}
function normalizeFromRaw(raw) {
  const customProviders = normalizeCustomProviders(raw.customProviders);
  const ids = providerIds(customProviders);
  const p = raw.provider;
  const hasCopilotToken = typeof raw.copilotGithubToken === "string" && raw.copilotGithubToken.length > 0;
  const provider = typeof p === "string" && customProviders.some((provider2) => provider2.id === p) || p === "openai" || p === "openai-compatible" || p === "anthropic" || p === "ollama" || p === "copilot" || p === "gemini" || p === "opencode" || p === "deepseek" ? p : hasCopilotToken ? "copilot" : "ollama";
  const providerModels = normalizeProviderModels(raw.providerModels, ids);
  const providerSelectedModels = normalizeProviderSelectedModels(raw.providerSelectedModels, ids);
  const providerConfigs = normalizeProviderConfigs(raw.providerConfigs, ids);
  const selectedModel = providerSelectedModels?.[provider];
  const providerConfig = providerConfigs?.[provider] ?? {};
  const allowLegacyProviderFields = !providerConfigs || Object.keys(providerConfigs).length === 0;
  return {
    provider,
    customProviders,
    providerConfigs,
    apiKey: providerConfig.apiKey ?? (allowLegacyProviderFields && typeof raw.apiKey === "string" ? raw.apiKey : void 0),
    baseURL: providerConfig.baseURL ?? (allowLegacyProviderFields && typeof raw.baseURL === "string" ? raw.baseURL : void 0),
    openaiCompatibleBaseURL: providerConfig.openaiCompatibleBaseURL ?? (allowLegacyProviderFields && typeof raw.openaiCompatibleBaseURL === "string" ? raw.openaiCompatibleBaseURL : void 0),
    geminiApiKey: providerConfig.geminiApiKey ?? (allowLegacyProviderFields && typeof raw.geminiApiKey === "string" ? raw.geminiApiKey : void 0),
    model: selectedModel ?? (typeof raw.model === "string" ? raw.model : void 0),
    providerModels,
    providerSelectedModels,
    copilotGithubToken: providerConfig.copilotGithubToken ?? (allowLegacyProviderFields && typeof raw.copilotGithubToken === "string" ? raw.copilotGithubToken : void 0),
    copilotRefreshToken: typeof raw.copilotRefreshToken === "string" ? raw.copilotRefreshToken : void 0,
    copilotExpiresAt: typeof raw.copilotExpiresAt === "number" ? raw.copilotExpiresAt : void 0,
    githubOAuthClientId: providerConfig.githubOAuthClientId ?? (allowLegacyProviderFields && typeof raw.githubOAuthClientId === "string" ? raw.githubOAuthClientId : void 0),
    taskProviderOverrides: typeof raw.taskProviderOverrides === "object" && raw.taskProviderOverrides ? raw.taskProviderOverrides : void 0,
    taskModelOverrides: typeof raw.taskModelOverrides === "object" && raw.taskModelOverrides ? raw.taskModelOverrides : void 0,
    memoryEnabled: typeof raw.memoryEnabled === "boolean" ? raw.memoryEnabled : void 0,
    memoryMaxItems: typeof raw.memoryMaxItems === "number" ? raw.memoryMaxItems : void 0,
    memoryIncludePrivate: typeof raw.memoryIncludePrivate === "boolean" ? raw.memoryIncludePrivate : void 0,
    aiActionRequirePermission: typeof raw.aiActionRequirePermission === "boolean" ? raw.aiActionRequirePermission : void 0,
    aiActionRedactionEnabled: typeof raw.aiActionRedactionEnabled === "boolean" ? raw.aiActionRedactionEnabled : void 0,
    uiStateRetentionMs: typeof raw.uiStateRetentionMs === "number" ? raw.uiStateRetentionMs : void 0
  };
}
function readLLMConfig() {
  const raw = readRawConfig();
  if (Object.keys(raw).length === 0) {
    return { provider: "ollama", baseURL: DEFAULT_OLLAMA_BASE, model: DEFAULT_OLLAMA_MODEL };
  }
  const n = normalizeFromRaw(raw);
  if (n.provider === "ollama") {
    return {
      ...n,
      baseURL: n.baseURL ?? DEFAULT_OLLAMA_BASE,
      model: n.model ?? DEFAULT_OLLAMA_MODEL
    };
  }
  if (n.provider === "gemini") {
    return {
      ...n,
      baseURL: n.baseURL ?? DEFAULT_GEMINI_BASE,
      model: n.model ?? DEFAULT_GEMINI_MODEL
    };
  }
  if (n.provider === "deepseek") {
    return {
      ...n,
      baseURL: n.baseURL ?? DEFAULT_DEEPSEEK_BASE,
      model: n.model ?? DEFAULT_DEEPSEEK_MODEL
    };
  }
  return n;
}
function configForProvider(cfg, provider) {
  const providerConfig = cfg.providerConfigs?.[provider] ?? {};
  const useCurrentProviderFields = cfg.provider === provider;
  const model = cfg.providerSelectedModels?.[provider] ?? (useCurrentProviderFields ? cfg.model : void 0);
  const next = {
    ...cfg,
    provider,
    model,
    apiKey: providerConfig.apiKey ?? (useCurrentProviderFields ? cfg.apiKey : void 0),
    baseURL: providerConfig.baseURL ?? (useCurrentProviderFields ? cfg.baseURL : void 0),
    openaiCompatibleBaseURL: providerConfig.openaiCompatibleBaseURL ?? (useCurrentProviderFields ? cfg.openaiCompatibleBaseURL : void 0),
    geminiApiKey: providerConfig.geminiApiKey ?? (useCurrentProviderFields ? cfg.geminiApiKey : void 0),
    copilotGithubToken: providerConfig.copilotGithubToken ?? (useCurrentProviderFields ? cfg.copilotGithubToken : void 0),
    githubOAuthClientId: providerConfig.githubOAuthClientId ?? (useCurrentProviderFields ? cfg.githubOAuthClientId : void 0)
  };
  if (provider === "ollama") {
    return { ...next, baseURL: next.baseURL ?? DEFAULT_OLLAMA_BASE, model: next.model ?? DEFAULT_OLLAMA_MODEL };
  }
  if (provider === "gemini") {
    return { ...next, baseURL: next.baseURL ?? DEFAULT_GEMINI_BASE, model: next.model ?? DEFAULT_GEMINI_MODEL };
  }
  if (provider === "deepseek") {
    return { ...next, baseURL: next.baseURL ?? DEFAULT_DEEPSEEK_BASE, model: next.model ?? DEFAULT_DEEPSEEK_MODEL };
  }
  return {
    ...next,
    model: next.model ?? (recommendedModel(provider) || defaultModels(provider)[0]?.id)
  };
}
function buildProviderForId(id, cfg) {
  return buildProvider(configForProvider(cfg, id));
}
function buildProvider(cfg) {
  if (isCustomProvider(cfg.provider)) {
    return new OpenAIProvider(
      cfg.openaiCompatibleBaseURL ?? cfg.baseURL ?? "",
      cfg.apiKey ?? "",
      cfg.model ?? "",
      cfg.customProviders?.find((provider) => provider.id === cfg.provider)?.title ?? "Custom provider"
    );
  }
  switch (cfg.provider) {
    case "openai":
      return new OpenAIProvider(
        cfg.baseURL ?? "https://api.openai.com/v1",
        cfg.apiKey ?? "",
        cfg.model ?? "gpt-4o-mini",
        "OpenAI"
      );
    case "openai-compatible":
      return new OpenAIProvider(
        cfg.openaiCompatibleBaseURL ?? cfg.baseURL ?? "https://api.openai.com/v1",
        cfg.apiKey ?? "",
        cfg.model ?? "gpt-4o-mini",
        "OpenAI-compatible provider"
      );
    case "anthropic":
      return new AnthropicProvider(
        cfg.apiKey ?? "",
        cfg.model ?? "claude-3-5-haiku-20241022",
        cfg.baseURL
      );
    case "ollama":
      return new OllamaProvider(cfg.baseURL ?? DEFAULT_OLLAMA_BASE, cfg.model ?? DEFAULT_OLLAMA_MODEL);
    case "copilot":
      return new CopilotProvider(cfg.model ?? "gpt-4o");
    case "gemini":
      return new OpenAIProvider(
        cfg.baseURL ?? DEFAULT_GEMINI_BASE,
        cfg.geminiApiKey ?? cfg.apiKey ?? "",
        cfg.model ?? DEFAULT_GEMINI_MODEL,
        "Gemini"
      );
    case "opencode":
      return new OpenCodeProvider(cfg.model ?? "opencode/big-pickle");
    case "deepseek":
      return new OpenAIProvider(
        cfg.baseURL ?? DEFAULT_DEEPSEEK_BASE,
        cfg.apiKey ?? "",
        cfg.model ?? DEFAULT_DEEPSEEK_MODEL,
        "DeepSeek"
      );
    default:
      return new OllamaProvider(DEFAULT_OLLAMA_BASE, DEFAULT_OLLAMA_MODEL);
  }
}
function invalidateProviderCache() {
  cacheKey = "";
  active = null;
}
function getProvider() {
  const cfg = readLLMConfig();
  const key = JSON.stringify(cfg);
  if (active && key === cacheKey) return active;
  active = buildProvider(cfg);
  cacheKey = key;
  return active;
}
function getProviderForTask(task) {
  const cfg = readLLMConfig();
  const providerOverride = cfg.taskProviderOverrides?.[task];
  const modelOverride = cfg.taskModelOverrides?.[task];
  const targetProvider = providerOverride ?? cfg.provider;
  const targetConfig = configForProvider(cfg, targetProvider);
  const merged = {
    ...targetConfig,
    model: modelOverride ?? targetConfig.model
  };
  return buildProvider(merged);
}
function getSelectedPiModelPattern() {
  const cfg = readLLMConfig();
  const model = cfg.model?.trim();
  if (!model) return void 0;
  const provider = cfg.provider;
  if (provider === "opencode") {
    if (model.startsWith("opencode/opencode/")) return model;
    if (model.startsWith("opencode/")) return `opencode/${model}`;
    return `opencode/opencode/${model}`;
  }
  if (model.startsWith(`${provider}/`)) return model;
  if (model.includes("/")) return model;
  return `${provider}/${model}`;
}
function stripProviderPrefix(model, provider) {
  const prefix = `${provider}/`;
  let normalized = model.trim();
  while (normalized.startsWith(prefix)) {
    normalized = normalized.slice(prefix.length);
  }
  return normalized;
}
function openAiCompatBaseUrl(cfg) {
  if (cfg.provider === "openai") return cfg.baseURL ?? "https://api.openai.com/v1";
  if (cfg.provider === "openai-compatible") return cfg.openaiCompatibleBaseURL ?? cfg.baseURL;
  if (cfg.provider === "gemini") return cfg.baseURL ?? DEFAULT_GEMINI_BASE;
  if (cfg.provider === "deepseek") return cfg.baseURL ?? DEFAULT_DEEPSEEK_BASE;
  if (isCustomProvider(cfg.provider)) return cfg.openaiCompatibleBaseURL ?? cfg.baseURL;
  if (cfg.provider === "ollama") {
    const base = cfg.baseURL ?? DEFAULT_OLLAMA_BASE;
    return base.endsWith("/v1") ? base : `${base.replace(/\/+$/, "")}/v1`;
  }
  return void 0;
}
function piApiKey(cfg) {
  if (cfg.provider === "gemini") return cfg.geminiApiKey ?? cfg.apiKey;
  if (cfg.provider === "ollama") return "ollama";
  return cfg.apiKey;
}
function getSelectedPiProviderBridge() {
  const cfg = readLLMConfig();
  const model = cfg.model?.trim();
  if (!model) return void 0;
  const modelId = stripProviderPrefix(model, cfg.provider);
  if (!modelId) return void 0;
  const selectedModel = cfg.providerModels?.[cfg.provider]?.find(
    (candidate) => stripProviderPrefix(candidate.id, cfg.provider) === modelId
  );
  const modelInput = selectedModel?.capabilities.includes("vision") ? ["text", "image"] : ["text"];
  const isAnthropic = cfg.provider === "anthropic";
  const baseUrl = isAnthropic ? cfg.baseURL ?? "https://api.anthropic.com" : openAiCompatBaseUrl(cfg);
  const apiKey = isAnthropic ? cfg.apiKey : piApiKey(cfg);
  if (!baseUrl || !apiKey) return void 0;
  const providerJson = JSON.stringify({
    baseUrl,
    apiKey,
    api: isAnthropic ? "anthropic-messages" : "openai-completions",
    authHeader: true,
    models: [
      {
        id: modelId,
        name: `Tezbar ${cfg.provider} ${modelId}`,
        reasoning: /reason|think|r1|o\d|gpt-5|claude|deepseek/i.test(modelId),
        input: modelInput,
        cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
        contextWindow: 128e3,
        maxTokens: 8192
      }
    ]
  });
  return {
    modelPattern: `tezbar/${modelId}`,
    providerJson,
    acceptsImages: modelInput.includes("image")
  };
}
var DEFAULT_OLLAMA_BASE, DEFAULT_OLLAMA_MODEL, DEFAULT_GEMINI_BASE, DEFAULT_GEMINI_MODEL, DEFAULT_DEEPSEEK_BASE, DEFAULT_DEEPSEEK_MODEL, cacheKey, active;
var init_registry = __esm({
  "src/main/llm/registry.ts"() {
    "use strict";
    init_aiProviders();
    init_anthropic();
    init_configStore();
    init_copilot();
    init_ollama();
    init_openai();
    init_opencode();
    DEFAULT_OLLAMA_BASE = "http://localhost:11434";
    DEFAULT_OLLAMA_MODEL = "llama3.2";
    DEFAULT_GEMINI_BASE = "https://generativelanguage.googleapis.com/v1beta/openai";
    DEFAULT_GEMINI_MODEL = "gemini-2.0-flash";
    DEFAULT_DEEPSEEK_BASE = "https://api.deepseek.com";
    DEFAULT_DEEPSEEK_MODEL = "deepseek-v4-flash";
    cacheKey = "";
    active = null;
  }
});

// src/main/extension-platform.ts
function normalizePlatform2(value) {
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
  if (!Array.isArray(manifest.platforms)) return [];
  const supported = /* @__PURE__ */ new Set();
  for (const raw of manifest.platforms) {
    const normalized = normalizePlatform2(raw);
    if (normalized) supported.add(normalized);
  }
  return [...supported];
}
function isManifestPlatformCompatible(manifest) {
  const supported = getManifestPlatforms(manifest);
  if (supported.length === 0) return true;
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

// src/main/esbuild-runtime.ts
function configurePackagedEsbuildBinary() {
  if (!app?.isPackaged || process.env.ESBUILD_BINARY_PATH || process.platform !== "darwin") {
    return;
  }
  if (typeof process.resourcesPath !== "string" || !process.resourcesPath) return;
  const packageArch = process.arch === "arm64" ? "darwin-arm64" : "darwin-x64";
  const binaryPath = (0, import_node_path11.join)(
    process.resourcesPath,
    "app.asar.unpacked",
    "node_modules",
    "@esbuild",
    packageArch,
    "bin",
    "esbuild"
  );
  if ((0, import_node_fs11.existsSync)(binaryPath)) {
    process.env.ESBUILD_BINARY_PATH = binaryPath;
  }
}
var import_node_fs11, import_node_path11;
var init_esbuild_runtime = __esm({
  "src/main/esbuild-runtime.ts"() {
    "use strict";
    init_electron_shim();
    import_node_fs11 = require("node:fs");
    import_node_path11 = require("node:path");
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
        const extension = path4.extname(args.path).toLowerCase();
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
function getManagedExtensionsDir() {
  const dir = path4.join(app.getPath("userData"), "extensions");
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  return dir;
}
function getBuildDir(extPath) {
  const dir = path4.join(extPath, ".sc-build");
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  return dir;
}
function expandHome(inputPath) {
  const raw = String(inputPath || "").trim();
  if (!raw) return "";
  if (raw.startsWith("~/")) return path4.join(os.homedir(), raw.slice(2));
  return raw;
}
function normalizeFsPath(inputPath) {
  return path4.resolve(expandHome(inputPath));
}
function normalizeExtensionName(name) {
  const raw = String(name || "").trim();
  if (!raw) return "";
  return raw.replace(/^@/, "").replace(/^raycast\./, "").replace(/[\\/]/g, "-");
}
function getConfiguredExtensionRoots() {
  const settingsPaths = [];
  const envPaths = String(process.env.SUPERCMD_EXTENSION_PATHS || "").split(path4.delimiter).map((value) => value.trim()).filter(Boolean);
  const unique = /* @__PURE__ */ new Set();
  for (const root of [
    getManagedExtensionsDir(),
    path4.join(getManagedExtensionsDir(), "packages"),
    path4.join(app.getPath("userData"), "extension-registry", "packages"),
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
    const pkgPath = path4.join(extPath, "package.json");
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
    const sourceRootPkg = path4.join(sourceRoot, "package.json");
    if (fs.existsSync(sourceRootPkg)) {
      addIfValid(sourceRoot, sourceRoot, path4.basename(sourceRoot));
      continue;
    }
    let entries = [];
    try {
      entries = fs.readdirSync(sourceRoot);
    } catch {
      continue;
    }
    for (const entry of entries) {
      addIfValid(path4.join(sourceRoot, entry), sourceRoot, entry);
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
  const candidates = [
    path4.join(extPath, "assets", iconFile),
    path4.join(extPath, iconFile)
  ];
  for (const p of candidates) {
    if (!fs.existsSync(p)) continue;
    try {
      const ext = path4.extname(p).toLowerCase();
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
    const pkgPath = path4.join(extPath, "package.json");
    const extName = source.extName;
    try {
      const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf-8"));
      if (!isManifestPlatformCompatible(pkg)) continue;
      const iconDataUrl = getExtensionIconDataUrl(
        extPath,
        pkg.icon || "icon.png"
      );
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
          keywords: [
            extName,
            pkg.title || "",
            cmd.name,
            cmd.title || "",
            cmd.description || ""
          ].filter(Boolean).map((s) => s.toLowerCase()),
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
    const pkgPath = path4.join(extPath, "package.json");
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
  const tsconfigPath = path4.join(extPath, "tsconfig.json");
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
    console.warn(`Failed to parse tsconfig for ${path4.basename(extPath)}:`, error?.message || error);
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
  const srcDir = path4.join(extPath, "src");
  const validExt = /\.(tsx?|jsx?)$/i;
  const explicitEntry = typeof cmd?.path === "string" ? cmd.path : typeof cmd?.entrypoint === "string" ? cmd.entrypoint : typeof cmd?.entry === "string" ? cmd.entry : typeof cmd?.file === "string" ? cmd.file : typeof cmd?.source === "string" ? cmd.source : "";
  const candidates = [
    explicitEntry ? path4.join(extPath, explicitEntry) : "",
    path4.join(srcDir, `${cmdName}.tsx`),
    path4.join(srcDir, `${cmdName}.ts`),
    path4.join(srcDir, `${cmdName}.jsx`),
    path4.join(srcDir, `${cmdName}.js`),
    path4.join(srcDir, cmdName, "index.tsx"),
    path4.join(srcDir, cmdName, "index.ts"),
    path4.join(srcDir, cmdName, "index.jsx"),
    path4.join(srcDir, cmdName, "index.js"),
    path4.join(srcDir, "commands", `${cmdName}.tsx`),
    path4.join(srcDir, "commands", `${cmdName}.ts`),
    path4.join(srcDir, "commands", `${cmdName}.jsx`),
    path4.join(srcDir, "commands", `${cmdName}.js`)
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
      const full = path4.join(dir, entry);
      let stat2;
      try {
        stat2 = fs.statSync(full);
      } catch {
        continue;
      }
      if (stat2.isDirectory()) {
        stack.push(full);
        continue;
      }
      if (!validExt.test(entry)) continue;
      const base = path4.basename(entry, path4.extname(entry)).toLowerCase();
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
  const pkgPath = path4.join(extPath, "package.json");
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
  const extNodeModules = path4.join(extPath, "node_modules");
  if (requiresNodeModules && !fs.existsSync(extNodeModules)) {
    try {
      const { installExtensionDeps: installExtensionDeps2 } = (init_extension_registry(), __toCommonJS(extension_registry_exports));
      await installExtensionDeps2(extPath);
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
    const outFile = path4.join(buildDir, `${cmd.name}.js`);
    fs.mkdirSync(path4.dirname(outFile), { recursive: true });
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
          plugins: [
            legacyCheerioInteropPlugin(),
            // Mark swift: imports as external so fakeRequire can handle them at runtime
            {
              name: "swift-external",
              setup(build) {
                build.onResolve({ filter: /^swift:/ }, (args) => ({
                  path: args.path,
                  external: true
                }));
              }
            }
          ],
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
            "global": "globalThis"
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
  const pkgPath = path4.join(extPath, "package.json");
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
  const outFile = path4.join(buildDir, `${cmdName}.js`);
  fs.mkdirSync(path4.dirname(outFile), { recursive: true });
  const extNodeModules = path4.join(extPath, "node_modules");
  if (requiresNodeModules && !fs.existsSync(extNodeModules)) {
    console.log(`  node_modules missing for ${extName}, installing dependencies\u2026`);
    try {
      const { installExtensionDeps: installExtensionDeps2 } = (init_extension_registry(), __toCommonJS(extension_registry_exports));
      await installExtensionDeps2(extPath);
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
        plugins: [
          legacyCheerioInteropPlugin(),
          {
            name: "swift-external",
            setup(build) {
              build.onResolve({ filter: /^swift:/ }, (args) => ({
                path: args.path,
                external: true
              }));
            }
          }
        ],
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
          "global": "globalThis"
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
    console.log(
      `  Missing packages for ${label} (${missing.join(", ")}); installing and retrying\u2026`
    );
    const { installSpecificPackages: installSpecificPackages2 } = (init_extension_registry(), __toCommonJS(extension_registry_exports));
    try {
      await installSpecificPackages2(extPath, missing);
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
  let outFile = path4.join(extPath, ".sc-build", `${cmdName}.js`);
  if (!fs.existsSync(outFile)) {
    console.log(`Pre-built bundle not found for ${normalizedExtName}/${cmdName}, building on-demand\u2026`);
    const built = await buildSingleCommand(normalizedExtName, cmdName);
    if (!built || !fs.existsSync(outFile)) {
      try {
        console.log(`Single-command build failed for ${normalizedExtName}/${cmdName}; trying full extension rebuild\u2026`);
        await buildAllCommands(normalizedExtName);
      } catch (rebuildError) {
        console.warn(`Full rebuild fallback failed for ${normalizedExtName}:`, rebuildError);
      }
    }
    if (!fs.existsSync(outFile)) {
      let diagnostic = "";
      try {
        const pkgPath = path4.join(extPath, "package.json");
        const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf-8"));
        const commands = Array.isArray(pkg?.commands) ? pkg.commands : [];
        const cmd = commands.find((c) => c?.name === cmdName);
        const nodeModulesExists = fs.existsSync(path4.join(extPath, "node_modules"));
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
    const pkgPath = path4.join(extPath, "package.json");
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
  const assetsPath = path4.join(extPath, "assets");
  const supportPath = path4.join(app.getPath("userData"), "extension-support", normalizedExtName);
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
var fs, os, path4, nodeBuiltins, lastBuildError;
var init_extension_builder = __esm({
  "src/main/extension-builder.ts"() {
    "use strict";
    init_electron_shim();
    fs = __toESM(require("fs"));
    os = __toESM(require("os"));
    path4 = __toESM(require("path"));
    init_extension_platform();
    init_esbuild_runtime();
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
  return new Promise((resolve4, reject) => {
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
          resolve4(JSON.parse(rawBody));
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
  return "";
}
function getBunDir() {
  return path5.join(app.getPath("userData"), "bun");
}
function getBunBinaryPath() {
  return path5.join(getBunDir(), "bun");
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
    const tmpZipPath = path5.join(app.getPath("temp"), `bun-${Date.now()}.zip`);
    fs2.writeFileSync(tmpZipPath, zipBuffer);
    const tmpExtractDir = path5.join(app.getPath("temp"), `bun-extract-${Date.now()}`);
    fs2.mkdirSync(tmpExtractDir, { recursive: true });
    await execAsync(`unzip -o "${tmpZipPath}" -d "${tmpExtractDir}"`, {
      timeout: 3e4
    });
    const bunBinary = findFile(tmpExtractDir, "bun");
    if (!bunBinary) {
      throw new Error("Bun binary not found in downloaded archive");
    }
    const destPath = getBunBinaryPath();
    fs2.copyFileSync(bunBinary, destPath);
    fs2.chmodSync(destPath, 493);
    try {
      fs2.rmSync(tmpZipPath, { force: true });
    } catch {
    }
    try {
      fs2.rmSync(tmpExtractDir, { recursive: true, force: true });
    } catch {
    }
    const { stdout } = await execAsync(`"${destPath}" --version`, { timeout: 5e3 });
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
  const pkgPath = path5.join(extPath, "package.json");
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
    console.log(`No third-party deps for ${path5.basename(extPath)} \u2014 skipping bun install`);
    return true;
  }
  console.log(`Installing ${thirdPartyDeps.length} deps via Bun for ${path5.basename(extPath)}...`);
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
    for (const lockfile of ["package-lock.json", "bun.lockb", "bun.lock", "yarn.lock", "pnpm-lock.yaml"]) {
      try {
        fs2.rmSync(path5.join(extPath, lockfile), { force: true });
      } catch {
      }
    }
    await execAsync(`"${bunPath}" install --production --no-save`, {
      cwd: extPath,
      timeout: 12e4,
      env: {
        ...process.env,
        PATH: `${path5.dirname(bunPath)}:${process.env.PATH || ""}`
      }
    });
    fs2.writeFileSync(pkgPath, originalPkg);
    const hasNodeModules2 = fs2.existsSync(path5.join(extPath, "node_modules"));
    if (hasNodeModules2) {
      console.log(`Bun install succeeded for ${path5.basename(extPath)}`);
      return true;
    }
    console.warn(`Bun completed but node_modules missing for ${path5.basename(extPath)}`);
    fs2.writeFileSync(pkgPath, originalPkg);
    return false;
  } catch (error) {
    console.warn(`Bun install failed for ${path5.basename(extPath)}:`, error?.message);
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
  console.log(`Installing specific packages via Bun for ${path5.basename(extPath)}: ${unique.join(", ")}`);
  try {
    await execFileAsync6(bunPath, ["add", "--no-save", ...unique], {
      cwd: extPath,
      timeout: 3e5,
      env: {
        ...process.env,
        PATH: `${path5.dirname(bunPath)}:${process.env.PATH || ""}`
      }
    });
    return fs2.existsSync(path5.join(extPath, "node_modules"));
  } catch (error) {
    console.warn(`Bun add failed for ${path5.basename(extPath)}:`, error?.message);
    return false;
  }
}
function downloadFile(url) {
  return new Promise((resolve4, reject) => {
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
        res.on("end", () => resolve4(Buffer.concat(chunks)));
      }).on("error", reject);
    };
    makeRequest(url);
  });
}
function findFile(dir, name) {
  try {
    const entries = fs2.readdirSync(dir, { withFileTypes: true });
    for (const entry of entries) {
      const full = path5.join(dir, entry.name);
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
var import_child_process, import_util, fs2, path5, https2, http2, execAsync, execFileAsync6, BUN_VERSION;
var init_bun_manager = __esm({
  "src/main/bun-manager.ts"() {
    "use strict";
    init_electron_shim();
    import_child_process = require("child_process");
    import_util = require("util");
    fs2 = __toESM(require("fs"));
    path5 = __toESM(require("path"));
    https2 = __toESM(require("https"));
    http2 = __toESM(require("http"));
    execAsync = (0, import_util.promisify)(import_child_process.exec);
    execFileAsync6 = (0, import_util.promisify)(import_child_process.execFile);
    BUN_VERSION = "1.2.5";
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
  installExtension: () => installExtension2,
  installExtensionDeps: () => installExtensionDeps,
  installRegistryExtension: () => installRegistryExtension,
  installSpecificPackages: () => installSpecificPackages,
  isExtensionInstalled: () => isExtensionInstalled,
  listInstalledExtensionSlugsFromDisk: () => listInstalledExtensionSlugsFromDisk,
  listInstalledRegistryExtensions: () => listInstalledRegistryExtensions,
  resolveInstalledPackageJsonPath: () => resolveInstalledPackageJsonPath,
  saveExtensionPreferences: () => saveExtensionPreferences,
  searchExtensionCatalog: () => searchExtensionCatalog,
  shouldShowExtensionPreferenceSetup: () => shouldShowExtensionPreferenceSetup,
  uninstallExtension: () => uninstallExtension2,
  uninstallRegistryExtension: () => uninstallRegistryExtension
});
function hasNodeModules(extPath) {
  try {
    return fs3.existsSync(path6.join(extPath, "node_modules"));
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
  const response = await fetchWithTimeout(
    GITHUB_TREE_API,
    { headers: githubApiHeaders() },
    9e4
  );
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
  const fileEntries = treeEntries.filter(
    (entry) => entry.type === "blob" && entry.path.startsWith(prefix)
  );
  if (fileEntries.length === 0) return null;
  const srcDir = path6.join(tmpDir, "extensions", name);
  fs3.mkdirSync(srcDir, { recursive: true });
  for (const entry of fileEntries) {
    const relativePath = entry.path.slice(prefix.length);
    if (!relativePath) continue;
    const destination = path6.join(srcDir, relativePath);
    fs3.mkdirSync(path6.dirname(destination), { recursive: true });
  }
  const CONCURRENCY = 30;
  let index = 0;
  const downloadOne = async () => {
    while (index < fileEntries.length) {
      const i = index++;
      const entry = fileEntries[i];
      const relativePath = entry.path.slice(prefix.length);
      if (!relativePath) continue;
      const destination = path6.join(srcDir, relativePath);
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
  const workers = Array.from(
    { length: Math.min(CONCURRENCY, fileEntries.length) },
    () => downloadOne()
  );
  await Promise.all(workers);
  console.log(`Downloaded ${fileEntries.length} files for "${name}"`);
  return srcDir;
}
function downloadExtensionViaSparseGit(name, tmpDir) {
  const checkoutDir = path6.join(tmpDir, "raymes-extensions-source");
  try {
    (0, import_child_process2.execFileSync)("git", [
      "clone",
      "--depth",
      "1",
      "--filter=blob:none",
      "--sparse",
      RAYMES_EXTENSIONS_GIT,
      checkoutDir
    ], { stdio: "ignore", timeout: 18e4 });
    (0, import_child_process2.execFileSync)("git", [
      "-C",
      checkoutDir,
      "sparse-checkout",
      "set",
      `extensions/${name}`
    ], { stdio: "ignore", timeout: 12e4 });
    const extensionDir = path6.join(checkoutDir, "extensions", name);
    return fs3.existsSync(path6.join(extensionDir, "package.json")) ? extensionDir : null;
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
    commands
  };
}
function getCatalogPath() {
  return path6.join(app.getPath("userData"), "extension-catalog.json");
}
function getExtensionsDir() {
  const dir = path6.join(app.getPath("userData"), "extensions");
  if (!fs3.existsSync(dir)) {
    fs3.mkdirSync(dir, { recursive: true });
  }
  return dir;
}
function getInstalledPath(name) {
  return path6.join(getExtensionsDir(), name);
}
function getLegacyInstalledPath(name) {
  const slug = slugFromRaymesExtensionId(name);
  return path6.join(getExtensionsDir(), "packages", normalizeRaymesExtensionId(slug));
}
function getLegacyRegistryInstalledPath(name) {
  const slug = slugFromRaymesExtensionId(name);
  return path6.join(app.getPath("userData"), "extension-registry", "packages", normalizeRaymesExtensionId(slug));
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
    if (fs3.existsSync(path6.join(candidate, "package.json"))) return candidate;
  }
  return null;
}
function loadCatalogFromDisk() {
  try {
    const data = fs3.readFileSync(getCatalogPath(), "utf-8");
    const parsed = JSON.parse(data);
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
async function getCatalog(forceRefresh = false) {
  if (!forceRefresh && catalogCache2 && Date.now() - catalogCache2.fetchedAt < CATALOG_TTL) {
    return catalogCache2.entries;
  }
  if (!forceRefresh) {
    const diskCache2 = loadCatalogFromDisk();
    if (diskCache2 && Date.now() - diskCache2.fetchedAt < CATALOG_TTL) {
      catalogCache2 = diskCache2;
      return diskCache2.entries;
    }
  }
  try {
    console.log("Fetching extension catalog from API\u2026");
    const entries = await fetchCatalogFromAPI();
    const cache2 = {
      entries,
      fetchedAt: Date.now(),
      version: CATALOG_VERSION
    };
    catalogCache2 = cache2;
    saveCatalogToDisk(cache2);
    console.log(`Extension catalog (API): ${entries.length} extensions cached.`);
    return entries;
  } catch (apiError) {
    console.warn("API catalog fetch failed:", apiError?.message || apiError);
  }
  const diskCache = loadCatalogFromDisk();
  if (diskCache) {
    catalogCache2 = diskCache;
    console.log(`Extension catalog (disk cache): ${diskCache.entries.length} extensions from cache.`);
    return diskCache.entries;
  }
  return [];
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
  const unique = Array.from(
    new Set(
      packageNames.map((name) => String(name || "").trim()).filter(Boolean)
    )
  );
  if (unique.length === 0) return;
  console.log(
    `Installing missing packages for ${path6.basename(extPath)}: ${unique.join(", ")}`
  );
  const ok = await installSpecificPackagesWithBun(extPath, unique);
  if (!ok) {
    throw new Error(`Bun failed to install packages for ${path6.basename(extPath)}`);
  }
}
async function installExtensionDeps(extPath) {
  const pkgPath = path6.join(extPath, "package.json");
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
    console.log(`No third-party dependencies for ${path6.basename(extPath)}`);
    return;
  }
  console.log(
    `Installing ${thirdPartyDeps.length} dependencies for ${path6.basename(extPath)}: ${thirdPartyDeps.join(", ")}`
  );
  const ok = await installDepsWithBun(extPath);
  if (!ok) {
    throw new Error(`Bun dependency installation failed for ${path6.basename(extPath)}`);
  }
  if (!hasNodeModules(extPath)) {
    throw new Error("Bun completed but node_modules is still missing");
  }
  console.log(`Dependencies installed for ${path6.basename(extPath)}`);
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
        const p = path6.join(root, d);
        if (fs3.statSync(p).isDirectory() && fs3.existsSync(path6.join(p, "package.json"))) {
          names.add(stripRaycastPrefix ? slugFromRaymesExtensionId(d) : d);
        }
      }
    } catch {
    }
  };
  scanRoot(getExtensionsDir(), false);
  scanRoot(path6.join(getExtensionsDir(), "packages"), true);
  scanRoot(path6.join(app.getPath("userData"), "extension-registry", "packages"), true);
  return [...names].filter(Boolean).sort((a, b) => a.localeCompare(b));
}
async function installExtension2(name) {
  if (!/^[A-Za-z0-9._-]+$/.test(String(name || ""))) {
    console.error(`Invalid extension name: "${name}"`);
    return false;
  }
  try {
    const success = await installExtensionFromBundle(name);
    if (success) return true;
  } catch (bundleError) {
    console.warn(`Bundle install failed for "${name}":`, bundleError?.message || bundleError);
  }
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
  const backupPath = hadExistingInstall ? path6.join(getExtensionsDir(), `${name}.backup-${Date.now()}`) : "";
  const tmpDir = path6.join(app.getPath("temp"), `supercmd-bundle-${Date.now()}`);
  try {
    const t0 = Date.now();
    const { url } = await getExtensionBundleUrl(name);
    console.log(`Downloading pre-built bundle for "${name}"\u2026`);
    fs3.mkdirSync(tmpDir, { recursive: true });
    await downloadAndExtractTarball(url, tmpDir);
    const nestedPath = path6.join(tmpDir, name);
    let srcDir = tmpDir;
    if (fs3.existsSync(path6.join(nestedPath, "package.json"))) {
      srcDir = nestedPath;
    } else if (!fs3.existsSync(path6.join(srcDir, "package.json"))) {
      const subdirs = fs3.readdirSync(tmpDir, { withFileTypes: true }).filter((d) => d.isDirectory());
      for (const sub of subdirs) {
        if (fs3.existsSync(path6.join(tmpDir, sub.name, "package.json"))) {
          srcDir = path6.join(tmpDir, sub.name);
          break;
        }
      }
    }
    if (!fs3.existsSync(path6.join(srcDir, "package.json"))) {
      throw new Error("Bundle has no package.json");
    }
    if (!fs3.existsSync(path6.join(srcDir, ".sc-build"))) {
      throw new Error("Bundle has no .sc-build/ directory \u2014 not a pre-built bundle");
    }
    const bundleManifest = JSON.parse(fs3.readFileSync(path6.join(srcDir, "package.json"), "utf8"));
    const missingCommandBundles = (Array.isArray(bundleManifest.commands) ? bundleManifest.commands : []).filter((command) => command?.name).filter((command) => !fs3.existsSync(path6.join(srcDir, ".sc-build", `${command.name}.js`))).map((command) => command.name);
    if (missingCommandBundles.length > 0) {
      throw new Error(`Bundle is incomplete; missing commands: ${missingCommandBundles.join(", ")}`);
    }
    if (hadExistingInstall) {
      fs3.renameSync(installPath, backupPath);
    }
    fs3.cpSync(srcDir, installPath, { recursive: true });
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
  const backupPath = hadExistingInstall ? path6.join(getExtensionsDir(), `${name}.backup-${Date.now()}`) : "";
  const tmpDir = path6.join(app.getPath("temp"), `supercmd-api-install-${Date.now()}`);
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
    if (!srcDir || !fs3.existsSync(path6.join(srcDir, "package.json"))) {
      throw new Error(`Extension "${name}" not found or has no package.json`);
    }
    const srcPkg = JSON.parse(fs3.readFileSync(path6.join(srcDir, "package.json"), "utf-8"));
    if (!isManifestPlatformCompatible(srcPkg)) {
      const supported = getManifestPlatforms(srcPkg);
      console.error(`Extension "${name}" is not compatible with ${getCurrentRaycastPlatform()} (supports: ${supported.join(", ")})`);
      return false;
    }
    if (hadExistingInstall) {
      fs3.renameSync(installPath, backupPath);
    }
    fs3.cpSync(srcDir, installPath, { recursive: true });
    {
      const extPkg = JSON.parse(fs3.readFileSync(path6.join(installPath, "package.json"), "utf-8"));
      const allDeps = { ...extPkg.dependencies || {}, ...extPkg.optionalDependencies || {} };
      const thirdPartyDeps = Object.keys(allDeps).filter((d) => !d.startsWith("@raycast/"));
      if (thirdPartyDeps.length === 0) {
        console.log(`No third-party dependencies for "${name}" \u2014 skipping install`);
      } else {
        const depsInstalled = await installDepsWithBun(installPath);
        if (!depsInstalled) {
          console.warn(`Could not install deps for "${name}" \u2014 extension may not work fully.`);
        }
      }
      const t1 = Date.now();
      console.log(`  Deps: ${t1 - t0}ms. Pre-building commands for "${name}"\u2026`);
      const { buildAllCommands: buildAllCommands2 } = (init_extension_builder(), __toCommonJS(extension_builder_exports));
      const builtCount = await buildAllCommands2(name);
      const expectedCount = (Array.isArray(extPkg.commands) ? extPkg.commands : []).filter((command) => command?.name).length;
      if (builtCount < expectedCount) {
        throw new Error(`Built ${builtCount}/${expectedCount} commands for "${name}"`);
      }
      console.log(`  Build: ${Date.now() - t1}ms. Extension "${name}" installed (${builtCount} commands) in ${Date.now() - t0}ms total`);
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
async function downloadAndExtractTarball(url, destDir) {
  return new Promise((resolve4, reject) => {
    const makeRequest = (requestUrl, redirectCount = 0) => {
      if (redirectCount > 5) {
        reject(new Error("Too many redirects"));
        return;
      }
      const parsedUrl = new URL(requestUrl);
      const isHttps = parsedUrl.protocol === "https:";
      const transport = isHttps ? require("https") : require("http");
      transport.get(requestUrl, { timeout: 12e4 }, (res) => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          makeRequest(res.headers.location, redirectCount + 1);
          return;
        }
        if (res.statusCode !== 200) {
          reject(new Error(`Download failed: ${res.statusCode} ${res.statusMessage}`));
          return;
        }
        const chunks = [];
        res.on("data", (chunk) => chunks.push(chunk));
        res.on("error", reject);
        res.on("end", () => {
          try {
            const buffer = Buffer.concat(chunks);
            extractTarGz(buffer, destDir);
            resolve4();
          } catch (err) {
            reject(err);
          }
        });
      }).on("error", reject);
    };
    makeRequest(url);
  });
}
function extractTarGz(buffer, destDir) {
  const decompressed = zlib.gunzipSync(buffer);
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
      const dirPath = path6.join(destDir, fullName);
      fs3.mkdirSync(dirPath, { recursive: true });
    } else if (typeFlag === 0 || typeFlag === 48) {
      const filePath = path6.join(destDir, fullName);
      fs3.mkdirSync(path6.dirname(filePath), { recursive: true });
      const fileData = decompressed.subarray(offset, offset + size);
      fs3.writeFileSync(filePath, fileData);
    }
    const dataBlocks = Math.ceil(size / 512);
    offset += dataBlocks * 512;
  }
}
function getMachineId() {
  if (_machineId) return _machineId;
  const idPath = path6.join(app.getPath("userData"), ".machine-id");
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
async function uninstallExtension2(name) {
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
  const pkgPath = extensionPath ? path6.join(extensionPath, "package.json") : "";
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
  const candidates = [
    path6.join(extensionPath, normalized),
    path6.join(extensionPath, "assets", normalized)
  ];
  return candidates.find((candidate) => fs3.existsSync(candidate));
}
function readAppBundleIdentifier(appPath) {
  const infoPlistPath = path6.join(appPath, "Contents", "Info.plist");
  if (!fs3.existsSync(infoPlistPath)) return void 0;
  try {
    return (0, import_child_process2.execFileSync)("/usr/bin/plutil", ["-extract", "CFBundleIdentifier", "raw", "-o", "-", infoPlistPath], {
      encoding: "utf8",
      timeout: 1e3
    }).trim() || void 0;
  } catch {
    return void 0;
  }
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
      packageJsonPath: path6.join(extensionPath, "package.json"),
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
  const pkgPath = extensionPath ? path6.join(extensionPath, "package.json") : "";
  return fs3.existsSync(pkgPath) ? pkgPath : null;
}
async function searchExtensionCatalog(query) {
  const q = String(query || "").trim().toLowerCase();
  const catalog = await getCatalog(false);
  return catalog.filter((entry) => {
    if (!q) return true;
    return [
      entry.name,
      entry.title,
      entry.description,
      entry.author,
      ...entry.categories || []
    ].join(" ").toLowerCase().includes(q);
  }).slice(0, 200).map((entry) => ({
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
}
async function installRegistryExtension(extensionIdOrSlug) {
  const slug = slugFromRaymesExtensionId(extensionIdOrSlug);
  if (!slug) throw new Error("A valid extension id is required");
  extensionRegistryEvents.emit("progress", { id: normalizeRaymesExtensionId(slug), progress: 5 });
  const ok = await installExtension2(slug);
  extensionRegistryEvents.emit("progress", { id: normalizeRaymesExtensionId(slug), progress: ok ? 100 : 0 });
  if (!ok) throw new Error(`Failed to install extension: ${slug}`);
  const installed = listInstalledRegistryExtensions().find((entry) => entry.slug === slug);
  if (!installed) throw new Error(`Extension installed but could not be loaded: ${slug}`);
  return installed;
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
function getExtensionPreferences(extensionId, commandName2) {
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
  const command = Array.isArray(pkg.commands) ? pkg.commands.find((cmd) => cmd?.name === commandName2) : null;
  applyDefaults(Array.isArray(command?.preferences) ? command.preferences : []);
  const extensionPath = resolveInstalledExtensionPathForRaymes(slug) || getInstalledPath(slug);
  const preferencesPath = path6.join(extensionPath, "preferences.json");
  if (fs3.existsSync(preferencesPath)) {
    try {
      const saved = JSON.parse(fs3.readFileSync(preferencesPath, "utf-8"));
      if (saved && typeof saved === "object") {
        Object.assign(values, saved);
        if (commandName2 && saved.commands?.[commandName2]) {
          Object.assign(values, saved.commands[commandName2]);
        }
      }
    } catch {
    }
  }
  return values;
}
function getExtensionPreferenceSetup(extensionId, commandName2) {
  const slug = slugFromRaymesExtensionId(extensionId);
  const pkg = readInstalledPackage(slug);
  const extensionPath = resolveInstalledExtensionPathForRaymes(slug) || getInstalledPath(slug);
  const command = Array.isArray(pkg.commands) ? pkg.commands.find((cmd) => cmd?.name === commandName2) : null;
  const extensionPreferences = Array.isArray(pkg.preferences) ? pkg.preferences.map((preference) => ({ ...preference })) : [];
  const commandPreferences = Array.isArray(command?.preferences) ? command.preferences.map((preference) => ({
    ...preference,
    commandName: commandName2,
    commandTitle: String(command?.title || commandName2)
  })) : [];
  const preferences = [...extensionPreferences, ...commandPreferences];
  const preferencesPath = path6.join(extensionPath, "preferences.json");
  return {
    extensionId: normalizeRaymesExtensionId(slug),
    commandName: commandName2,
    title: String(pkg.title || extensionNameFromSlug(slug)),
    iconPath: resolveInstalledIconPath(extensionPath, pkg.icon || "icon.png") || void 0,
    preferences,
    values: getExtensionPreferences(extensionId, commandName2),
    hasSavedPreferences: fs3.existsSync(preferencesPath)
  };
}
function shouldShowExtensionPreferenceSetup(extensionId, commandName2) {
  const setup = getExtensionPreferenceSetup(extensionId, commandName2);
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
function saveExtensionPreferences(extensionId, values, commandName2) {
  const slug = slugFromRaymesExtensionId(extensionId);
  const extensionPath = resolveInstalledExtensionPathForRaymes(slug) || getInstalledPath(slug);
  fs3.mkdirSync(extensionPath, { recursive: true });
  const preferencesPath = path6.join(extensionPath, "preferences.json");
  let existing = {};
  if (fs3.existsSync(preferencesPath)) {
    try {
      const parsed = JSON.parse(fs3.readFileSync(preferencesPath, "utf-8"));
      if (parsed && typeof parsed === "object") existing = parsed;
    } catch {
    }
  }
  if (commandName2) {
    existing.commands = existing.commands && typeof existing.commands === "object" ? existing.commands : {};
    existing.commands[commandName2] = {
      ...existing.commands[commandName2] || {},
      ...values
    };
  } else {
    existing = {
      ...existing,
      ...values
    };
  }
  fs3.writeFileSync(preferencesPath, JSON.stringify(existing, null, 2));
  return getExtensionPreferences(extensionId, commandName2);
}
var import_child_process2, import_events2, fs3, path6, zlib, extensionRegistryEvents, GITHUB_RAW, GITHUB_API, GITHUB_TREE_API, RAYMES_EXTENSIONS_GIT, REPO_TREE_TTL_MS, repoTreeCache, CATALOG_VERSION, CATALOG_TTL, catalogCache2, _machineId;
var init_extension_registry = __esm({
  "src/main/extension-registry.ts"() {
    "use strict";
    init_electron_shim();
    import_child_process2 = require("child_process");
    import_events2 = require("events");
    fs3 = __toESM(require("fs"));
    path6 = __toESM(require("path"));
    zlib = __toESM(require("zlib"));
    init_extension_platform();
    init_extension_builder();
    init_extension_api();
    init_bun_manager();
    extensionRegistryEvents = new import_events2.EventEmitter();
    GITHUB_RAW = "https://raw.githubusercontent.com/raycast/extensions/main";
    GITHUB_API = "https://api.github.com/repos/raycast/extensions/contents";
    GITHUB_TREE_API = "https://api.github.com/repos/raycast/extensions/git/trees/main?recursive=1";
    RAYMES_EXTENSIONS_GIT = "https://github.com/almatkai/raymes-extensions.git";
    REPO_TREE_TTL_MS = 10 * 60 * 1e3;
    repoTreeCache = null;
    CATALOG_VERSION = 6;
    CATALOG_TTL = 24 * 60 * 60 * 1e3;
    catalogCache2 = null;
    _machineId = null;
  }
});

// src/main/llm/extensionAI.ts
async function askExtensionAI(prompt) {
  const normalizedPrompt = String(prompt || "").trim();
  if (!normalizedPrompt) throw new Error("AI prompt is required");
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), EXTENSION_AI_TIMEOUT_MS);
  const messages = [
    { role: "system", content: EXTENSION_AI_SYSTEM },
    { role: "user", content: normalizedPrompt }
  ];
  try {
    const { getProviderForTask: getProviderForTask2 } = await Promise.resolve().then(() => (init_registry(), registry_exports));
    const provider = getProviderForTask2("action");
    if (!await provider.isAvailable()) {
      throw new Error("The configured AI provider is unavailable");
    }
    const stream = await provider.chat(messages, void 0, { signal: controller.signal });
    let answer = "";
    for await (const delta of stream) {
      if (delta.text) answer += delta.text;
    }
    return answer.trim();
  } catch (error) {
    if (controller.signal.aborted) {
      throw new Error("Extension AI request timed out");
    }
    throw error;
  } finally {
    clearTimeout(timeout);
  }
}
var EXTENSION_AI_TIMEOUT_MS, EXTENSION_AI_SYSTEM;
var init_extensionAI = __esm({
  "src/main/llm/extensionAI.ts"() {
    "use strict";
    EXTENSION_AI_TIMEOUT_MS = 6e4;
    EXTENSION_AI_SYSTEM = "You are answering an AI request from a Raycast-compatible extension. Return only the requested result, without commentary or tool use.";
  }
});

// src/main/extension-runner.ts
var extension_runner_exports = {};
__export(extension_runner_exports, {
  clearAllExtensionSessions: () => clearAllExtensionSessions,
  disposeExtensionSession: () => disposeExtensionSession,
  invokeExtensionAction: () => invokeExtensionAction,
  loadMoreExtensionSession: () => loadMoreExtensionSession,
  refreshExtensionSession: () => refreshExtensionSession,
  runExtensionCommand: () => runExtensionCommand,
  runExtensionCommandFromPackageJson: () => runExtensionCommandFromPackageJson,
  updateSearchText: () => updateSearchText
});
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
function makeId2(prefix) {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}
function delay(ms) {
  return new Promise((resolve4) => setTimeout(resolve4, ms));
}
function elapsedMs(startedAt) {
  return `${Date.now() - startedAt}ms`;
}
function hookDepsEqual(previous, next) {
  if (!previous || !next || previous.length !== next.length) return false;
  return next.every((value, index) => Object.is(value, previous[index]));
}
function flushPendingEffects(session2) {
  const effects = session2.pendingEffects.splice(0);
  for (const { idx, sideEffect, deps, label } of effects) {
    const previousCleanup = session2.effectCleanups.get(idx);
    if (previousCleanup) {
      try {
        previousCleanup();
      } catch (error) {
        console.error(`[${label}] cleanup threw:`, error);
      }
      session2.effectCleanups.delete(idx);
    }
    try {
      const cleanup2 = sideEffect();
      session2.effectDeps.set(idx, deps);
      if (typeof cleanup2 === "function") session2.effectCleanups.set(idx, cleanup2);
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
function promiseResultCachePath(session2, key) {
  const digest = (0, import_node_crypto5.createHash)("sha256").update(session2.bundledCode).update("\0").update(session2.extensionId).update("\0").update(session2.commandName).update("\0").update(key).digest("hex");
  return (0, import_node_path12.join)(session2.packageRoot, ".tezbar-runtime-cache", `${digest}.bin.gz`);
}
function readPromiseResultCache(session2, key) {
  const memoryKey = `${session2.extensionId}/${session2.commandName}:${key}`;
  const memoryEntry = promiseResultMemoryCache.get(memoryKey);
  if (memoryEntry && Date.now() - memoryEntry.cachedAt <= PROMISE_RESULT_CACHE_TTL_MS) {
    return memoryEntry;
  }
  const cachePath = promiseResultCachePath(session2, key);
  try {
    const stats = (0, import_node_fs12.statSync)(cachePath);
    if (Date.now() - stats.mtimeMs > PROMISE_RESULT_CACHE_TTL_MS) return null;
    const compressed = (0, import_node_fs12.readFileSync)(cachePath);
    const payload = (0, import_node_v8.deserialize)((0, import_node_zlib.gunzipSync)(compressed));
    setPromiseResultMemoryCache(memoryKey, payload);
    console.log(
      `[usePromise] Persistent cache hit ${session2.extensionId}/${session2.commandName}; bytes=${compressed.byteLength}`
    );
    return payload;
  } catch {
    return null;
  }
}
function writePromiseResultCache(session2, key, data) {
  if (data === void 0 || session2.disposed) return;
  const cachedAt = Date.now();
  const memoryKey = `${session2.extensionId}/${session2.commandName}:${key}`;
  setPromiseResultMemoryCache(memoryKey, { data, cachedAt });
  const cachePath = promiseResultCachePath(session2, key);
  void (async () => {
    const startedAt = Date.now();
    try {
      const encoded = (0, import_node_v8.serialize)({ data, cachedAt });
      const compressed = await gzipAsync(encoded);
      (0, import_node_fs12.mkdirSync)((0, import_node_path12.dirname)(cachePath), { recursive: true });
      await (0, import_promises2.writeFile)(cachePath, compressed);
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
async function recoverIncompleteChunkedCache(session2, error, promiseKey) {
  if (!(error instanceof Error) || session2.cacheRecoveryKeys.has(promiseKey)) return false;
  const missingIndex = error.message.match(
    /ENOENT:.*open ['"]([^'"]+)[/\\]([^/\\]+)[/\\]index\.json['"]/
  );
  if (!missingIndex) return false;
  const indexPath = missingIndex[1];
  const cacheName = missingIndex[2];
  if (!indexPath || !cacheName) return false;
  const supportRoot = (0, import_node_path12.join)(session2.packageRoot, ".tezbar-support");
  const chunkDirectory = (0, import_node_path12.join)(indexPath, cacheName);
  const sourcePath = (0, import_node_path12.join)(indexPath, `${cacheName}.json`);
  if ((0, import_node_path12.dirname)(sourcePath) !== supportRoot || (0, import_node_path12.dirname)(chunkDirectory) !== supportRoot) {
    return false;
  }
  let handle = null;
  try {
    handle = await (0, import_promises2.open)(sourcePath, "r");
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
  session2.cacheRecoveryKeys.add(promiseKey);
  await Promise.all([
    (0, import_promises2.rm)(sourcePath, { force: true }),
    (0, import_promises2.rm)(chunkDirectory, { recursive: true, force: true })
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
async function runAppleScript2(source) {
  if (process.platform !== "darwin") {
    throw new Error("AppleScript is only available on macOS");
  }
  if (typeof source !== "string" || source.trim().length === 0) {
    return "";
  }
  const { stdout } = await execFileAsync7("/usr/bin/osascript", ["-e", source], {
    encoding: "utf8",
    maxBuffer: 10 * 1024 * 1024
  });
  return String(stdout).replace(/\r?\n$/, "");
}
async function runAppleScriptForSession(session2, source) {
  pushEffect(session2, { kind: "apple-script", value: String(source ?? "").slice(0, 2e3) });
  if (session2.effectMode === "record") return "";
  return runAppleScript2(source);
}
function nativeColorPickerBinaryPath() {
  return (0, import_node_path12.join)(app.getPath("userData"), "native", "color-picker");
}
function nativeColorPickerSourcePath() {
  const candidates = [
    (0, import_node_path12.join)(process.cwd(), "native", "color-picker", "main.swift"),
    (0, import_node_path12.join)(process.cwd(), "src", "native", "color-picker.swift"),
    (0, import_node_path12.join)(app.getAppPath(), "native", "color-picker", "main.swift"),
    (0, import_node_path12.join)(app.getAppPath(), "src", "native", "color-picker.swift")
  ];
  return candidates.find((candidate) => (0, import_node_fs12.existsSync)(candidate)) ?? null;
}
async function ensureNativeColorPickerBinary() {
  const binaryPath = nativeColorPickerBinaryPath();
  if ((0, import_node_fs12.existsSync)(binaryPath)) return binaryPath;
  const sourcePath = nativeColorPickerSourcePath();
  if (!sourcePath) return null;
  const moduleCachePath = (0, import_node_path12.join)((0, import_node_path12.dirname)(binaryPath), "swift-module-cache");
  (0, import_node_fs12.mkdirSync)((0, import_node_path12.dirname)(binaryPath), { recursive: true });
  (0, import_node_fs12.mkdirSync)(moduleCachePath, { recursive: true });
  try {
    await execFileAsync7("/usr/bin/swiftc", [
      "-module-cache-path",
      moduleCachePath,
      "-O",
      "-o",
      binaryPath,
      sourcePath,
      "-framework",
      "AppKit"
    ]);
    return (0, import_node_fs12.existsSync)(binaryPath) ? binaryPath : null;
  } catch (error) {
    console.error("[ColorPicker] Failed to compile native helper:", error);
    return null;
  }
}
async function pickColorWithNativeSampler() {
  const visibleWindows = BrowserWindow.getAllWindows().filter((window2) => window2.isVisible());
  try {
    const binaryPath = await ensureNativeColorPickerBinary();
    if (!binaryPath) {
      return null;
    }
    setSuppressBlurHide(true);
    for (const window2 of visibleWindows) {
      window2.hide();
    }
    app.hide();
    await delay(80);
    const { stdout } = await execFileAsync7(binaryPath);
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
  } catch {
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
function screenOcrHelperPath2() {
  if (process.env.SCREENOCR_HELPER_PATH) return process.env.SCREENOCR_HELPER_PATH;
  if (app?.isPackaged) {
    const resourcesPath = process.resourcesPath;
    if (resourcesPath) {
      return (0, import_node_path12.join)(resourcesPath, "app.asar.unpacked", "native", "screenocr", "screenocr-helper");
    }
  }
  return (0, import_node_path12.join)(process.cwd(), "native", "screenocr", "screenocr-helper");
}
async function runScreenOcrHelper(command, values) {
  const helperPath = screenOcrHelperPath2();
  if (!(0, import_node_fs12.existsSync)(helperPath)) {
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
    const { stdout } = await execFileAsync7(helperPath, [command, JSON.stringify(values)], {
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
function attachRuntimeRootMetadata(root, session2) {
  root.props = {
    ...root.props ?? {},
    assetsPath: (0, import_node_path12.join)(session2.packageRoot, "assets")
  };
  if (typeof root.props.markdown === "string") {
    root.props.markdown = resolveExtensionMarkdownAssets(root.props.markdown, session2.packageRoot);
  }
}
function buildPreferenceSetupRoot(extensionId, commandName2) {
  const setup = getExtensionPreferenceSetup(extensionId, commandName2);
  return {
    type: "Tezbar.PreferenceSetup",
    props: {
      extensionId: setup.extensionId,
      commandName: commandName2,
      title: setup.title,
      iconPath: setup.iconPath,
      preferences: setup.preferences,
      values: setup.values,
      includeApiKey: extensionId === "raycast.google-translate"
    },
    children: []
  };
}
function parsePackageJson(path7) {
  if (!(0, import_node_fs12.existsSync)(path7)) {
    throw new Error(`Missing package.json at ${path7}`);
  }
  const raw = (0, import_node_fs12.readFileSync)(path7, "utf8");
  const parsed = JSON.parse(raw);
  return parsed && typeof parsed === "object" ? parsed : {};
}
function findCommandInManifest(pkg, commandName2) {
  const command = (pkg.commands ?? []).find((entry) => entry.name === commandName2);
  if (!command) {
    throw new Error(`Command not found: ${commandName2}`);
  }
  return command;
}
function resolveCommandEntry(packageRoot, commandName2, command) {
  const prebuilt = (0, import_node_path12.join)(packageRoot, ".sc-build", `${commandName2}.js`);
  if ((0, import_node_fs12.existsSync)(prebuilt)) return prebuilt;
  const explicit = [command.path, command.entrypoint, command.entry, command.file, command.source].filter((entry) => typeof entry === "string" && entry.trim().length > 0).map((entry) => (0, import_node_path12.join)(packageRoot, entry));
  const src = (0, import_node_path12.join)(packageRoot, "src");
  const defaults = [
    (0, import_node_path12.join)(src, `${commandName2}.tsx`),
    (0, import_node_path12.join)(src, `${commandName2}.ts`),
    (0, import_node_path12.join)(src, `${commandName2}.jsx`),
    (0, import_node_path12.join)(src, `${commandName2}.js`),
    (0, import_node_path12.join)(src, commandName2, "index.tsx"),
    (0, import_node_path12.join)(src, commandName2, "index.ts"),
    (0, import_node_path12.join)(src, commandName2, "index.jsx"),
    (0, import_node_path12.join)(src, commandName2, "index.js"),
    (0, import_node_path12.join)(src, "commands", `${commandName2}.tsx`),
    (0, import_node_path12.join)(src, "commands", `${commandName2}.ts`),
    (0, import_node_path12.join)(src, "commands", `${commandName2}.jsx`),
    (0, import_node_path12.join)(src, "commands", `${commandName2}.js`)
  ];
  const candidate = [...explicit, ...defaults].find((entry) => (0, import_node_fs12.existsSync)(entry));
  if (!candidate) {
    throw new Error(`Could not resolve entry file for command ${commandName2}`);
  }
  return candidate;
}
async function bundleCommand(entryPath, packageRoot) {
  if (entryPath.includes(`${(0, import_node_path12.join)(".sc-build", "")}`) || entryPath.includes("/.sc-build/")) {
    const prebuilt = (0, import_node_fs12.readFileSync)(entryPath, "utf8");
    if (!prebuilt.trim()) throw new Error(`Prebuilt extension bundle is empty: ${entryPath}`);
    return prebuilt;
  }
  configurePackagedEsbuildBinary();
  const esbuild = await import("esbuild");
  const legacyCheerioInterop = {
    name: "legacy-cheerio-default-interop",
    setup(build) {
      build.onLoad({ filter: /\.[cm]?[jt]sx?$/ }, (args) => {
        const source = (0, import_node_fs12.readFileSync)(args.path, "utf8");
        if (!/import\s+[A-Za-z_$][\w$]*\s+from\s+['"]cheerio['"]/.test(source)) return null;
        const extension = (0, import_node_path12.extname)(args.path).toLowerCase();
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
    nodePaths: [(0, import_node_path12.join)(packageRoot, "node_modules")],
    logLevel: "silent"
  });
  const output = result.outputFiles?.[0]?.text;
  if (!output) {
    throw new Error("esbuild did not produce output");
  }
  return output;
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
function createReactShim(session2) {
  const jsxRuntime = createJsxRuntimeShim();
  const jsx = jsxRuntime.jsx;
  const queueEffect = (idx, sideEffect, deps, label = "useEffect") => {
    const prevDeps = session2.effectDeps.get(idx);
    if (deps && hookDepsEqual(prevDeps, deps)) return;
    session2.pendingEffects.push({ idx, sideEffect, deps, label });
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
      session2.hasStateUpdates = true;
    }
    forceUpdate() {
      session2.hasStateUpdates = true;
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
      const idx = session2.hookIndex++;
      if (session2.hookStates.length > idx) {
        return session2.hookStates[idx];
      }
      let value = typeof initial === "function" ? initial() : initial;
      const setState = (next) => {
        value = typeof next === "function" ? next(value) : next;
        session2.hookStates[idx] = [value, setState];
        session2.hasStateUpdates = true;
      };
      const tuple = [value, setState];
      session2.hookStates[idx] = tuple;
      return tuple;
    },
    useEffect: (sideEffect, deps) => {
      const idx = session2.hookIndex++;
      queueEffect(idx, sideEffect, deps, "useEffect");
    },
    useLayoutEffect: (sideEffect, deps) => {
      const idx = session2.hookIndex++;
      queueEffect(idx, sideEffect, deps, "useLayoutEffect");
    },
    useMemo: (factory, deps) => {
      const idx = session2.hookIndex++;
      const existing = session2.hookStates[idx];
      if (existing?.kind === "memo" && deps && hookDepsEqual(existing.deps, deps)) {
        return existing.value;
      }
      const value = factory();
      session2.hookStates[idx] = { kind: "memo", value, deps };
      return value;
    },
    useCallback: (callback, deps) => {
      const idx = session2.hookIndex++;
      const existing = session2.hookStates[idx];
      if (existing?.kind === "callback" && deps && hookDepsEqual(existing.deps, deps)) {
        return existing.value;
      }
      session2.hookStates[idx] = { kind: "callback", value: callback, deps };
      return callback;
    },
    useRef: (value) => {
      const idx = session2.hookIndex++;
      const existing = session2.hookStates[idx];
      if (existing && typeof existing === "object" && "current" in existing) {
        return existing;
      }
      const ref = { current: value };
      session2.hookStates[idx] = ref;
      return ref;
    },
    useContext: (context) => {
      session2.hookIndex++;
      return context && context.$$typeof === REACT_CONTEXT ? context._currentValue : null;
    },
    useDebugValue: () => {
      session2.hookIndex++;
    },
    useSyncExternalStore: (subscribe, getSnapshot, getServerSnapshot) => {
      void getServerSnapshot;
      const idx = session2.hookIndex++;
      const snapshot = getSnapshot();
      const existing = session2.hookStates[idx];
      const state = existing?.kind === "external-store" ? existing : { kind: "external-store", snapshot };
      state.snapshot = snapshot;
      session2.hookStates[idx] = state;
      queueEffect(
        idx,
        () => subscribe(() => {
          const nextSnapshot = getSnapshot();
          if (Object.is(state.snapshot, nextSnapshot)) return;
          state.snapshot = nextSnapshot;
          session2.hasStateUpdates = true;
        }),
        [subscribe, getSnapshot],
        "useSyncExternalStore"
      );
      return snapshot;
    },
    useReducer: (reducer, initialArg) => {
      const idx = session2.hookIndex++;
      if (session2.hookStates.length > idx) {
        return session2.hookStates[idx];
      }
      let current = initialArg;
      const dispatch = (action) => {
        current = reducer(current, action);
        session2.hookStates[idx] = [current, dispatch];
        session2.hasStateUpdates = true;
      };
      const tuple = [current, dispatch];
      session2.hookStates[idx] = tuple;
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
  const hash = (0, import_node_crypto5.createHash)("sha1").update(`${index}:${typeName}:${title}`).digest("hex").slice(0, 12);
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
function pushFeedback(session2, feedback) {
  session2.feedback.push(feedback);
  if (session2.feedback.length > 20) {
    session2.feedback.splice(0, session2.feedback.length - 20);
  }
}
function pushEffect(session2, effect) {
  session2.effects.push(effect);
  if (session2.effects.length > 50) {
    session2.effects.splice(0, session2.effects.length - 50);
  }
}
function createLocalStorageShim(packageRoot) {
  const storagePath = (0, import_node_path12.join)(packageRoot, ".tezbar-local-storage.json");
  const readAll2 = () => {
    if (!(0, import_node_fs12.existsSync)(storagePath)) return {};
    try {
      const parsed = JSON.parse((0, import_node_fs12.readFileSync)(storagePath, "utf8"));
      return parsed && typeof parsed === "object" ? parsed : {};
    } catch {
      return {};
    }
  };
  const writeAll2 = (value) => {
    (0, import_node_fs12.mkdirSync)((0, import_node_path12.dirname)(storagePath), { recursive: true });
    (0, import_node_fs12.writeFileSync)(storagePath, JSON.stringify(value, null, 2), "utf8");
  };
  return {
    getItem: async (key) => readAll2()[String(key)],
    setItem: async (key, value) => {
      const all = readAll2();
      all[String(key)] = String(value);
      writeAll2(all);
    },
    removeItem: async (key) => {
      const all = readAll2();
      delete all[String(key)];
      writeAll2(all);
    },
    clear: async () => writeAll2({}),
    allItems: async () => readAll2()
  };
}
function createCacheShim(packageRoot) {
  return class CacheShim {
    subscribers = /* @__PURE__ */ new Set();
    storagePath;
    constructor(options) {
      const rawNamespace = typeof options?.namespace === "string" && options.namespace.trim().length > 0 ? options.namespace.trim() : "shared";
      const safeNamespace = rawNamespace.replace(/[^a-z0-9._-]+/gi, "_");
      this.storagePath = (0, import_node_path12.join)(packageRoot, ".tezbar-support", "cache", `${safeNamespace}.json`);
    }
    readAll() {
      if (!(0, import_node_fs12.existsSync)(this.storagePath)) return {};
      try {
        const parsed = JSON.parse((0, import_node_fs12.readFileSync)(this.storagePath, "utf8"));
        return parsed && typeof parsed === "object" ? parsed : {};
      } catch {
        return {};
      }
    }
    writeAll(value) {
      (0, import_node_fs12.mkdirSync)((0, import_node_path12.dirname)(this.storagePath), { recursive: true });
      (0, import_node_fs12.writeFileSync)(this.storagePath, JSON.stringify(value, null, 2), "utf8");
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
function copyToSystemClipboard(session2, value) {
  const effectValue = typeof value === "string" ? value : value && typeof value === "object" ? String(
    value.text ?? value.file ?? ""
  ) : String(value ?? "");
  pushEffect(session2, { kind: "clipboard", value: effectValue.slice(0, 2e3) });
  if (session2.effectMode === "record") return;
  if (value && typeof value === "object") {
    const payload = value;
    if (typeof payload.file === "string" && payload.file.trim()) {
      const filePath = payload.file.trim();
      const image = nativeImage.createFromPath(filePath);
      if (!image.isEmpty()) {
        clipboard.writeImage(image);
        return;
      }
      clipboard.writeText(filePath);
      return;
    }
    if (typeof payload.html === "string" && typeof payload.text === "string") {
      clipboard.write({ html: payload.html, text: payload.text });
      return;
    }
    if (typeof payload.html === "string") {
      clipboard.write({ html: payload.html, text: payload.html });
      return;
    }
    if (typeof payload.text === "string") {
      clipboard.writeText(payload.text);
      return;
    }
  }
  clipboard.writeText(String(value ?? ""));
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
function createRaycastApiShim(session2) {
  class PKCEClientShim {
    tokenPath;
    constructor(options = {}) {
      const providerId = String(options.providerId ?? options.providerName ?? "oauth").replace(/[^a-z0-9._-]+/gi, "_").toLowerCase();
      this.tokenPath = (0, import_node_path12.join)(session2.packageRoot, ".tezbar-support", "oauth", `${providerId}.json`);
    }
    async getTokens() {
      if (!(0, import_node_fs12.existsSync)(this.tokenPath)) return void 0;
      try {
        const stored = JSON.parse((0, import_node_fs12.readFileSync)(this.tokenPath, "utf8"));
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
      (0, import_node_fs12.mkdirSync)((0, import_node_path12.dirname)(this.tokenPath), { recursive: true });
      (0, import_node_fs12.writeFileSync)(this.tokenPath, JSON.stringify(tokens), "utf8");
    }
    async removeTokens() {
      await (0, import_promises2.rm)(this.tokenPath, { force: true });
    }
    async authorizationRequest(options) {
      return {
        ...options,
        codeVerifier: makeId2("pkce").replace(/[^a-z0-9]/gi, ""),
        redirectURI: `raycast://oauth?extension=${encodeURIComponent(session2.extensionId)}`
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
      pushFeedback(session2, this.shownFeedback);
      pushEffect(session2, this.shownEffect);
    }
    async hide() {
    }
  }
  return {
    List,
    Form,
    Grid,
    AI: {
      ask: async (prompt) => askExtensionAI(String(prompt ?? "")),
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
    LaunchType: {
      UserInitiated: "userInitiated",
      Background: "background"
    },
    environment: {
      raycastVersion: "1.80.0",
      extensionName: session2.extensionId,
      commandName: session2.commandName,
      isDevelopment: false,
      commandMode: session2.commandMode,
      assetsPath: (0, import_node_path12.join)(session2.packageRoot, "assets"),
      supportPath: (0, import_node_path12.join)(session2.packageRoot, ".tezbar-support"),
      canAccess: () => false,
      get searchText() {
        return session2.searchText;
      }
    },
    LocalStorage: createLocalStorageShim(session2.packageRoot),
    Cache: createCacheShim(session2.packageRoot),
    runAppleScript: (source) => runAppleScriptForSession(session2, source),
    Clipboard: {
      copy: async (value) => {
        copyToSystemClipboard(session2, value);
      },
      paste: async (value) => {
        copyToSystemClipboard(session2, value);
      },
      read: async () => {
        const text = clipboard.readText();
        return text ? { text } : {};
      },
      readText: async () => clipboard.readText()
    },
    getPreferenceValues: () => session2.preferences,
    getSelectedFinderItems: async () => {
      if (process.platform !== "darwin") return [];
      try {
        const output = await runAppleScript2(`
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
        return output.split(/\r?\n/).map((value) => value.trim()).filter(Boolean).map((path7) => ({ path: path7 }));
      } catch {
        return [];
      }
    },
    launchCommand: async () => {
    },
    useNavigation: () => ({
      push: (next) => {
        session2.stack.push(next);
      },
      pop: () => {
        if (session2.stack.length > 1) {
          session2.stack.pop();
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
      pushFeedback(session2, { kind: "hud", message });
      pushEffect(session2, { kind: "hud", message });
    },
    open: async (target) => {
      if (typeof target !== "string") return;
      if (/^[a-z][a-z0-9+.-]*:\/\//i.test(target) || target.startsWith("mailto:")) {
        pushEffect(session2, { kind: "open", value: target });
        if (session2.effectMode === "record") return;
        await shell.openExternal(target);
      }
    },
    showInFinder: async (path7) => {
      if (typeof path7 !== "string") return;
      pushEffect(session2, { kind: "show-in-finder", value: path7 });
      if (session2.effectMode === "record") return;
      shell.showItemInFolder(path7);
    },
    getApplications: async () => {
      const now = Date.now();
      if (applicationsCache && applicationsCache.expiresAt > now) {
        return applicationsCache.promise;
      }
      console.log("[getApplications] Starting Spotlight query for installed apps...");
      const promise = (async () => {
        try {
          const { stdout } = await execFileAsync7(
            "/usr/bin/mdfind",
            ["kMDItemKind == 'Application'"],
            {
              maxBuffer: 10 * 1024 * 1024,
              timeout: 3e3
            }
          );
          const apps = stdout.trim().split("\n").filter((p) => p.endsWith(".app")).map((appPath) => ({ name: (0, import_node_path12.basename)(appPath, ".app"), path: appPath })).sort((a, b) => a.name.localeCompare(b.name));
          console.log(`[getApplications] mdfind returned ${apps.length} applications`);
          return apps;
        } catch (err) {
          console.warn("[getApplications] mdfind failed, falling back to directory scan:", err);
          const apps = [];
          const dirs = ["/Applications", "/System/Applications", (0, import_node_path12.join)((0, import_node_os7.homedir)(), "Applications")];
          for (const dir of dirs) {
            try {
              for (const entry of (0, import_node_fs12.readdirSync)(dir)) {
                if (entry.endsWith(".app")) {
                  apps.push({ name: (0, import_node_path12.basename)(entry, ".app"), path: (0, import_node_path12.join)(dir, entry) });
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
    getFrontmostApplication: async () => {
      try {
        const script = 'tell application "System Events" to get name of first application process whose frontmost is true';
        const { stdout } = await execFileAsync7("/usr/bin/osascript", ["-e", script], {
          timeout: 3e3
        });
        const name = stdout.trim();
        if (name) return { name, path: `/Applications/${name}.app` };
        return { name: "Raymes", path: process.execPath };
      } catch {
        return { name: "Raymes", path: process.execPath };
      }
    },
    getDefaultApplication: async () => {
      return null;
    },
    confirmAlert: async () => true,
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
function createRaycastUtilsShim(session2) {
  const CacheShim = createCacheShim(session2.packageRoot);
  const cache2 = new CacheShim();
  const functionCache = /* @__PURE__ */ new Map();
  const useCachedState = (key, initialValue) => {
    const hookIdx = session2.hookIndex++;
    const existing = session2.hookStates[hookIdx];
    if (existing) return existing;
    const getInitialValue = () => {
      const raw = cache2.get(String(key));
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
      cache2.set(String(key), JSON.stringify(current));
      session2.hookStates[hookIdx] = [current, setValue];
      session2.hasStateUpdates = true;
    };
    const tuple = [current, setValue];
    session2.hookStates[hookIdx] = tuple;
    return tuple;
  };
  const cacheKey2 = (fn, args) => {
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
      const hookIdx = session2.hookIndex++;
      const stableArgs = Array.isArray(args) ? args : [];
      const opts = options && typeof options === "object" ? options : {};
      const key = `${hookIdx}:${cacheKey2(fn, stableArgs)}`;
      const label = promiseHookLabel(hookIdx, fn, stableArgs);
      const shouldExecute = opts?.execute !== false;
      const previousKey = session2.promiseKeysByHook.get(hookIdx);
      const previousEntry = previousKey && previousKey !== key ? session2.promiseCache.get(previousKey) : void 0;
      const previousData = opts.keepPreviousData ? previousEntry?.data : void 0;
      if (previousKey !== key) {
        opts.abortable?.current?.abort();
        if (previousKey) session2.promiseCache.delete(previousKey);
        session2.promisePaginationByHook.delete(hookIdx);
        session2.promiseKeysByHook.set(hookIdx, key);
      }
      const schedule = (retainedData) => {
        if (!shouldExecute || typeof fn !== "function" || session2.disposed) return null;
        const startedAt = Date.now();
        console.log(`[usePromise] Scheduled ${label}`);
        opts.abortable?.current?.abort();
        const controller = new AbortController();
        if (opts.abortable) opts.abortable.current = controller;
        session2.abortControllers.add(controller);
        const tracked = delay(0).then(async () => {
          if (session2.disposed || controller.signal.aborted) {
            const error = new Error("Aborted");
            error.name = "AbortError";
            throw error;
          }
          console.log(`[usePromise] Starting ${label}`);
          await Promise.resolve(opts.onWillExecute?.(stableArgs));
          return await Promise.resolve(fn(...stableArgs));
        }).then(async (data) => {
          if (session2.promiseCache.get(key)?.promise !== tracked) return data;
          if (typeof data === "function") {
            const loader = data;
            const paginationState = {
              key,
              page: -1,
              hasMore: true,
              loader,
              loadingPromise: null
            };
            session2.promisePaginationByHook.set(hookIdx, paginationState);
            const firstPage = await Promise.resolve(loader({ page: 0 }));
            const pageResult = firstPage && typeof firstPage === "object" ? firstPage : { data: firstPage, hasMore: false };
            paginationState.page = 0;
            paginationState.hasMore = pageResult.hasMore === true;
            data = pageResult.data;
          }
          console.log(
            `[usePromise] Resolved ${label} after ${elapsedMs(startedAt)}; data=${Array.isArray(data) ? `array(${data.length})` : typeof data}`
          );
          session2.promiseCache.set(key, { data, error: void 0, label });
          if (persistent) writePromiseResultCache(session2, key, data);
          await Promise.resolve(opts.onData?.(data));
          if (!session2.disposed) session2.hasStateUpdates = true;
          return data;
        }).catch(async (error) => {
          if (session2.promiseCache.get(key)?.promise !== tracked) return void 0;
          const isAbort = controller.signal.aborted || error instanceof Error && error.name === "AbortError";
          const recovered = !isAbort && await recoverIncompleteChunkedCache(session2, error, key);
          if (recovered) {
            console.warn(
              `[usePromise] Recovered ${label} after ${elapsedMs(startedAt)}; retrying on refresh.`
            );
            session2.promiseCache.delete(key);
            if (!session2.disposed) session2.hasStateUpdates = true;
            return void 0;
          }
          session2.promiseCache.set(key, {
            data: retainedData,
            error: isAbort ? void 0 : error
          });
          const paginationState = session2.promisePaginationByHook.get(hookIdx);
          if (paginationState?.key === key && paginationState.page < 0) {
            session2.promisePaginationByHook.delete(hookIdx);
          }
          if (isAbort) return void 0;
          console.error(
            `[usePromise] Rejected ${label} after ${elapsedMs(startedAt)}:`,
            error instanceof Error ? error.message : String(error)
          );
          if (!session2.disposed) session2.hasStateUpdates = true;
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
          session2.abortControllers.delete(controller);
          if (opts.abortable?.current === controller) {
            opts.abortable.current = null;
          }
        });
        session2.promiseCache.set(key, {
          promise: tracked,
          data: retainedData,
          error: void 0,
          label,
          startedAt
        });
        return tracked;
      };
      let cached = session2.promiseCache.get(key);
      if (!cached && shouldExecute) {
        const persistentEntry = persistent ? readPromiseResultCache(session2, key) : null;
        const retainedData = previousData ?? persistentEntry?.data ?? opts.initialData;
        schedule(retainedData);
        cached = session2.promiseCache.get(key);
      }
      const pagination = () => {
        const paginationState = session2.promisePaginationByHook.get(hookIdx);
        if (!paginationState || paginationState.key !== key) return void 0;
        return {
          hasMore: paginationState.hasMore,
          onLoadMore: async () => {
            if (!paginationState.hasMore || session2.disposed) return;
            if (paginationState.loadingPromise) return paginationState.loadingPromise;
            const nextPage = paginationState.page + 1;
            const loadingPromise = Promise.resolve(paginationState.loader({ page: nextPage })).then(async (rawResult) => {
              const pageResult = rawResult && typeof rawResult === "object" ? rawResult : { data: rawResult, hasMore: false };
              const currentData = session2.promiseCache.get(key)?.data;
              const mergedData = Array.isArray(currentData) && Array.isArray(pageResult.data) ? [...currentData, ...pageResult.data] : pageResult.data;
              paginationState.page = nextPage;
              paginationState.hasMore = pageResult.hasMore === true;
              session2.promiseCache.set(key, { data: mergedData, error: void 0, label });
              await Promise.resolve(opts.onData?.(mergedData));
              if (!session2.disposed) session2.hasStateUpdates = true;
            }).catch(async (error) => {
              session2.promiseCache.set(key, {
                data: session2.promiseCache.get(key)?.data,
                error,
                label
              });
              paginationState.hasMore = false;
              if (typeof opts.onError === "function") {
                await Promise.resolve(opts.onError(error));
              }
              if (!session2.disposed) session2.hasStateUpdates = true;
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
          session2.pendingPromises.push(cached.promise);
          return {
            data: cached.data,
            isLoading: true,
            error: void 0,
            revalidate: async () => {
              await schedule(cached?.data);
            },
            mutate: async (next) => {
              const value = typeof next === "function" ? next(cached?.data) : next;
              session2.promiseCache.set(key, { data: value, error: void 0 });
              if (!session2.disposed) session2.hasStateUpdates = true;
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
            session2.promiseCache.set(key, { data: value, error: void 0 });
            if (!session2.disposed) session2.hasStateUpdates = true;
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
  const useExec = (command, args = [], options) => {
    const exec2 = async () => {
      const { stdout, stderr } = await execFileAsync7(command, args, {
        encoding: "utf8",
        maxBuffer: 10 * 1024 * 1024
      });
      const result = { stdout, stderr, exitCode: 0 };
      return options?.parseOutput ? options.parseOutput(result) : stdout;
    };
    return useExecPromise(exec2, [command, ...args], options);
  };
  const useFetch = (input, options) => {
    const requestInit = {
      method: options?.method,
      headers: options?.headers,
      body: options?.body
    };
    const load2 = async () => {
      const response = await fetch(input, requestInit);
      if (!response.ok) throw new Error(`Request failed with status ${response.status}`);
      if (options?.parseResponse) return options.parseResponse(response);
      const contentType = response.headers.get("content-type") ?? "";
      const parsed = contentType.includes("json") ? await response.json() : await response.text();
      const mapped = options?.mapResult ? options.mapResult(parsed) : parsed;
      return mapped && typeof mapped === "object" && "data" in mapped ? mapped.data : mapped;
    };
    return useFetchPromise(load2, [String(input), requestInit], options);
  };
  const useSQL = (databasePath, query, options) => {
    const load2 = async (dbPath3, sql) => {
      const sqlite = process.platform === "win32" ? "sqlite3.exe" : "/usr/bin/sqlite3";
      const { stdout } = await execFileAsync7(sqlite, ["-readonly", "-json", dbPath3, sql], {
        encoding: "utf8",
        maxBuffer: 20 * 1024 * 1024
      });
      const trimmed = stdout.trim();
      return trimmed ? JSON.parse(trimmed) : [];
    };
    const result = useSQLPromise(load2, [databasePath, query], options);
    return { ...result, permissionView: void 0 };
  };
  const FormValidation = { Required: "required" };
  const useForm = (options = {}) => {
    const [values, setValues] = useCachedState(
      `form-values:${session2.commandName}`,
      options.initialValues ?? {}
    );
    const [errors, setErrors] = useCachedState(
      `form-errors:${session2.commandName}`,
      {}
    );
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
    const itemProps = Object.fromEntries([...keys].map((key) => [key, {
      id: key,
      value: values[key],
      error: errors[key],
      onChange: (value) => setValue(key, value)
    }]));
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
    const [ranking, setRanking] = useCachedState(
      `frecency:${namespace}`,
      {}
    );
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
        await (0, import_promises2.rm)(this.tokenPath, { force: true });
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
      this.tokenPath = (0, import_node_path12.join)(
        session2.packageRoot,
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
      if (!(0, import_node_fs12.existsSync)(this.tokenPath)) return void 0;
      try {
        const value = JSON.parse((0, import_node_fs12.readFileSync)(this.tokenPath, "utf8"));
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
      if (!accessToken) throw new Error(`${this.provider} OAuth response did not include an access token`);
      const expiresIn = Number(response.expires_in);
      const stored = {
        accessToken,
        refreshToken: String(response.refresh_token ?? existingRefreshToken ?? "") || void 0,
        expiresAt: Number.isFinite(expiresIn) ? Date.now() + expiresIn * 1e3 : void 0,
        scope: String(response.scope ?? "") || void 0
      };
      (0, import_node_fs12.mkdirSync)((0, import_node_path12.dirname)(this.tokenPath), { recursive: true });
      (0, import_node_fs12.writeFileSync)(this.tokenPath, JSON.stringify(stored), "utf8");
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
          String(payload.error_description ?? payload.error ?? `OAuth token exchange failed (${response.status})`)
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
      const state = (0, import_node_crypto5.randomBytes)(24).toString("base64url");
      const codeVerifier = (0, import_node_crypto5.randomBytes)(48).toString("base64url");
      const codeChallenge = (0, import_node_crypto5.createHash)("sha256").update(codeVerifier).digest("base64url");
      let settleCallback = null;
      let rejectCallback = null;
      const callbackPromise = new Promise((resolve4, reject) => {
        settleCallback = resolve4;
        rejectCallback = reject;
      });
      const server = (0, import_node_http.createServer)((request, response) => {
        const address2 = server.address();
        const port2 = address2 && typeof address2 === "object" ? address2.port : 0;
        const redirectUri2 = `http://127.0.0.1:${port2}/oauth/callback`;
        const callbackUrl = new URL(request.url ?? "/", redirectUri2);
        if (callbackUrl.pathname !== "/oauth/callback") {
          response.writeHead(404).end("Not found");
          return;
        }
        if (callbackUrl.searchParams.get("state") !== state) {
          response.writeHead(400).end("Invalid OAuth state");
          rejectCallback?.(new Error("OAuth callback state did not match"));
          return;
        }
        const providerError = callbackUrl.searchParams.get("error");
        const code = callbackUrl.searchParams.get("code");
        if (providerError || !code) {
          response.writeHead(400).end("Authorization failed");
          rejectCallback?.(new Error(providerError || "OAuth callback did not include a code"));
          return;
        }
        response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
        response.end("<!doctype html><title>Tezbar authorized</title><p>You can close this window.</p>");
        settleCallback?.({ code, redirectUri: redirectUri2 });
      });
      await new Promise((resolve4, reject) => {
        server.once("error", reject);
        server.listen(0, "127.0.0.1", resolve4);
      });
      const address = server.address();
      const port = address && typeof address === "object" ? address.port : 0;
      const redirectUri = `http://127.0.0.1:${port}/oauth/callback`;
      const authorizationEndpoint = typeof this.options.authorizationEndpoint === "string" && this.options.authorizationEndpoint ? this.options.authorizationEndpoint : "https://accounts.google.com/o/oauth2/v2/auth";
      const authorizationUrl = new URL(authorizationEndpoint);
      authorizationUrl.search = new URLSearchParams({
        client_id: clientId,
        redirect_uri: redirectUri,
        response_type: "code",
        scope,
        state,
        code_challenge: codeChallenge,
        code_challenge_method: "S256",
        access_type: "offline",
        prompt: "consent"
      }).toString();
      pushEffect(session2, { kind: "open", value: authorizationUrl.toString() });
      const timeoutMs = Math.max(1e3, Number(this.options.timeoutMs) || 12e4);
      const timeout = setTimeout(
        () => rejectCallback?.(new Error("OAuth authorization timed out")),
        timeoutMs
      );
      try {
        if (this.options.openAuthorizationUrl) {
          await Promise.resolve(this.options.openAuthorizationUrl(authorizationUrl.toString()));
        } else if (session2.effectMode === "record") {
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
        await new Promise((resolve4) => server.close(() => resolve4()));
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
    useLocalStorage: (key, initialValue) => {
      const [value, setValue] = useCachedState(key, initialValue);
      return {
        value,
        setValue: async (next) => setValue(next),
        removeValue: async () => {
          cache2.remove(String(key));
          setValue(initialValue);
        },
        isLoading: false
      };
    },
    getAvatarIcon: avatarIcon,
    withCache: (fn, options) => {
      return async (...args) => {
        const key = cacheKey2(fn, args);
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
    runAppleScript: (source) => runAppleScriptForSession(session2, source),
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
      pushFeedback(session2, feedback);
      pushEffect(session2, feedback);
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
function mimeTypeForAsset(path7) {
  switch ((0, import_node_path12.extname)(path7).toLowerCase()) {
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
      const assetPath = (0, import_node_path12.join)(packageRoot, "assets", cleanSrc);
      if (!(0, import_node_fs12.existsSync)(assetPath)) {
        console.warn(`[ExtensionAssets] Missing markdown asset: ${assetPath}`);
        return match;
      }
      try {
        const encoded = (0, import_node_fs12.readFileSync)(assetPath).toString("base64");
        console.log(`[ExtensionAssets] Inlined markdown asset: ${assetPath}`);
        return `![${alt}](data:${mimeTypeForAsset(assetPath)};base64,${encoded})`;
      } catch {
        return match;
      }
    }
  );
}
function registerAction(typeName, props, session2) {
  const index = session2.currentActions.length;
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
  session2.currentActions.push(action);
  const handler = async (formValues) => {
    if (kind === "copy") {
      const content = props.content ?? props.title ?? "";
      copyToSystemClipboard(session2, content);
      if (typeof props.onPaste === "function") {
        await Promise.resolve(props.onPaste());
      }
    }
    if (kind === "open") {
      const url = typeof props.url === "string" ? props.url : "";
      if (url) {
        pushEffect(session2, { kind: "open", value: url });
        if (session2.effectMode !== "record") await shell.openExternal(url);
      }
    }
    if (kind === "show-in-finder") {
      const path7 = typeof props.path === "string" ? props.path : "";
      if (path7) {
        pushEffect(session2, { kind: "show-in-finder", value: path7 });
        if (session2.effectMode !== "record") shell.showItemInFolder(path7);
      }
    }
    if (kind === "push" && props.target !== void 0) {
      session2.stack.push(props.target);
    }
    if (kind === "pop") {
      if (session2.stack.length > 1) {
        session2.stack.pop();
      }
    }
    if (kind === "submit-form" && typeof props.onSubmit === "function") {
      await Promise.resolve(
        props.onSubmit(formValues ?? {})
      );
      return;
    }
    if (typeof props.onAction === "function") {
      await Promise.resolve(props.onAction());
    }
  };
  session2.actionHandlers.set(id, handler);
}
function walkRuntimeNodes(input, session2, depth, budget, options = {}) {
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
      nodes.push(...walkRuntimeNodes(entry, session2, depth, budget, options));
    }
    return nodes;
  }
  if (!isJsxNode(input)) {
    return [];
  }
  const type = input.type;
  const props = input.props ?? {};
  if (type === JSX_FRAGMENT) {
    return walkRuntimeNodes(props.children, session2, depth + 1, budget, options);
  }
  if (typeof type === "function") {
    let rendered;
    try {
      const component = type;
      if (typeof component.prototype?.render === "function") {
        const instance2 = new component(props);
        const derived = component.getDerivedStateFromProps?.(props, instance2.state);
        if (derived && typeof derived === "object") {
          instance2.state = {
            ...instance2.state && typeof instance2.state === "object" ? instance2.state : {},
            ...derived
          };
        }
        rendered = instance2.render();
      } else {
        rendered = component(props);
      }
    } catch (error) {
      console.error("[ExtensionRuntime] Component render failed:", error);
      const message = error && typeof error === "object" && typeof error.message === "string" ? String(error.message) : String(error);
      session2.renderErrors.push(message);
      return [];
    }
    return walkRuntimeNodes(rendered, session2, depth + 1, budget, options);
  }
  const typeName = isToken(type) ? type.name : typeof type === "string" ? type : "";
  if (!typeName) return [];
  if (typeName.startsWith("Action")) {
    if (typeName === "ActionPanel" || typeName.startsWith("ActionPanel.")) {
      return walkRuntimeNodes(props.children, session2, depth + 1, budget, options);
    }
    registerAction(typeName, props, session2);
    return [];
  }
  if (typeName === "List.Item" && options.listItemsSeen && options.listItemLimit) {
    if (options.listItemsSeen.count >= options.listItemLimit) {
      return [];
    }
    options.listItemsSeen.count += 1;
  }
  if (typeName === "List" && typeof props.onSearchTextChange === "function") {
    session2.searchTextChangeHandler = props.onSearchTextChange;
  }
  const actionStart = session2.currentActions.length;
  const nestedOptions = {
    ...options,
    listItemsSeen: void 0,
    listItemLimit: void 0,
    listItemsTruncated: void 0
  };
  if (props.actions !== void 0) {
    walkRuntimeNodes(props.actions, session2, depth + 1, budget, nestedOptions);
  }
  const actionIds = session2.currentActions.slice(actionStart).map((action) => action.id);
  const metadataNodes = props.metadata !== void 0 ? walkRuntimeNodes(props.metadata, session2, depth + 1, budget, nestedOptions) : [];
  const detailNodes = props.detail !== void 0 ? walkRuntimeNodes(props.detail, session2, depth + 1, budget, nestedOptions) : [];
  const searchBarAccessoryNodes = props.searchBarAccessory !== void 0 ? walkRuntimeNodes(props.searchBarAccessory, session2, depth + 1, budget, nestedOptions) : [];
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
    const actionId = makeId2("list-dropdown");
    sanitizedProps.actionId = actionId;
    session2.actionHandlers.set(actionId, async (formValues) => {
      await Promise.resolve(
        props.onChange(String(formValues?.value ?? ""))
      );
    });
  }
  if (typeName === "List" && session2.searchTextChangeHandler && sanitizedProps) {
    sanitizedProps.__hasServerSearch = true;
  }
  if (typeName === "List" && props.pagination && typeof props.pagination === "object") {
    const pagination = props.pagination;
    if (typeof pagination.onLoadMore === "function") {
      session2.serverLoadMoreHandler = pagination.onLoadMore;
      session2.serverHasMore = pagination.hasMore === true;
    }
  }
  const listItemsTruncated = typeName === "List" ? { value: false } : options.listItemsTruncated;
  const childOptions = typeName === "List" ? {
    ...options,
    listItemsSeen: { count: 0 },
    listItemLimit: session2.listItemLimit,
    listItemsTruncated
  } : typeName === "List.Section" ? {
    ...options,
    listItemsSeen: options.listItemsSeen ?? { count: 0 },
    listItemLimit: session2.listItemLimit
  } : options;
  const children = walkRuntimeNodes(props.children, session2, depth + 1, budget, childOptions);
  if (typeName === "List" && sanitizedProps && listItemsTruncated) {
    sanitizedProps.__hasMore = session2.serverHasMore || listItemsTruncated.value && session2.listItemLimit < RUNTIME_COMPONENT_LIMIT;
    sanitizedProps.__pageSize = session2.listItemLimit;
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
function renderCurrentView(session2) {
  const startedAt = Date.now();
  console.log(
    `[Runner] renderCurrentView start ${session2.extensionId}/${session2.commandName}; stack=${session2.stack.length}, limit=${session2.listItemLimit}`
  );
  const top = session2.stack.at(-1);
  if (!top) {
    return {
      ok: false,
      message: "No view is available for this extension session."
    };
  }
  session2.actionHandlers.clear();
  session2.currentActions = [];
  session2.renderErrors = [];
  session2.serverLoadMoreHandler = null;
  session2.serverHasMore = false;
  const budget = { remaining: RUNTIME_COMPONENT_LIMIT };
  const internalTop = top;
  const nodes = typeof internalTop.type === "string" && internalTop.type.startsWith("Tezbar.") ? [top] : walkRuntimeNodes(top, session2, 0, budget);
  console.log(
    `[Runner] walkRuntimeNodes complete after ${elapsedMs(startedAt)}; nodes=${nodes.length}, budgetUsed=${RUNTIME_COMPONENT_LIMIT - budget.remaining}, actions=${session2.currentActions.length}`
  );
  if (session2.renderErrors.length > 0) {
    session2.pendingEffects = [];
    return {
      ok: false,
      message: `Extension render failed: ${session2.renderErrors.join("; ")}`
    };
  }
  flushPendingEffects(session2);
  const root = nodes[0] ?? {
    type: "Detail",
    props: { markdown: "This extension returned an empty view." },
    children: []
  };
  attachRuntimeRootMetadata(root, session2);
  console.log(
    `[Runner] renderCurrentView complete after ${elapsedMs(startedAt)}; root=${root.type}, children=${root.children?.length ?? 0}`
  );
  return {
    ok: true,
    mode: "view",
    message: formatFeedback(session2.feedback.at(-1)),
    sessionId: session2.id,
    extensionId: session2.extensionId,
    commandName: session2.commandName,
    title: session2.title,
    root,
    actions: [...session2.currentActions],
    effects: [...session2.effects]
  };
}
async function rerenderSessionCommand(session2, label) {
  if (!session2.commandFn) {
    return renderCurrentView(session2);
  }
  console.log(`[Runner] ${label}: rerendering ${session2.extensionId}/${session2.commandName}`);
  const startedAt = Date.now();
  session2.hookIndex = 0;
  session2.pendingPromises = [];
  session2.actionHandlers.clear();
  session2.currentActions = [];
  session2.feedback = [];
  session2.hasStateUpdates = false;
  console.log(`[Runner] ${label}: command function start`);
  const result = await Promise.resolve(session2.commandFn({ arguments: session2.commandArgs }));
  console.log(
    `[Runner] ${label}: command function complete after ${elapsedMs(startedAt)}; jsx=${isJsxNode(result)}`
  );
  session2.stack = isJsxNode(result) ? [result] : [];
  const view = renderCurrentView(session2);
  console.log(`[Runner] ${label}: rerender complete after ${elapsedMs(startedAt)}`);
  return view;
}
async function refreshExtensionSession(request) {
  const sessionId = String(request.sessionId || "").trim();
  if (!sessionId) {
    return { ok: false, message: "sessionId is required." };
  }
  const session2 = sessions.get(sessionId);
  if (!session2) {
    return { ok: false, message: "Extension session not found." };
  }
  const inFlight = [...session2.promiseCache.values()].filter((entry) => entry.promise).map(
    (entry) => `${entry.label ?? "unknown"} age=${entry.startedAt ? elapsedMs(entry.startedAt) : "?"}`
  );
  console.log(
    `[Runner] Refresh request ${session2.extensionId}/${session2.commandName}; stateUpdates=${session2.hasStateUpdates}, inFlight=${inFlight.length}${inFlight.length ? `
  ${inFlight.join("\n  ")}` : ""}`
  );
  if (!session2.hasStateUpdates) return { ok: true, mode: "unchanged" };
  return rerenderSessionCommand(session2, "Refresh");
}
async function loadMoreExtensionSession(request) {
  const sessionId = String(request.sessionId || "").trim();
  const session2 = sessions.get(sessionId);
  if (!session2) return { ok: false, message: "Extension session not found." };
  if (session2.serverLoadMoreHandler) {
    if (!session2.serverHasMore) return { ok: true, mode: "unchanged" };
    if (session2.serverLoadMoreRequest) return session2.serverLoadMoreRequest;
    const request2 = (async () => {
      try {
        await session2.serverLoadMoreHandler?.();
        return await rerenderSessionCommand(session2, "Load more");
      } catch (error) {
        return {
          ok: false,
          message: error instanceof Error ? error.message : String(error)
        };
      } finally {
        session2.serverLoadMoreRequest = null;
      }
    })();
    session2.serverLoadMoreRequest = request2;
    return request2;
  }
  const nextLimit = Math.min(session2.listItemLimit + LIST_ITEM_PAGE_SIZE, RUNTIME_COMPONENT_LIMIT);
  if (nextLimit === session2.listItemLimit) return { ok: true, mode: "unchanged" };
  session2.listItemLimit = nextLimit;
  return rerenderSessionCommand(session2, "Load more");
}
async function updateSearchText(request) {
  const sessionId = String(request.sessionId || "").trim();
  const searchText = String(request.searchText ?? "");
  if (!sessionId) {
    return { ok: false, message: "sessionId is required." };
  }
  const session2 = sessions.get(sessionId);
  if (!session2) {
    return { ok: false, message: "Extension session not found." };
  }
  session2.searchText = searchText;
  session2.listItemLimit = searchText.trim() && !session2.searchTextChangeHandler ? RUNTIME_COMPONENT_LIMIT : LIST_ITEM_PAGE_SIZE;
  if (session2.searchTextChangeHandler) {
    try {
      session2.searchTextChangeHandler(searchText);
    } catch (error) {
      return {
        ok: false,
        message: error instanceof Error ? error.message : String(error)
      };
    }
  }
  if (session2.commandFn) {
    try {
      const execArgs = { arguments: session2.commandArgs };
      let result;
      const searchPass = async (label) => {
        console.log(
          `[Runner] ${label}: executing ${session2.extensionId}/${session2.commandName} search="${searchText}"`
        );
        session2.hookIndex = 0;
        session2.pendingPromises = [];
        session2.actionHandlers.clear();
        session2.currentActions = [];
        session2.feedback = [];
        session2.hasStateUpdates = false;
        const r = await Promise.resolve(session2.commandFn(execArgs));
        console.log(
          `[Runner] ${label} complete: ${session2.pendingPromises.length} promises, ${session2.hookStates.length} states, stateUpdates=${session2.hasStateUpdates}`
        );
        return r;
      };
      for (let p = 1; p <= SEARCH_TEXT_RENDER_PASSES; p += 1) {
        result = await searchPass(`Search Pass ${p}`);
        if (!session2.hasStateUpdates) break;
      }
      const searchInFlight = [...session2.promiseCache.values()].filter(
        (entry) => entry.promise
      ).length;
      if (searchInFlight > 0) {
        console.log(
          `[Runner] Search render returned with ${searchInFlight} in-flight promises; refresh will continue.`
        );
      }
      session2.stack = isJsxNode(result) ? [result] : [];
      if (session2.stack.length > 0) {
        return renderCurrentView(session2);
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
    message: formatFeedback(session2.feedback.at(-1)) || ""
  };
}
function cleanupRuntimeSession(session2) {
  if (session2.disposed) return;
  session2.disposed = true;
  for (const controller of session2.abortControllers) {
    controller.abort();
  }
  session2.abortControllers.clear();
  for (const cleanup2 of session2.effectCleanups.values()) {
    try {
      cleanup2();
    } catch (error) {
      console.warn("[Runner] Extension effect cleanup failed:", error);
    }
  }
  session2.effectCleanups.clear();
  session2.effectDeps.clear();
  session2.promiseCache.clear();
  session2.promiseKeysByHook.clear();
  session2.promisePaginationByHook.clear();
  session2.cacheRecoveryKeys.clear();
}
function deleteRuntimeSession(sessionId) {
  const session2 = sessions.get(sessionId);
  if (!session2) return false;
  cleanupRuntimeSession(session2);
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
function runBundle(code, packageRoot, session2) {
  const fileRequire = (0, import_node_module2.createRequire)((0, import_node_path12.join)(packageRoot, "package.json"));
  const jsxRuntimeShim = createJsxRuntimeShim();
  const reactShim = createReactShim(session2);
  const raycastApiShim = createRaycastApiShim(session2);
  const raycastUtilsShim = createRaycastUtilsShim(session2);
  const customRequire = (specifier) => {
    if (specifier === "@raycast/api") return raycastApiShim;
    if (specifier === "@raycast/utils") return raycastUtilsShim;
    if (specifier === "react") return reactShim;
    if (specifier === "react/jsx-runtime" || specifier === "react/jsx-dev-runtime") {
      return jsxRuntimeShim;
    }
    if (specifier === "child_process" || specifier === "node:child_process") {
      return {
        ...fileRequire(specifier),
        spawn: (...args) => {
          const child = (0, import_node_child_process8.spawn)(...args);
          const stdout = child.stdout;
          if (stdout) {
            const originalOn = stdout.on.bind(stdout);
            const dataListeners = /* @__PURE__ */ new Set();
            const pendingChunks = [];
            let buffer = "";
            const flushLine = (line) => {
              const trimmed = line.trim();
              if (!trimmed) return;
              const payload = Buffer.from(trimmed);
              if (dataListeners.size === 0) {
                pendingChunks.push(payload);
                return;
              }
              for (const listener of dataListeners) {
                listener(payload);
              }
            };
            const flushBuffer = () => {
              if (!buffer.trim()) return;
              flushLine(buffer);
              buffer = "";
            };
            originalOn("data", (chunk) => {
              buffer += Buffer.isBuffer(chunk) ? chunk.toString("utf8") : String(chunk);
              let newlineIndex = buffer.indexOf("\n");
              while (newlineIndex >= 0) {
                flushLine(buffer.slice(0, newlineIndex));
                buffer = buffer.slice(newlineIndex + 1);
                newlineIndex = buffer.indexOf("\n");
              }
            });
            child.once("exit", flushBuffer);
            stdout.on = ((event, listener) => {
              if (event === "data") {
                const dataListener = listener;
                dataListeners.add(dataListener);
                while (pendingChunks.length > 0) {
                  const chunk = pendingChunks.shift();
                  if (chunk) dataListener(chunk);
                }
                return stdout;
              }
              return originalOn(event, listener);
            });
          }
          return child;
        }
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
          const sum = (0, import_node_crypto5.createHash)("sha256").update((0, import_node_fs12.readFileSync)(filename)).digest("hex");
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
            const error = new Error(`Request failed with status code ${response.status}`);
            error.response = result;
            error.config = merged;
            throw error;
          }
          return result;
        };
        const instance2 = ((config) => execute(config));
        Object.assign(instance2, {
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
        instance2.default = instance2;
        return instance2;
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
      const request = async (url, options) => {
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
        request,
        fetch,
        ProxyAgent,
        default: { request, fetch, ProxyAgent },
        __esModule: true
      };
    }
    if (specifier === "tar") {
      const extract = async (options) => {
        if (!options?.file || !options.cwd) throw new Error("tar.extract requires file and cwd");
        (0, import_node_fs12.mkdirSync)(options.cwd, { recursive: true });
        const args = ["-xzf", options.file, "-C", options.cwd];
        try {
          if (typeof options.filter === "function") {
            if (options.filter("speedtest")) args.push("speedtest");
          } else if (options.filter === "speedtest") {
            args.push("speedtest");
          }
        } catch {
        }
        await execFileAsync7("/usr/bin/tar", args);
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
        (0, import_node_fs12.mkdirSync)(dir, { recursive: true });
        await execFileAsync7("/usr/bin/unzip", ["-o", file, "-d", dir]);
      };
      return {
        default: extractZip,
        __esModule: true
      };
    }
    if (specifier.startsWith("swift:") || specifier.startsWith("rust:")) {
      const pickColor = async () => {
        if (session2.commandName === "color-wheel") return null;
        const picked = await pickColorWithNativeSampler();
        session2.pickedColor = picked;
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
    filename: (0, import_node_path12.join)(packageRoot, ".tezbar-runtime-bundle.cjs")
  });
  const fn = script.runInContext(context);
  const mod = { exports: {} };
  fn(mod.exports, customRequire, mod, (0, import_node_path12.join)(packageRoot, ".tezbar-runtime-bundle.cjs"), packageRoot);
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
async function runCommandFromPackagePath(packageJsonPath, extensionId, commandName2, argumentValues, preferenceValues, options) {
  const packageRoot = (0, import_node_path12.dirname)(packageJsonPath);
  const pkg = parsePackageJson(packageJsonPath);
  const command = findCommandInManifest(pkg, commandName2);
  const mode = String(command.mode || "").toLowerCase();
  const title = String(command.title || commandName2);
  const entryPath = resolveCommandEntry(packageRoot, commandName2, command);
  console.log(`[Runner] Mode=${mode}, title="${title}", entry=${entryPath}`);
  const bundled = await bundleCommand(entryPath, packageRoot);
  console.log(`[Runner] Bundle size: ${bundled.length} chars`);
  const session2 = {
    id: makeId2("ext-session"),
    extensionId,
    commandName: commandName2,
    commandMode: mode || "view",
    title,
    packageRoot,
    actionHandlers: /* @__PURE__ */ new Map(),
    currentActions: [],
    feedback: [],
    effects: [],
    effectMode: options?.effectMode ?? "system",
    stack: [],
    preferences: preferenceValues ?? getExtensionPreferences(extensionId, commandName2),
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
  if (preferenceValues === void 0 && shouldShowExtensionPreferenceSetup(extensionId, commandName2)) {
    session2.stack = [buildPreferenceSetupRoot(extensionId, commandName2)];
    sessions.set(session2.id, session2);
    pruneSessions();
    return renderCurrentView(session2);
  }
  const moduleExports = runBundle(bundled, packageRoot, session2);
  const commandFn = getCommandExport(moduleExports);
  if (!commandFn) {
    return { ok: false, message: "Extension command entry is not executable." };
  }
  session2.commandFn = commandFn;
  const execArgs = { arguments: argumentValues };
  let result;
  const executePass = async (passLabel) => {
    console.log(`[Runner] ${passLabel}: executing ${extensionId}/${commandName2}`);
    session2.hookIndex = 0;
    session2.pendingPromises = [];
    session2.actionHandlers.clear();
    session2.currentActions = [];
    session2.feedback = [];
    session2.hasStateUpdates = false;
    result = await Promise.resolve(commandFn(execArgs));
    console.log(
      `[Runner] ${passLabel} complete: ${session2.pendingPromises.length} promises, ${session2.hookStates.length} hook states, stateUpdates=${session2.hasStateUpdates}`
    );
  };
  for (let p = 1; p <= INITIAL_RENDER_PASSES; p += 1) {
    await executePass(`Pass ${p}`);
    if (!session2.hasStateUpdates) break;
  }
  const remainingInFlight = [...session2.promiseCache.values()].filter(
    (entry) => entry.promise
  ).length;
  if (remainingInFlight > 0) {
    console.log(
      `[Runner] Initial multi-pass exited with ${remainingInFlight} in-flight promises; polling refresh will pick them up.`
    );
  }
  if (commandName2 === "pick-color" && session2.pickedColor) {
    session2.title = "Color Wheel";
    session2.stack = [
      {
        __jsx: true,
        type: makeToken("Detail"),
        props: {
          markdown: colorWheelMarkdown(),
          initialColor: session2.pickedColor
        }
      }
    ];
    sessions.set(session2.id, session2);
    pruneSessions();
    return renderCurrentView(session2);
  }
  if (mode === "no-view" || !isJsxNode(result)) {
    flushPendingEffects(session2);
    const message = formatFeedback(session2.feedback.at(-1)) || "";
    return {
      ok: true,
      mode: "no-view",
      message,
      effects: [...session2.effects]
    };
  }
  session2.stack = [result];
  sessions.set(session2.id, session2);
  pruneSessions();
  return renderCurrentView(session2);
}
async function runExtensionCommand(request) {
  const extensionId = String(request.extensionId || "").trim();
  const commandName2 = String(request.commandName || "").trim();
  console.log(`[Runner] runExtensionCommand called: ${extensionId}/${commandName2}`);
  if (!extensionId || !commandName2) {
    return { ok: false, message: "Extension id and command name are required." };
  }
  const packagePath = resolveInstalledPackageJsonPath(extensionId);
  if (!packagePath) {
    console.error(`[Runner] Extension not installed: ${extensionId}`);
    return { ok: false, message: `Extension is not installed: ${extensionId}` };
  }
  console.log(`[Runner] Found package.json at ${packagePath}`);
  for (const [sessionId, session2] of sessions) {
    if (session2.extensionId === extensionId && session2.commandName === commandName2) {
      deleteRuntimeSession(sessionId);
    }
  }
  try {
    return await runCommandFromPackagePath(
      packagePath,
      extensionId,
      commandName2,
      request.argumentValues ?? {}
    );
  } catch (error) {
    return {
      ok: false,
      message: error instanceof Error ? error.message : String(error)
    };
  }
}
async function runExtensionCommandFromPackageJson(packageJsonPath, commandName2, argumentValues, preferenceValues, options) {
  const normalizedPath = String(packageJsonPath || "").trim();
  const normalizedCommandName = String(commandName2 || "").trim();
  if (!normalizedPath || !normalizedCommandName) {
    return { ok: false, message: "packageJsonPath and commandName are required." };
  }
  const extensionId = `raycast.${(0, import_node_path12.dirname)(normalizedPath).split("/").pop() || "external"}`;
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
async function invokeExtensionAction(request) {
  const sessionId = String(request.sessionId || "").trim();
  const actionId = String(request.actionId || "").trim();
  if (!sessionId || !actionId) {
    return { ok: false, message: "sessionId and actionId are required." };
  }
  const session2 = sessions.get(sessionId);
  if (!session2) {
    return { ok: false, message: "Extension session not found." };
  }
  if (actionId === "__nav_pop__") {
    if (session2.stack.length > 1) {
      session2.stack.pop();
    }
    return renderCurrentView(session2);
  }
  const handler = session2.actionHandlers.get(actionId);
  if (!handler) {
    return { ok: false, message: "Action is no longer available in this session." };
  }
  try {
    const stackDepthBefore = session2.stack.length;
    await Promise.resolve(handler(request.formValues ?? {}));
    if (session2.stack.length !== stackDepthBefore) {
      return renderCurrentView(session2);
    }
    if (session2.commandFn && session2.hasStateUpdates) {
      return rerenderSessionCommand(session2, "Action");
    }
    if (session2.stack.length > 0) {
      return renderCurrentView(session2);
    }
    return {
      ok: true,
      mode: "no-view",
      message: formatFeedback(session2.feedback.at(-1)) || "",
      effects: [...session2.effects]
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
  for (const session2 of sessions.values()) {
    cleanupRuntimeSession(session2);
  }
  sessions.clear();
}
var import_node_fs12, import_promises2, import_node_child_process8, import_node_crypto5, import_node_http, import_node_os7, import_node_module2, import_node_path12, import_node_stream, import_web, import_node_util7, import_node_v8, import_node_zlib, import_node_vm, RUNTIME_COMPONENT_LIMIT, RUNTIME_RECURSION_LIMIT, SESSIONS_SOFT_LIMIT, INITIAL_RENDER_PASSES, SEARCH_TEXT_RENDER_PASSES, LIST_ITEM_PAGE_SIZE, APPLICATIONS_CACHE_TTL_MS, PROMISE_RESULT_CACHE_TTL_MS, PROMISE_RESULT_MEMORY_CACHE_LIMIT, BUILTIN_SET, JSX_FRAGMENT, REACT_CONTEXT, execFileAsync7, gzipAsync, sessions, promiseResultMemoryCache, applicationsCache, iconProxy;
var init_extension_runner = __esm({
  "src/main/extension-runner.ts"() {
    "use strict";
    init_electron_shim();
    import_node_fs12 = require("node:fs");
    import_promises2 = require("node:fs/promises");
    import_node_child_process8 = require("node:child_process");
    import_node_crypto5 = require("node:crypto");
    import_node_http = require("node:http");
    import_node_os7 = require("node:os");
    import_node_module2 = require("node:module");
    import_node_path12 = require("node:path");
    import_node_stream = require("node:stream");
    import_web = require("node:stream/web");
    import_node_util7 = require("node:util");
    import_node_v8 = require("node:v8");
    import_node_zlib = require("node:zlib");
    import_node_vm = __toESM(require("node:vm"));
    init_esbuild_runtime();
    init_extensionAI();
    init_windowState();
    init_extension_registry();
    RUNTIME_COMPONENT_LIMIT = 1e4;
    RUNTIME_RECURSION_LIMIT = 80;
    SESSIONS_SOFT_LIMIT = 30;
    INITIAL_RENDER_PASSES = 1;
    SEARCH_TEXT_RENDER_PASSES = 1;
    LIST_ITEM_PAGE_SIZE = 30;
    APPLICATIONS_CACHE_TTL_MS = 3e4;
    PROMISE_RESULT_CACHE_TTL_MS = 24 * 60 * 60 * 1e3;
    PROMISE_RESULT_MEMORY_CACHE_LIMIT = 200;
    BUILTIN_SET = new Set(import_node_module2.builtinModules);
    JSX_FRAGMENT = /* @__PURE__ */ Symbol.for("tezbar.jsx.fragment");
    REACT_CONTEXT = /* @__PURE__ */ Symbol.for("react.context");
    execFileAsync7 = (0, import_node_util7.promisify)(import_node_child_process8.execFile);
    gzipAsync = (0, import_node_util7.promisify)(import_node_zlib.gzip);
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

// src/main/ipc.ts
init_electron_shim();
init_windowState();

// src/shared/agent.ts
var AGENT_IPC = {
  RUN: "agent:run",
  CANCEL: "agent:cancel",
  EVENT: "agent:event",
  APPROVE: "agent:approve",
  CAPTURE_ACTIVE_SCREEN: "agent:capture-active-screen"
};

// src/shared/chat.ts
var CHAT_CONTEXT_MAX_TURNS = 16;
var CHAT_IPC = {
  RUN: "chat:run",
  LIST: "chat:list",
  GET: "chat:get",
  APPEND: "chat:append",
  UPDATE_TITLE: "chat:update-title",
  DELETE: "chat:delete",
  CLEAR: "chat:clear"
};

// src/main/agent/bridge.ts
var import_node_child_process2 = require("node:child_process");
var import_node_crypto = require("node:crypto");
var import_node_events = require("node:events");
var import_node_fs2 = require("node:fs");
var import_node_os2 = require("node:os");
var import_node_path2 = __toESM(require("node:path"));
init_electron_shim();

// src/main/agent/tools.ts
function str(value, fallback = "") {
  return typeof value === "string" ? value : fallback;
}
function truncate(value, max = 60) {
  const clean = value.replace(/\s+/g, " ").trim();
  if (clean.length <= max) return clean;
  return `${clean.slice(0, max - 1)}\u2026`;
}
var PI_TOOLS = {
  read: {
    name: "read",
    description: "Read a file (optional offset/limit for large files)",
    argKeys: ["path", "offset", "limit"],
    mutates: false,
    label: (args) => `read ${truncate(str(args.path, "<path>"))}`
  },
  bash: {
    name: "bash",
    description: "Run a shell command (optional timeout ms)",
    argKeys: ["command", "timeout"],
    mutates: true,
    label: (args) => `bash: ${truncate(str(args.command, "<cmd>"))}`
  },
  edit: {
    name: "edit",
    description: "Apply one or more oldText/newText edits to a file",
    argKeys: ["path", "edits"],
    mutates: true,
    label: (args) => {
      const edits = Array.isArray(args.edits) ? args.edits.length : 0;
      return `edit ${truncate(str(args.path, "<path>"))} (${edits} change${edits === 1 ? "" : "s"})`;
    }
  },
  write: {
    name: "write",
    description: "Overwrite (or create) a file with full content",
    argKeys: ["path", "content"],
    mutates: true,
    label: (args) => `write ${truncate(str(args.path, "<path>"))}`
  },
  grep: {
    name: "grep",
    description: "Ripgrep-backed content search (glob / literal / context)",
    argKeys: [
      "pattern",
      "path",
      "glob",
      "ignoreCase",
      "literal",
      "context",
      "limit"
    ],
    mutates: false,
    label: (args) => `grep ${truncate(str(args.pattern, "<pattern>"))}`
  },
  find: {
    name: "find",
    description: "Find files by filename pattern",
    argKeys: ["pattern", "path", "limit"],
    mutates: false,
    label: (args) => `find ${truncate(str(args.pattern, "<pattern>"))}`
  },
  ls: {
    name: "ls",
    description: "List directory contents",
    argKeys: ["path", "limit"],
    mutates: false,
    label: (args) => `ls ${truncate(str(args.path, "."))}`
  }
};
function labelForToolCall(toolName, args) {
  const descriptor = PI_TOOLS[toolName];
  if (!descriptor) return `${toolName}`;
  const safeArgs = args && typeof args === "object" ? args : {};
  return descriptor.label(safeArgs);
}

// src/main/agent/loop.ts
function errorDetail(result) {
  if (!result || typeof result !== "object") return void 0;
  const content = result.content;
  if (!Array.isArray(content)) return void 0;
  for (const item of content) {
    if (item && typeof item === "object" && item.type === "text") {
      const text = item.text;
      if (typeof text === "string" && text.trim()) {
        return text.replace(/\s+/g, " ").trim().slice(0, 160);
      }
    }
  }
  return void 0;
}
function createLoopDriver(callbacks) {
  const tracker = {
    stages: /* @__PURE__ */ new Map(),
    nextIndex: 0,
    currentText: "",
    ended: false
  };
  const emitStage = (stage) => {
    tracker.stages.set(`stage:${stage.index}`, stage);
    callbacks.onStage(stage);
  };
  const updateStageStatus = (toolCallId, status, detail) => {
    const existing = tracker.stages.get(toolCallId);
    if (!existing) return;
    const next = detail ? { ...existing, status, detail } : { ...existing, status };
    tracker.stages.set(toolCallId, next);
    callbacks.onStage(next);
  };
  const asString = (v, fallback = "") => typeof v === "string" ? v : fallback;
  const asBool = (v) => v === true;
  const asNumber = (v, fallback = 0) => typeof v === "number" && Number.isFinite(v) ? v : fallback;
  const asRecord2 = (v) => v && typeof v === "object" && !Array.isArray(v) ? v : void 0;
  return function handle(event) {
    switch (event.type) {
      case "agent_start": {
        tracker.stages.clear();
        tracker.nextIndex = 0;
        tracker.currentText = "";
        tracker.ended = false;
        return;
      }
      case "message_update": {
        const ev = asRecord2(event["assistantMessageEvent"]);
        if (!ev) return;
        const subType = asString(ev["type"]);
        const delta = ev["delta"];
        const content = ev["content"];
        if (subType === "text_delta" && typeof delta === "string") {
          tracker.currentText += delta;
          callbacks.onMessageDelta(delta);
        } else if (subType === "text_end" && typeof content === "string") {
          tracker.currentText = content;
        }
        return;
      }
      case "tool_execution_start": {
        const toolCallId = asString(event["toolCallId"]);
        if (!toolCallId) return;
        const index = tracker.nextIndex++;
        const stage = {
          index,
          label: labelForToolCall(asString(event["toolName"], "tool"), event["args"]),
          status: "running"
        };
        tracker.stages.set(toolCallId, stage);
        emitStage(stage);
        return;
      }
      case "tool_execution_end": {
        const toolCallId = asString(event["toolCallId"]);
        if (!toolCallId) return;
        const isError = asBool(event["isError"]);
        updateStageStatus(
          toolCallId,
          isError ? "failed" : "done",
          isError ? errorDetail(event["result"]) : void 0
        );
        return;
      }
      case "auto_retry_start": {
        const attempt = asNumber(event["attempt"]);
        const maxAttempts = asNumber(event["maxAttempts"]);
        const stage = {
          index: tracker.nextIndex++,
          label: `retry (${attempt}/${maxAttempts})`,
          status: "running",
          detail: asString(event["errorMessage"]).slice(0, 160) || void 0
        };
        tracker.stages.set(`retry:${attempt}`, stage);
        emitStage(stage);
        return;
      }
      case "auto_retry_end": {
        const retryKey = Array.from(tracker.stages.keys()).reverse().find((k) => k.startsWith("retry:"));
        if (retryKey) {
          updateStageStatus(
            retryKey,
            asBool(event["success"]) ? "done" : "failed",
            asString(event["finalError"]) || void 0
          );
        }
        return;
      }
      case "agent_end": {
        tracker.ended = true;
        if (tracker.currentText.trim()) {
          callbacks.onAnswer(tracker.currentText.trim());
        }
        callbacks.onDone();
        return;
      }
      default:
        return;
    }
  };
}

// src/main/agent/observer.ts
function asRecord(value) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return void 0;
  return value;
}
function modelLabel(state) {
  const m = state.model;
  if (!m) return void 0;
  if (m.provider && m.id) return `${m.provider}/${m.id}`;
  return m.id;
}
async function observe(cwd, query) {
  const [stateRaw, statsRaw] = await Promise.all([
    query({ type: "get_state" }).catch(() => null),
    query({ type: "get_session_stats" }).catch(() => null)
  ]);
  const state = asRecord(stateRaw)?.data ?? {};
  const stats = asRecord(statsRaw)?.data ?? {};
  return {
    cwd,
    sessionFile: state.sessionFile ?? stats.sessionFile ?? void 0,
    model: modelLabel(state),
    thinkingLevel: state.thinkingLevel,
    isStreaming: state.isStreaming === true,
    messageCount: state.messageCount ?? 0,
    pendingMessageCount: state.pendingMessageCount ?? 0,
    toolCalls: stats.toolCalls ?? 0,
    contextUsage: stats.contextUsage
  };
}

// src/main/agent/prompt.ts
var MAX_AGENT_IMAGES = 4;
var MAX_AGENT_IMAGE_BYTES = 8 * 1024 * 1024;
var SUPPORTED_IMAGE_TYPES = /* @__PURE__ */ new Set([
  "image/png",
  "image/jpeg",
  "image/webp"
]);
function rawBase64(data) {
  const trimmed = data.trim();
  const comma = trimmed.indexOf(",");
  return trimmed.startsWith("data:image/") && comma >= 0 ? trimmed.slice(comma + 1) : trimmed;
}
function estimatedDecodedBytes(data) {
  const padding = data.endsWith("==") ? 2 : data.endsWith("=") ? 1 : 0;
  return Math.max(0, Math.floor(data.length * 3 / 4) - padding);
}
function normalizeAgentImages(images) {
  if (!images?.length) return [];
  if (images.length > MAX_AGENT_IMAGES) {
    throw new Error(`Agent accepts at most ${MAX_AGENT_IMAGES} images per prompt`);
  }
  return images.map((image) => {
    if (!SUPPORTED_IMAGE_TYPES.has(image.mimeType)) {
      throw new Error(`Unsupported agent image type: ${image.mimeType}`);
    }
    const data = rawBase64(image.data);
    if (!data || !/^[A-Za-z0-9+/]+={0,2}$/.test(data)) {
      throw new Error("Agent image is not valid base64 data");
    }
    if (estimatedDecodedBytes(data) > MAX_AGENT_IMAGE_BYTES) {
      throw new Error("Agent image exceeds the 8 MB limit");
    }
    return { type: "image", data, mimeType: image.mimeType };
  });
}
function buildPromptCommand(message, images) {
  const normalized = normalizeAgentImages(images);
  return normalized.length > 0 ? { type: "prompt", message, images: normalized } : { type: "prompt", message };
}

// src/main/agent/bridge.ts
var PI_BIN_CANDIDATES = [
  // Where pnpm installs global bins for this user (matches `which pi`
  // at the time this bridge was written). We resolve at runtime so a
  // reinstall or version bump does not require a rebuild.
  import_node_path2.default.join((0, import_node_os2.homedir)(), "Library", "pnpm", "pi"),
  import_node_path2.default.join((0, import_node_os2.homedir)(), ".local", "share", "pnpm", "pi")
];
var OPENCODE_PI_EXTENSION = import_node_path2.default.join(
  (0, import_node_os2.homedir)(),
  ".pi",
  "agent",
  "extensions",
  "opencode",
  "index.ts"
);
function resolveRaymesPiExtension() {
  const resourcesPath = process.resourcesPath;
  const candidates = [
    process.env["RAYMES_PI_EXTENSION"],
    import_node_path2.default.join(process.cwd(), "src", "main", "agent", "raymes-pi-policy.ts"),
    ...app.isPackaged && resourcesPath ? [import_node_path2.default.join(resourcesPath, "agent", "raymes-pi-policy.ts")] : []
  ];
  return candidates.find(
    (candidate) => Boolean(candidate && (0, import_node_fs2.existsSync)(candidate))
  );
}
function resolvePiBinary(override) {
  if (override && override.trim()) return override.trim();
  const envOverride = process.env["RAYMES_PI_BIN"];
  if (envOverride && envOverride.trim()) return envOverride.trim();
  for (const candidate of PI_BIN_CANDIDATES) {
    if ((0, import_node_fs2.existsSync)(candidate)) return candidate;
  }
  return "pi";
}
function makeId() {
  return (0, import_node_crypto.randomUUID)();
}
function writeCommand(child, command) {
  const line = `${JSON.stringify(command)}
`;
  child.stdin.write(line);
}
function spawnRpc(options) {
  const args = ["--mode", "rpc"];
  if (options.ephemeral) args.push("--no-session");
  args.push("--no-extensions");
  if (options.model) args.push("--model", options.model);
  const raymesPiExtension = resolveRaymesPiExtension();
  if (raymesPiExtension) args.push("--extension", raymesPiExtension);
  if (options.model?.startsWith("opencode/") && (0, import_node_fs2.existsSync)(OPENCODE_PI_EXTENSION)) {
    args.push("--extension", OPENCODE_PI_EXTENSION);
  }
  args.push(...options.extraArgs);
  const child = (0, import_node_child_process2.spawn)(options.piBin, args, {
    cwd: options.cwd,
    env: {
      ...process.env,
      ...options.raymesProviderJson ? { RAYMES_PI_PROVIDER_JSON: options.raymesProviderJson } : {},
      ...options.raymesAlwaysAllowJson ? { RAYMES_PI_ALWAYS_ALLOW_JSON: options.raymesAlwaysAllowJson } : {}
    },
    stdio: ["pipe", "pipe", "pipe"]
  });
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  return child;
}
function shouldSuppressPiStderr(line) {
  return /^Warning: No models match pattern "(?:kiro-cli\/|opencode\/opencode\/)[^"]+"$/.test(
    line.trim()
  );
}
async function handleExtensionUiRequest(handle, msg) {
  const id = msg.id;
  if (typeof id !== "string") return;
  if (msg.method === "confirm") {
    const title = msg.title || "Allow command?";
    const command = msg.message || "";
    let confirmed = false;
    try {
      confirmed = await handle.requestApproval?.({ title, command }) ?? false;
    } catch {
      confirmed = false;
    }
    writeCommand(handle.child, {
      type: "extension_ui_response",
      id,
      confirmed
    });
    return;
  }
  writeCommand(handle.child, { type: "extension_ui_response", id, cancelled: true });
}
function attachLineReader(stream, onLine) {
  let buffer = "";
  stream.on("data", (chunk) => {
    buffer += typeof chunk === "string" ? chunk : chunk.toString("utf8");
    let newlineAt = buffer.indexOf("\n");
    while (newlineAt >= 0) {
      const raw = buffer.slice(0, newlineAt);
      buffer = buffer.slice(newlineAt + 1);
      const line = raw.endsWith("\r") ? raw.slice(0, -1) : raw;
      if (line.length > 0) onLine(line);
      newlineAt = buffer.indexOf("\n");
    }
  });
}
function attachHandlers(handle, onStderrLine) {
  attachLineReader(handle.child.stdout, (line) => {
    let parsed;
    try {
      parsed = JSON.parse(line);
    } catch {
      return;
    }
    if (!parsed || typeof parsed !== "object") return;
    const msg = parsed;
    if (msg.type === "response" && typeof msg.id === "string") {
      const pending = handle.pending.get(msg.id);
      if (!pending) return;
      handle.pending.delete(msg.id);
      if (msg.success === true) {
        pending.resolve(parsed);
      } else {
        pending.reject(new Error(msg.error || "pi rpc command failed"));
      }
      return;
    }
    if (msg.type === "extension_ui_request") {
      void handleExtensionUiRequest(handle, msg);
      return;
    }
    if (typeof msg.type === "string") {
      handle.onEvent(parsed);
    }
  });
  attachLineReader(handle.child.stderr, (line) => {
    if (shouldSuppressPiStderr(line)) return;
    handle.stderrBuffer.push(line);
    onStderrLine?.(line);
    if (handle.stderrBuffer.length > 50) handle.stderrBuffer.shift();
  });
  handle.child.on("close", () => {
    handle.closed = true;
    const pendings = Array.from(handle.pending.values());
    for (let i = 0; i < pendings.length; i++) {
      const pending = pendings[i];
      if (pending) pending.reject(new Error("pi rpc session closed before response"));
    }
    handle.pending.clear();
  });
  handle.child.on("error", (err) => {
    handle.closed = true;
    const pendings = Array.from(handle.pending.values());
    for (let i = 0; i < pendings.length; i++) {
      const pending = pendings[i];
      if (pending) pending.reject(err);
    }
    handle.pending.clear();
  });
}
async function sendAndAwait(handle, command, timeoutMs) {
  if (handle.closed) throw new Error("pi rpc session already closed");
  const id = makeId();
  return new Promise((resolve4, reject) => {
    const timer = setTimeout(() => {
      handle.pending.delete(id);
      reject(new Error(`pi rpc command timed out after ${timeoutMs}ms: ${command.type}`));
    }, timeoutMs);
    handle.pending.set(id, {
      resolve: (value) => {
        clearTimeout(timer);
        resolve4(value);
      },
      reject: (err) => {
        clearTimeout(timer);
        reject(err);
      }
    });
    writeCommand(handle.child, { ...command, id });
  });
}
function createBridge() {
  const ownedChildren = /* @__PURE__ */ new Set();
  function trackChild(child) {
    ownedChildren.add(child);
    child.on("close", () => ownedChildren.delete(child));
  }
  return {
    async run(task, options = {}) {
      if (!task.trim()) {
        throw new Error("agent.run: task is empty");
      }
      const runId = options.runId ?? makeId();
      const cwd = options.cwd ?? process.cwd();
      const piBin = resolvePiBinary(options.piBin);
      const ephemeral = options.ephemeral !== false;
      const stages = [];
      let finalAnswer = "";
      const driver = createLoopDriver({
        onStage: (stage) => {
          const existing = stages.findIndex((s) => s.index === stage.index);
          if (existing >= 0) stages[existing] = stage;
          else stages.push(stage);
          options.onStage?.(stage);
        },
        onMessageDelta: (delta) => {
          options.onMessageDelta?.(delta);
        },
        onAnswer: (text) => {
          finalAnswer = text;
          options.onAnswer?.(text);
        },
        onDone: () => {
        },
        onError: (message) => {
          throw new Error(message);
        }
      });
      const child = spawnRpc({
        cwd,
        piBin,
        ephemeral,
        model: options.model,
        raymesProviderJson: options.raymesProviderJson,
        raymesAlwaysAllowJson: options.raymesAlwaysAllowJson,
        extraArgs: options.extraArgs ?? []
      });
      trackChild(child);
      console.log("[tezbar:agent] spawn", {
        runId,
        piBin,
        cwd,
        ephemeral,
        model: options.model ?? "(default)",
        taskChars: task.length
      });
      let agentEndResolved = false;
      let agentEnded = () => void 0;
      const agentEndPromise = new Promise((resolve4) => {
        agentEnded = () => {
          if (agentEndResolved) return;
          agentEndResolved = true;
          resolve4();
        };
      });
      const handle = {
        child,
        pending: /* @__PURE__ */ new Map(),
        stderrBuffer: [],
        closed: false,
        requestApproval: options.requestApproval,
        onEvent: (event) => {
          driver(event);
          if (event.type === "agent_end") agentEnded();
        }
      };
      attachHandlers(handle, options.onStderrLine);
      const onAbort = () => {
        if (handle.closed) return;
        try {
          writeCommand(child, { type: "abort", id: makeId() });
        } catch {
        }
        setTimeout(() => {
          if (!handle.closed && !child.killed) child.kill("SIGTERM");
        }, 500);
      };
      options.signal?.addEventListener("abort", onAbort, { once: true });
      try {
        await sendAndAwait(handle, buildPromptCommand(task, options.images), 15e3);
        let runTimeout;
        try {
          await Promise.race([
            agentEndPromise,
            (0, import_node_events.once)(child, "close").then(() => void 0),
            new Promise((_resolve, reject) => {
              runTimeout = setTimeout(
                () => reject(
                  new Error(`Agent run timed out after ${options.timeoutMs ?? 15 * 6e4}ms`)
                ),
                options.timeoutMs ?? 15 * 6e4
              );
            })
          ]);
        } finally {
          if (runTimeout) clearTimeout(runTimeout);
        }
        if (options.signal?.aborted) {
          throw new Error("Agent run aborted");
        }
        if (handle.closed && !agentEndResolved) {
          const tail = handle.stderrBuffer.slice(-8).join("\n").trim();
          throw new Error(
            tail ? `pi exited before finishing:
${tail}` : "pi exited before finishing"
          );
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        const tail = handle.stderrBuffer.filter((line) => !message.includes(line)).slice(-6).join("\n").trim();
        throw new Error(tail ? `${message}
${tail}` : message);
      } finally {
        options.signal?.removeEventListener("abort", onAbort);
        if (!handle.closed) {
          try {
            writeCommand(child, { type: "abort", id: makeId() });
          } catch {
          }
          child.stdin.end();
          setTimeout(() => {
            if (!handle.closed && !child.killed) child.kill("SIGTERM");
          }, 500);
        }
      }
      return { runId, answer: finalAnswer, stages };
    },
    async query(command, queryOptions = {}) {
      const cwd = queryOptions.cwd ?? process.cwd();
      const piBin = resolvePiBinary(queryOptions.piBin);
      const timeoutMs = queryOptions.timeoutMs ?? 1e4;
      const child = spawnRpc({ cwd, piBin, ephemeral: true, extraArgs: [] });
      trackChild(child);
      const handle = {
        child,
        pending: /* @__PURE__ */ new Map(),
        stderrBuffer: [],
        closed: false,
        requestApproval: void 0,
        onEvent: () => void 0
      };
      attachHandlers(handle);
      try {
        const result = await sendAndAwait(handle, command, timeoutMs);
        return result;
      } finally {
        try {
          writeCommand(child, { type: "abort", id: makeId() });
        } catch {
        }
        child.stdin.end();
        setTimeout(() => {
          if (!handle.closed && !child.killed) child.kill("SIGTERM");
        }, 250);
      }
    },
    async observe(observeOptions = {}) {
      const cwd = observeOptions.cwd ?? process.cwd();
      const piBin = resolvePiBinary(observeOptions.piBin);
      return observe(cwd, (command) => this.query(command, { cwd, piBin, timeoutMs: 5e3 }));
    },
    dispose() {
      const children = Array.from(ownedChildren.values());
      for (let i = 0; i < children.length; i++) {
        const child = children[i];
        if (child && !child.killed) child.kill("SIGTERM");
      }
      ownedChildren.clear();
    }
  };
}
var sharedBridge;
function getSharedBridge() {
  if (!sharedBridge) sharedBridge = createBridge();
  return sharedBridge;
}
function disposeSharedBridge() {
  sharedBridge?.dispose();
  sharedBridge = void 0;
}

// src/main/agent/imageContext.ts
var import_node_child_process3 = require("node:child_process");
var import_node_fs3 = require("node:fs");
var import_promises = require("node:fs/promises");
var import_node_os3 = require("node:os");
var import_node_path3 = __toESM(require("node:path"));
var import_node_util2 = require("node:util");
init_electron_shim();
var execFileAsync2 = (0, import_node_util2.promisify)(import_node_child_process3.execFile);
var MAX_OCR_CHARS = 4e4;
function screenOcrHelperPath() {
  const resourcesPath = process.resourcesPath;
  const candidates = [
    process.env["SCREENOCR_HELPER_PATH"],
    app.isPackaged && resourcesPath ? import_node_path3.default.join(resourcesPath, "app.asar.unpacked", "native", "screenocr", "screenocr-helper") : void 0,
    import_node_path3.default.join(process.cwd(), "native", "screenocr", "screenocr-helper")
  ];
  return candidates.find((candidate) => Boolean(candidate && (0, import_node_fs3.existsSync)(candidate)));
}
function imageExtension(mimeType) {
  if (mimeType === "image/jpeg") return "jpg";
  if (mimeType === "image/webp") return "webp";
  return "png";
}
async function extractTextFromAgentImages(images) {
  if (process.platform !== "darwin" || images.length === 0) return "";
  const helperPath = screenOcrHelperPath();
  if (!helperPath) return "";
  const workDir = await (0, import_promises.mkdtemp)(import_node_path3.default.join((0, import_node_os3.tmpdir)(), "raymes-agent-image-"));
  try {
    const textBlocks = [];
    for (let index = 0; index < images.length; index += 1) {
      const image = images[index];
      if (!image) continue;
      const imagePath = import_node_path3.default.join(workDir, `attachment-${index}.${imageExtension(image.mimeType)}`);
      await (0, import_promises.writeFile)(imagePath, Buffer.from(image.data, "base64"));
      const { stdout } = await execFileAsync2(
        helperPath,
        [
          "recognize-text",
          JSON.stringify({
            imagePath,
            fast: false,
            languageCorrection: true,
            ignoreLineBreaks: false
          })
        ],
        { timeout: 45e3, maxBuffer: 4 * 1024 * 1024 }
      );
      const response = JSON.parse(stdout.trim());
      if (!response.ok) throw new Error(response.error || "Local screen text extraction failed");
      if (response.value?.trim()) textBlocks.push(response.value.trim());
    }
    return textBlocks.join("\n\n").slice(0, MAX_OCR_CHARS);
  } finally {
    await (0, import_promises.rm)(workDir, { recursive: true, force: true });
  }
}

// src/main/chat/sessionStore.ts
init_electron_shim();

// src/main/better-sqlite3-shim.ts
var import_bun_sqlite = require("bun:sqlite");
var StatementShim = class {
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
var DatabaseShim = class {
  _db;
  constructor(filename) {
    this._db = new import_bun_sqlite.Database(filename);
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
  transaction(fn) {
    return this._db.transaction(fn);
  }
};
var better_sqlite3_shim_default = DatabaseShim;

// src/main/chat/sessionStore.ts
var import_node_fs4 = require("node:fs");
var import_node_path4 = require("node:path");
function safeParseStages(raw) {
  if (!raw) return void 0;
  try {
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return void 0;
    const stages = parsed.filter((item) => {
      if (!item || typeof item !== "object") return false;
      const stage = item;
      return typeof stage.index === "number" && typeof stage.label === "string" && (stage.status === "running" || stage.status === "done" || stage.status === "failed");
    });
    return stages.length > 0 ? stages : void 0;
  } catch {
    return void 0;
  }
}
function safeParseAttachments(raw) {
  if (!raw) return void 0;
  try {
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return void 0;
    const attachments = parsed.filter((item) => {
      if (!item || typeof item !== "object") return false;
      const attachment = item;
      return attachment.kind === "image" && typeof attachment.name === "string" && (attachment.mimeType === "image/png" || attachment.mimeType === "image/jpeg" || attachment.mimeType === "image/webp");
    });
    return attachments.length > 0 ? attachments : void 0;
  } catch {
    return void 0;
  }
}
function dbPath() {
  const dir = (0, import_node_path4.join)(app.getPath("userData"), "chat");
  (0, import_node_fs4.mkdirSync)(dir, { recursive: true });
  return (0, import_node_path4.join)(dir, "sessions.sqlite3");
}
var ChatSessionDatabase = class {
  _db = null;
  _initPromise = null;
  get db() {
    if (!this._db) {
      throw new Error("Database not initialized - call ensureInitialized() first");
    }
    return this._db;
  }
  async ensureInitialized() {
    if (this._initPromise) return this._initPromise;
    this._initPromise = new Promise((resolve4) => {
      setImmediate(() => {
        this._db = new better_sqlite3_shim_default(dbPath());
        this._db.pragma("journal_mode = WAL");
        this._db.pragma("synchronous = NORMAL");
        this._db.pragma("foreign_keys = ON");
        this.bootstrap();
        resolve4();
      });
    });
    return this._initPromise;
  }
  bootstrap() {
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS chat_sessions (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
      );
      CREATE TABLE IF NOT EXISTS chat_turns (
        id TEXT PRIMARY KEY,
        session_id TEXT NOT NULL,
        role TEXT NOT NULL,
        text TEXT NOT NULL,
        stages_json TEXT,
        attachments_json TEXT,
        error TEXT,
        created_at INTEGER NOT NULL,
        FOREIGN KEY(session_id) REFERENCES chat_sessions(id) ON DELETE CASCADE
      );
      CREATE INDEX IF NOT EXISTS idx_chat_turns_session ON chat_turns(session_id);
      CREATE INDEX IF NOT EXISTS idx_chat_sessions_updated ON chat_sessions(updated_at DESC);
    `);
    try {
      this.db.exec(`ALTER TABLE chat_turns ADD COLUMN attachments_json TEXT;`);
    } catch {
    }
  }
  listSessions(limit = 100) {
    const rows = this.db.prepare(
      `SELECT s.id, s.title, s.created_at, s.updated_at,
                (SELECT COUNT(*) FROM chat_turns t WHERE t.session_id = s.id) AS turn_count,
                (SELECT t.text FROM chat_turns t
                   WHERE t.session_id = s.id AND t.role = 'user'
                   ORDER BY t.created_at DESC LIMIT 1) AS preview
         FROM chat_sessions s
         ORDER BY s.updated_at DESC
         LIMIT ?`
    ).all(limit);
    return rows.map((r) => ({
      id: r.id,
      title: r.title,
      createdAt: r.created_at,
      updatedAt: r.updated_at,
      turnCount: Number(r.turn_count),
      preview: r.preview ?? ""
    }));
  }
  getSession(id) {
    const sessionRow = this.db.prepare(
      `SELECT id, title, created_at, updated_at FROM chat_sessions WHERE id = ?`
    ).get(id);
    if (!sessionRow) return null;
    const turnRows = this.db.prepare(
      `SELECT id, session_id, role, text, stages_json, attachments_json, error, created_at
         FROM chat_turns WHERE session_id = ? ORDER BY created_at ASC`
    ).all(id);
    return {
      id: sessionRow.id,
      title: sessionRow.title,
      createdAt: sessionRow.created_at,
      updatedAt: sessionRow.updated_at,
      turns: turnRows.map((t) => ({
        id: t.id,
        role: t.role === "assistant" ? "assistant" : "user",
        text: t.text,
        stages: safeParseStages(t.stages_json),
        attachments: safeParseAttachments(t.attachments_json),
        error: t.error ?? void 0,
        createdAt: t.created_at
      }))
    };
  }
  upsertSession(session2) {
    this.db.prepare(
      `INSERT INTO chat_sessions(id, title, created_at, updated_at)
         VALUES(?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           title = excluded.title,
           updated_at = excluded.updated_at`
    ).run(session2.id, session2.title, session2.createdAt, session2.updatedAt);
  }
  appendTurn(sessionId, turn) {
    this.db.prepare(
      `INSERT INTO chat_turns(id, session_id, role, text, stages_json, attachments_json, error, created_at)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           text = excluded.text,
           stages_json = excluded.stages_json,
           attachments_json = excluded.attachments_json,
           error = excluded.error`
    ).run(
      turn.id,
      sessionId,
      turn.role,
      turn.text,
      turn.stages ? JSON.stringify(turn.stages) : null,
      turn.attachments ? JSON.stringify(
        turn.attachments.map((attachment) => {
          const metadata = { ...attachment };
          delete metadata.data;
          return metadata;
        })
      ) : null,
      turn.error ?? null,
      turn.createdAt
    );
    this.db.prepare(`UPDATE chat_sessions SET updated_at = ? WHERE id = ?`).run(Math.max(turn.createdAt, Date.now()), sessionId);
  }
  updateTitle(sessionId, title) {
    this.db.prepare(`UPDATE chat_sessions SET title = ?, updated_at = ? WHERE id = ?`).run(title, Date.now(), sessionId);
  }
  deleteSession(id) {
    const info = this.db.prepare(`DELETE FROM chat_sessions WHERE id = ?`).run(id);
    return info.changes > 0;
  }
  clearAll() {
    this.db.exec(`DELETE FROM chat_turns; DELETE FROM chat_sessions;`);
  }
};
var instance = null;
function store() {
  if (!instance) instance = new ChatSessionDatabase();
  return instance;
}
async function listChatSessions(limit) {
  await store().ensureInitialized();
  return store().listSessions(limit);
}
async function getChatSession(id) {
  await store().ensureInitialized();
  return store().getSession(id);
}
async function upsertChatSession(session2) {
  await store().ensureInitialized();
  store().upsertSession(session2);
}
async function appendChatTurn(sessionId, turn) {
  await store().ensureInitialized();
  store().appendTurn(sessionId, turn);
}
async function updateChatSessionTitle(sessionId, title) {
  await store().ensureInitialized();
  store().updateTitle(sessionId, title);
}
async function deleteChatSession(id) {
  await store().ensureInitialized();
  return store().deleteSession(id);
}
async function clearAllChatSessions() {
  await store().ensureInitialized();
  store().clearAll();
}

// src/shared/ipc.ts
var IPC_CHANNELS = {
  QUERY: "query",
  SEARCH_ALL: "search:all",
  PATH_COMPLETE: "path:complete",
  DIRECTORY_VISIT_RECORD: "directory-visit:record",
  SEARCH_EXECUTE: "search:execute",
  SEARCH_BENCHMARK_RUN: "search:benchmark:run",
  SEARCH_BENCHMARK_HISTORY: "search:benchmark:history",
  AI_ACTION: "ai:action",
  VOICE_TTS_SPEAK: "voice:tts:speak",
  VOICE_TTS_STOP: "voice:tts:stop",
  VOICE_STT_MODES: "voice:stt:modes",
  VOICE_STT_TRANSCRIBE: "voice:stt:transcribe",
  VOICE_MODELS_LIST: "voice:models:list",
  VOICE_MODEL_DOWNLOAD: "voice:model:download",
  VOICE_MODEL_GET_SELECTED: "voice:model:get-selected",
  VOICE_MODEL_SET_SELECTED: "voice:model:set-selected"
};
function isRecord(value) {
  return typeof value === "object" && value !== null;
}
function parseSearchExecuteRequest(payload) {
  if (!isRecord(payload) || !("action" in payload)) {
    throw new Error("Invalid search execute payload");
  }
  const action = payload.action;
  const context = isRecord(payload.context) ? payload.context : void 0;
  return { action, context };
}
function parseVoiceSpeakRequest(payload) {
  if (!isRecord(payload) || typeof payload.text !== "string") {
    throw new Error("Invalid voice speak payload");
  }
  return { text: payload.text };
}
function toUint8Array(value) {
  if (value instanceof Uint8Array) {
    return value;
  }
  if (value instanceof ArrayBuffer) {
    return new Uint8Array(value);
  }
  if (ArrayBuffer.isView(value)) {
    return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  }
  if (Array.isArray(value) && value.every((item) => typeof item === "number")) {
    return Uint8Array.from(value);
  }
  return null;
}
function parseVoiceTranscribeRequest(payload) {
  if (!isRecord(payload)) {
    throw new Error("Invalid voice transcription payload");
  }
  const audioBytes = toUint8Array(payload.audioBytes);
  if (!audioBytes || audioBytes.byteLength === 0) {
    throw new Error("Voice transcription payload must include audio bytes");
  }
  const mimeType = typeof payload.mimeType === "string" ? payload.mimeType : void 0;
  const language = typeof payload.language === "string" ? payload.language : void 0;
  return { audioBytes, mimeType, language };
}
function parseVoiceModelRequest(payload) {
  if (!isRecord(payload) || typeof payload.modelId !== "string") {
    throw new Error("Invalid voice model payload");
  }
  return { modelId: payload.modelId };
}
function parseAiActionRequest(payload) {
  if (!isRecord(payload) || typeof payload.instruction !== "string") {
    throw new Error("Invalid AI action payload");
  }
  return {
    instruction: payload.instruction,
    selectedText: typeof payload.selectedText === "string" ? payload.selectedText : void 0,
    appContext: typeof payload.appContext === "string" ? payload.appContext : void 0,
    allowAutomation: payload.allowAutomation === true,
    redactSensitive: payload.redactSensitive !== false
  };
}

// src/main/appIcon.ts
init_electron_shim();
var import_node_child_process4 = require("node:child_process");
var import_node_crypto2 = require("node:crypto");
var import_node_fs5 = require("node:fs");
var import_node_path5 = require("node:path");
var import_node_util3 = require("node:util");
var execFileAsync3 = (0, import_node_util3.promisify)(import_node_child_process4.execFile);
var appIconCache = /* @__PURE__ */ new Map();
async function appIconDataUrl(appPath) {
  if (appIconCache.has(appPath)) return appIconCache.get(appPath) ?? void 0;
  try {
    const resourceDir = (0, import_node_path5.join)(appPath, "Contents", "Resources");
    let iconName;
    try {
      const { stdout } = await execFileAsync3("/usr/bin/plutil", [
        "-extract",
        "CFBundleIconFile",
        "raw",
        "-o",
        "-",
        (0, import_node_path5.join)(appPath, "Contents", "Info.plist")
      ]);
      const configured = stdout.trim();
      if (configured) iconName = (0, import_node_path5.extname)(configured) ? configured : `${configured}.icns`;
    } catch {
    }
    const resourceEntries = (0, import_node_fs5.readdirSync)(resourceDir);
    if (!iconName || !(0, import_node_fs5.existsSync)((0, import_node_path5.join)(resourceDir, iconName))) {
      const appName = (0, import_node_path5.basename)(appPath, ".app").toLowerCase();
      iconName = resourceEntries.find((entry) => entry.toLowerCase() === `${appName}.icns`) ?? resourceEntries.find((entry) => entry.toLowerCase().endsWith(".icns"));
    }
    if (!iconName) {
      appIconCache.set(appPath, null);
      return void 0;
    }
    const iconPath = (0, import_node_path5.join)(resourceDir, iconName);
    const cacheDir = (0, import_node_path5.join)(app.getPath("userData"), "icon-cache");
    (0, import_node_fs5.mkdirSync)(cacheDir, { recursive: true });
    const cacheName = `${(0, import_node_crypto2.createHash)("sha1").update(iconPath).digest("hex")}-64.png`;
    const pngPath = (0, import_node_path5.join)(cacheDir, cacheName);
    if (!(0, import_node_fs5.existsSync)(pngPath)) {
      await execFileAsync3("/usr/bin/sips", [
        "-Z",
        "64",
        "-s",
        "format",
        "png",
        iconPath,
        "--out",
        pngPath
      ]);
    }
    const dataUrl = `data:image/png;base64,${(0, import_node_fs5.readFileSync)(pngPath).toString("base64")}`;
    appIconCache.set(appPath, dataUrl);
    return dataUrl;
  } catch {
    appIconCache.set(appPath, null);
    return void 0;
  }
}

// src/main/pathIcons.ts
init_electron_shim();
var import_node_child_process5 = require("node:child_process");
var import_node_crypto3 = require("node:crypto");
var import_node_fs6 = require("node:fs");
var import_node_path6 = require("node:path");
var import_node_util4 = require("node:util");
var execFileAsync4 = (0, import_node_util4.promisify)(import_node_child_process5.execFile);
var FILE_ICON_STYLES = {
  ".c": { label: "C", color: "#6b8dd6" },
  ".cpp": { label: "C++", color: "#5e74c9" },
  ".css": { label: "CSS", color: "#4a90e2" },
  ".go": { label: "GO", color: "#45b8d8" },
  ".html": { label: "HTML", color: "#e66b3d" },
  ".java": { label: "JAVA", color: "#d95d54" },
  ".js": { label: "JS", color: "#e5c441" },
  ".jsx": { label: "JSX", color: "#5fc9e8" },
  ".json": { label: "{}", color: "#d2b84c" },
  ".kt": { label: "KT", color: "#8c6bd1" },
  ".md": { label: "MD", color: "#778195" },
  ".pdf": { label: "PDF", color: "#df5b5b" },
  ".php": { label: "PHP", color: "#777bb3" },
  ".py": { label: "PY", color: "#4d8fbd" },
  ".rb": { label: "RB", color: "#c95151" },
  ".rs": { label: "RS", color: "#c7764d" },
  ".scss": { label: "SASS", color: "#cc6699" },
  ".sh": { label: ">_", color: "#58a36b" },
  ".sql": { label: "SQL", color: "#527fa5" },
  ".swift": { label: "SW", color: "#ef704f" },
  ".ts": { label: "TS", color: "#3178c6" },
  ".tsx": { label: "TSX", color: "#4ba6c8" },
  ".txt": { label: "TXT", color: "#7d8798" },
  ".xml": { label: "XML", color: "#d58945" },
  ".yaml": { label: "YML", color: "#c85a67" },
  ".yml": { label: "YML", color: "#c85a67" },
  ".zip": { label: "ZIP", color: "#a78a55" }
};
var IMAGE_EXTENSIONS = /* @__PURE__ */ new Set([
  ".avif",
  ".bmp",
  ".gif",
  ".heic",
  ".jpeg",
  ".jpg",
  ".png",
  ".svg",
  ".webp"
]);
var ARCHIVE_EXTENSIONS = /* @__PURE__ */ new Set([".7z", ".bz2", ".gz", ".rar", ".tar", ".tgz"]);
var nativeFileIconCache = /* @__PURE__ */ new Map();
function svgDataUrl(svg) {
  return `data:image/svg+xml;base64,${Buffer.from(svg).toString("base64")}`;
}
function documentSvg(label, color) {
  const safeLabel = label.replace(/[&<>"']/g, "");
  return `<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64"><path fill="#f5f6f8" d="M13 5h25l13 13v41H13z"/><path fill="#d9dde5" d="M38 5v14h13z"/><rect x="17" y="36" width="30" height="17" rx="4" fill="${color}"/><text x="32" y="48" text-anchor="middle" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="10" font-weight="800" fill="white">${safeLabel}</text></svg>`;
}
var folderIconDataUrl = svgDataUrl(
  '<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64"><path fill="#62a8ed" d="M5 15a6 6 0 0 1 6-6h15l6 7h21a6 6 0 0 1 6 6v29a6 6 0 0 1-6 6H11a6 6 0 0 1-6-6z"/><path fill="#8bc4f7" d="M5 25h54v26a6 6 0 0 1-6 6H11a6 6 0 0 1-6-6z"/></svg>'
);
function fileIconDataUrl(path7) {
  const extension = (0, import_node_path6.extname)(path7).toLowerCase();
  const style = FILE_ICON_STYLES[extension];
  if (style) return svgDataUrl(documentSvg(style.label, style.color));
  if (IMAGE_EXTENSIONS.has(extension)) return svgDataUrl(documentSvg("IMG", "#8b6fc0"));
  if (ARCHIVE_EXTENSIONS.has(extension)) return svgDataUrl(documentSvg("ZIP", "#a78a55"));
  return svgDataUrl(
    documentSvg(extension ? extension.slice(1, 5).toUpperCase() : "FILE", "#7d8798")
  );
}
function imageFileDataUrl(path7) {
  if (!(0, import_node_fs6.existsSync)(path7)) return void 0;
  const mimeType = (0, import_node_path6.extname)(path7).toLowerCase() === ".svg" ? "image/svg+xml" : (0, import_node_path6.extname)(path7).toLowerCase() === ".jpg" || (0, import_node_path6.extname)(path7).toLowerCase() === ".jpeg" ? "image/jpeg" : (0, import_node_path6.extname)(path7).toLowerCase() === ".webp" ? "image/webp" : "image/png";
  try {
    return `data:${mimeType};base64,${(0, import_node_fs6.readFileSync)(path7).toString("base64")}`;
  } catch {
    return void 0;
  }
}
async function nativeFileIconDataUrl(path7) {
  if (nativeFileIconCache.has(path7)) return nativeFileIconCache.get(path7) ?? void 0;
  if (!(0, import_node_fs6.existsSync)(path7)) return void 0;
  try {
    const stats = (0, import_node_fs6.statSync)(path7);
    const cacheKey2 = (0, import_node_crypto3.createHash)("sha1").update(`${path7}:${stats.mtimeMs}:${stats.size}`).digest("hex");
    const outputDir = (0, import_node_path6.join)(app.getPath("userData"), "icon-cache", "files", cacheKey2);
    const outputPath = (0, import_node_path6.join)(outputDir, `${(0, import_node_path6.basename)(path7)}.png`);
    (0, import_node_fs6.mkdirSync)(outputDir, { recursive: true });
    if (!(0, import_node_fs6.existsSync)(outputPath)) {
      await execFileAsync4("/usr/bin/qlmanage", ["-t", "-i", "-s", "64", "-o", outputDir, path7], {
        timeout: 3e3
      });
    }
    if (!(0, import_node_fs6.existsSync)(outputPath)) {
      nativeFileIconCache.set(path7, null);
      return void 0;
    }
    const dataUrl = `data:image/png;base64,${(0, import_node_fs6.readFileSync)(outputPath).toString("base64")}`;
    nativeFileIconCache.set(path7, dataUrl);
    return dataUrl;
  } catch {
    nativeFileIconCache.set(path7, null);
    return void 0;
  }
}

// src/main/llm/memoryStore.ts
init_electron_shim();
var import_node_fs7 = require("node:fs");
var import_node_path7 = require("node:path");
function memoryPath() {
  const dir = (0, import_node_path7.join)(app.getPath("userData"), "llm");
  (0, import_node_fs7.mkdirSync)(dir, { recursive: true });
  return (0, import_node_path7.join)(dir, "memory.json");
}
function readDb() {
  try {
    const raw = (0, import_node_fs7.readFileSync)(memoryPath(), "utf8");
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed.entries)) return { entries: [] };
    return { entries: parsed.entries };
  } catch {
    return { entries: [] };
  }
}
function writeDb(db) {
  (0, import_node_fs7.writeFileSync)(memoryPath(), `${JSON.stringify(db, null, 2)}
`, "utf8");
}
function tokenize(input) {
  return new Set(
    input.toLowerCase().split(/\s+/).map((token) => token.replace(/[^a-z0-9_-]/g, "")).filter((token) => token.length > 2)
  );
}
function overlapScore(query, text) {
  if (query.size === 0 || text.size === 0) return 0;
  let overlap = 0;
  query.forEach((token) => {
    if (text.has(token)) overlap += 1;
  });
  return overlap / query.size;
}
function redactSensitive(text) {
  return text.replace(/(sk-[A-Za-z0-9]{12,})/g, "[REDACTED_API_KEY]").replace(/(gh[pousr]_[A-Za-z0-9_]{12,})/g, "[REDACTED_TOKEN]").replace(/(password\s*[=:]\s*[^\s]+)/gi, "password=[REDACTED]");
}
function rememberMemory(text, source, isPrivate = false) {
  const cleaned = text.trim();
  if (!cleaned) return;
  const db = readDb();
  db.entries = [
    {
      id: `mem:${Date.now()}:${Math.random().toString(36).slice(2, 8)}`,
      text: redactSensitive(cleaned),
      source,
      createdAt: Date.now(),
      private: isPrivate
    },
    ...db.entries
  ].slice(0, 500);
  writeDb(db);
}
function retrieveMemories(query, policy) {
  if (!policy.enabled || policy.maxItems <= 0) return [];
  const queryTokens = tokenize(query);
  const db = readDb();
  return db.entries.filter((entry) => policy.includePrivate || !entry.private).map((entry) => ({
    text: entry.text,
    score: overlapScore(queryTokens, tokenize(entry.text)),
    createdAt: entry.createdAt
  })).filter((entry) => entry.score > 0).sort((a, b) => {
    if (b.score !== a.score) return b.score - a.score;
    return b.createdAt - a.createdAt;
  }).slice(0, policy.maxItems).map((entry) => entry.text);
}

// src/main/llm/answerStream.ts
init_registry();
var HERMES_ANSWER_SYSTEM = "You are Hermes, a helpful assistant. Answer briefly and clearly unless the user asks for more detail.";
async function streamAnswerToRenderer(sender, userText, signal) {
  const token = (text) => {
    if (!sender.isDestroyed()) sender.send("stream-token", text);
  };
  const done = () => {
    if (!sender.isDestroyed()) sender.send("stream-done");
  };
  const err = (message) => {
    if (!sender.isDestroyed()) sender.send("stream-error", message);
  };
  const cfg = readLLMConfig();
  const provider = getProviderForTask("chat");
  const memories = retrieveMemories(userText, {
    enabled: cfg.memoryEnabled === true,
    maxItems: Math.max(0, cfg.memoryMaxItems ?? 3),
    includePrivate: cfg.memoryIncludePrivate === true
  });
  const messages = [{ role: "system", content: HERMES_ANSWER_SYSTEM }];
  if (memories.length > 0) {
    messages.push({
      role: "system",
      content: `Relevant memory:
${memories.map((entry, i) => `${i + 1}. ${entry}`).join("\n")}`
    });
  }
  messages.push({ role: "user", content: userText });
  let fullText = "";
  try {
    console.log("[streamAnswerToRenderer] using provider:", provider.name);
    const stream = await provider.chat(messages, void 0, { signal });
    console.log("[streamAnswerToRenderer] stream started");
    for await (const delta of stream) {
      if (signal?.aborted) {
        console.log("[streamAnswerToRenderer] signal aborted");
        done();
        return;
      }
      if (delta.text) {
        fullText += delta.text;
        token(delta.text);
      }
    }
    console.log("[streamAnswerToRenderer] stream finished, full text length:", fullText.length);
    if (cfg.memoryEnabled === true) {
      rememberMemory(`User: ${userText}
Assistant: ${fullText.slice(0, 1200)}`, "conversation");
    }
    done();
  } catch (e) {
    if (signal?.aborted) {
      console.log("[streamAnswerToRenderer] aborted in catch");
      done();
      return;
    }
    console.error("[streamAnswerToRenderer] error:", e);
    err(e instanceof Error ? e.message : String(e));
    done();
  }
}

// src/main/windowBounds.ts
var WINDOW_WIDTH = 760;
var WINDOW_MAX_HEIGHT = 640;
var WINDOW_MIN_HEIGHT = 120;
function setLauncherContentHeight(win, rawHeight, rawZoomFactor = 1) {
  const zoomFactor = Number.isFinite(rawZoomFactor) && rawZoomFactor > 0 ? Math.max(1, rawZoomFactor) : 1;
  const maxHeight = Math.round(WINDOW_MAX_HEIGHT * zoomFactor);
  const height = Math.min(
    Math.max(Math.round(rawHeight), WINDOW_MIN_HEIGHT),
    maxHeight
  );
  win.setMaximumSize(WINDOW_WIDTH, maxHeight);
  const [curW, curH] = win.getContentSize();
  if (curW === WINDOW_WIDTH && curH === height) return;
  win.setContentSize(WINDOW_WIDTH, height, false);
}

// src/main/ipc.ts
init_configStore();
init_githubCopilotAuth();

// src/main/llm/listModels.ts
init_aiProviders();
init_copilot();
init_registry();
function trimSlash4(url) {
  return url.replace(/\/+$/, "");
}
function modelsUrl2(baseURL) {
  const base = trimSlash4(baseURL);
  if (base.endsWith("/chat/completions")) {
    return `${base.slice(0, -"/chat/completions".length)}/models`;
  }
  return `${base}/models`;
}
function uniqSorted(ids) {
  return Array.from(new Set(ids.filter(Boolean))).sort((a, b) => a.localeCompare(b));
}
function extractModelIds(json) {
  if (!json || typeof json !== "object") return [];
  const o = json;
  if (Array.isArray(o.data)) {
    const ids = [];
    for (const item of o.data) {
      if (item && typeof item === "object" && "id" in item) {
        const id = item.id;
        if (typeof id === "string" && id) ids.push(id);
      }
    }
    if (ids.length) return uniqSorted(ids);
  }
  if (Array.isArray(o.models)) {
    const ids = [];
    for (const item of o.models) {
      if (!item || typeof item !== "object") continue;
      const m = item;
      const name = typeof m.name === "string" ? m.name : typeof m.model === "string" ? m.model : "";
      if (name) ids.push(name);
    }
    if (ids.length) return uniqSorted(ids);
  }
  return [];
}
var COPILOT_MODELS = "https://api.githubcopilot.com/models";
async function fetchCopilotModelIds(accessToken, signal) {
  if (!accessToken.trim()) return [];
  try {
    const res = await fetch(COPILOT_MODELS, {
      method: "GET",
      headers: {
        Authorization: `Bearer ${accessToken}`,
        "Editor-Version": "Tezbar/0.1.0",
        "Copilot-Integration-Id": "vscode-chat",
        Accept: "application/json"
      },
      signal: signal ?? AbortSignal.timeout(12e3)
    });
    if (!res.ok) return [];
    const json = await res.json();
    return extractModelIds(json);
  } catch {
    return [];
  }
}
async function listModelsForProvider(id, signal) {
  const cfg = configForProvider(readLLMConfig(), id);
  if (isCustomProvider(id)) {
    const base = cfg.openaiCompatibleBaseURL ?? cfg.baseURL ?? "";
    const key = cfg.apiKey ?? "";
    if (!base.trim() || !key.trim()) return [];
    try {
      const res = await fetch(modelsUrl2(base), {
        method: "GET",
        headers: { Authorization: `Bearer ${key}` },
        signal: signal ?? AbortSignal.timeout(12e3)
      });
      if (!res.ok) return [];
      return extractModelIds(await res.json());
    } catch {
      return [];
    }
  }
  switch (id) {
    case "openai": {
      const base = cfg.baseURL ?? "https://api.openai.com/v1";
      const key = cfg.apiKey ?? "";
      if (!key.trim()) return [];
      try {
        const res = await fetch(modelsUrl2(base), {
          method: "GET",
          headers: { Authorization: `Bearer ${key}` },
          signal: signal ?? AbortSignal.timeout(12e3)
        });
        if (!res.ok) return [];
        return extractModelIds(await res.json());
      } catch {
        return [];
      }
    }
    case "openai-compatible": {
      const base = cfg.openaiCompatibleBaseURL ?? cfg.baseURL ?? "https://api.openai.com/v1";
      const key = cfg.apiKey ?? "";
      if (!key.trim()) return [];
      try {
        const res = await fetch(modelsUrl2(base), {
          method: "GET",
          headers: { Authorization: `Bearer ${key}` },
          signal: signal ?? AbortSignal.timeout(12e3)
        });
        if (!res.ok) return [];
        return extractModelIds(await res.json());
      } catch {
        return [];
      }
    }
    case "anthropic": {
      const apiBase = trimSlash4(cfg.baseURL ?? "https://api.anthropic.com");
      const key = cfg.apiKey ?? "";
      if (!key.trim()) return [];
      try {
        const res = await fetch(`${apiBase}/v1/models`, {
          method: "GET",
          headers: {
            "x-api-key": key,
            "anthropic-version": "2023-06-01"
          },
          signal: signal ?? AbortSignal.timeout(12e3)
        });
        if (!res.ok) return [];
        return extractModelIds(await res.json());
      } catch {
        return [];
      }
    }
    case "ollama": {
      const base = cfg.baseURL ?? "http://localhost:11434";
      try {
        const res = await fetch(`${trimSlash4(base)}/api/tags`, {
          method: "GET",
          signal: signal ?? AbortSignal.timeout(12e3)
        });
        if (!res.ok) return [];
        return extractModelIds(await res.json());
      } catch {
        return [];
      }
    }
    case "gemini": {
      const base = cfg.baseURL ?? "https://generativelanguage.googleapis.com/v1beta/openai";
      const key = cfg.geminiApiKey ?? cfg.apiKey ?? "";
      if (!key.trim()) return [];
      try {
        const res = await fetch(modelsUrl2(base), {
          method: "GET",
          headers: { Authorization: `Bearer ${key}` },
          signal: signal ?? AbortSignal.timeout(12e3)
        });
        if (!res.ok) {
          return ["gemini-2.0-flash", "gemini-2.5-flash", "gemini-2.5-pro"];
        }
        const ids = extractModelIds(await res.json());
        return ids.length > 0 ? ids : ["gemini-2.0-flash", "gemini-2.5-flash", "gemini-2.5-pro"];
      } catch {
        return ["gemini-2.0-flash", "gemini-2.5-flash", "gemini-2.5-pro"];
      }
    }
    case "copilot": {
      const cp = new CopilotProvider(cfg.model ?? "gpt-4o");
      const token = await cp.getAccessToken({ signal });
      return fetchCopilotModelIds(token, signal);
    }
    case "opencode": {
      try {
        const { execFile: execFile13 } = await import("node:child_process");
        const { promisify: promisify14 } = await import("node:util");
        const execFileAsync13 = promisify14(execFile13);
        const { stdout } = await execFileAsync13("opencode", ["models"], { timeout: 12e3, signal });
        const models = stdout.replace(/\x1b\[[0-9;]*m/g, "").split("\n").map((line) => line.trim()).filter((line) => line.startsWith("opencode/") || line.startsWith("opencode-go/"));
        return models.length > 0 ? models : ["opencode/big-pickle"];
      } catch {
        return ["opencode/big-pickle"];
      }
    }
    case "deepseek": {
      const base = cfg.baseURL ?? "https://api.deepseek.com";
      const key = cfg.apiKey ?? "";
      if (!key.trim()) return ["deepseek-v4-flash", "deepseek-v4-pro", "deepseek-reasoner"];
      try {
        const res = await fetch(modelsUrl2(base), {
          method: "GET",
          headers: { Authorization: `Bearer ${key}` },
          signal: signal ?? AbortSignal.timeout(12e3)
        });
        if (!res.ok) return ["deepseek-v4-flash", "deepseek-v4-pro", "deepseek-reasoner"];
        const ids = extractModelIds(await res.json());
        return ids.length > 0 ? ids : ["deepseek-v4-flash", "deepseek-v4-pro", "deepseek-reasoner"];
      } catch {
        return ["deepseek-v4-flash", "deepseek-v4-pro", "deepseek-reasoner"];
      }
    }
    default:
      return [];
  }
}

// src/main/ipc.ts
init_registry();

// src/main/router.ts
var QUESTION_PREFIX_RE = /^(what|why|how|who|when|is|are|can|does)\b/i;
var FILE_HINT_RE = /\b(file|folder|path|directory|finder|desktop|documents|downloads)\b/i;
var APP_HINT_RE = /\b(open|launch|start)\s+/i;
function classifyNaturalLanguageSystem(lower) {
  if (/\b(disable|turn off|switch off)\b.*\b(wi-?fi|wifi)\b/.test(lower)) {
    return { type: "system", action: "wifi-off" };
  }
  if (/\b(turn|switch|set|enable)\b.*\b(wi-?fi|wifi)\b/.test(lower)) {
    return { type: "system", action: "wifi-on" };
  }
  if (/\b(enable|turn on|set)\b.*\b(dark mode|dark)\b/.test(lower)) {
    return { type: "system", action: "dark-mode-on" };
  }
  if (/\b(disable|turn off)\b.*\b(dark mode|dark)\b/.test(lower)) {
    return { type: "system", action: "dark-mode-off" };
  }
  if (/\b(mute|silence)\b/.test(lower)) {
    return { type: "system", action: "mute-on" };
  }
  if (/\b(unmute|sound on)\b/.test(lower)) {
    return { type: "system", action: "mute-off" };
  }
  return null;
}
function parseExtension(input) {
  const body = input.slice(1).trim();
  if (!body) {
    return { type: "extension", name: "", args: "" };
  }
  const space = body.search(/\s/);
  if (space === -1) {
    return { type: "extension", name: body, args: "" };
  }
  return {
    type: "extension",
    name: body.slice(0, space),
    args: body.slice(space + 1).trim()
  };
}
function classifySystem(trimmed, lower) {
  const nl = classifyNaturalLanguageSystem(lower);
  if (nl) return nl;
  if (lower === "quit") {
    return { type: "system", action: "quit" };
  }
  if (lower === "calculator") {
    return { type: "system", action: "calculator" };
  }
  if (lower.startsWith("open ")) {
    let target = trimmed.slice(5).trim();
    if (target.endsWith("?")) {
      target = target.slice(0, -1).trim();
    }
    return { type: "application", target };
  }
  return null;
}
function looksLikeAnswer(trimmed) {
  if (trimmed.length >= 120) return false;
  if (trimmed.endsWith("?")) return true;
  return QUESTION_PREFIX_RE.test(trimmed);
}
async function classifyIntent(raw) {
  const input = raw.trim();
  if (input.startsWith("/")) {
    return parseExtension(input);
  }
  const lower = input.toLowerCase();
  const system = classifySystem(input, lower);
  if (system) {
    return system;
  }
  if (FILE_HINT_RE.test(input)) {
    return { type: "file", query: input };
  }
  if (APP_HINT_RE.test(input)) {
    const target = input.replace(/^\s*(open|launch|start)\s+/i, "").trim();
    if (target) {
      return { type: "application", target };
    }
  }
  if (looksLikeAnswer(input)) {
    return { type: "ai", input };
  }
  if (/\b(explain|summarize|write|draft|generate)\b/i.test(input)) {
    return { type: "ai", input };
  }
  if (/\b(run|execute|toggle|enable|disable|kill|stop|start|turn)\b/i.test(input)) {
    return { type: "command", command: input, confidence: 0.62 };
  }
  return { type: "agent", input };
}

// src/main/extensions/service.ts
init_electron_shim();
var import_node_crypto4 = require("node:crypto");
var import_node_module = require("node:module");
var import_node_path10 = require("node:path");
var import_node_fs10 = require("node:fs");
var import_node_os6 = require("node:os");

// node_modules/.pnpm/fuse.js@7.3.0/node_modules/fuse.js/dist/fuse.mjs
function isArray2(value) {
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
var INCORRECT_INDEX_TYPE = "Incorrect 'index' type";
var LOGICAL_SEARCH_INVALID_QUERY_FOR_KEY = (key) => `Invalid value for key ${key}`;
var PATTERN_LENGTH_TOO_LARGE = (max) => `Pattern length exceeds max of ${max}.`;
var MISSING_KEY_PROPERTY = (name) => `Missing ${name} property in key`;
var INVALID_KEY_WEIGHT_VALUE = (key) => `Property 'weight' in key '${key}' must be a positive integer`;
var hasOwn2 = Object.prototype.hasOwnProperty;
var KeyStore = class {
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
function createKey(key) {
  let path7 = null;
  let id = null;
  let src = null;
  let weight = 1;
  let getFn = null;
  if (isString(key) || isArray2(key)) {
    src = key;
    path7 = createKeyPath(key);
    id = createKeyId(key);
  } else {
    if (!hasOwn2.call(key, "name")) {
      throw new Error(MISSING_KEY_PROPERTY("name"));
    }
    const name = key.name;
    src = name;
    if (hasOwn2.call(key, "weight")) {
      weight = key.weight;
      if (weight <= 0) {
        throw new Error(INVALID_KEY_WEIGHT_VALUE(name));
      }
    }
    path7 = createKeyPath(name);
    id = createKeyId(name);
    getFn = key.getFn;
  }
  return {
    path: path7,
    id,
    weight,
    src,
    getFn
  };
}
function createKeyPath(key) {
  return isArray2(key) ? key : key.split(".");
}
function createKeyId(key) {
  return isArray2(key) ? key.join(".") : key;
}
function get(obj, path7) {
  const list = [];
  let arr = false;
  const deepGet = (obj2, path8, index, arrayIndex) => {
    if (!isDefined(obj2)) {
      return;
    }
    if (!path8[index]) {
      list.push(arrayIndex !== void 0 ? {
        v: obj2,
        i: arrayIndex
      } : obj2);
    } else {
      const key = path8[index];
      const value = obj2[key];
      if (!isDefined(value)) {
        return;
      }
      if (index === path8.length - 1 && (isString(value) || isNumber(value) || isBoolean(value) || typeof value === "bigint")) {
        list.push(arrayIndex !== void 0 ? {
          v: toString(value),
          i: arrayIndex
        } : toString(value));
      } else if (isArray2(value)) {
        arr = true;
        for (let i = 0, len = value.length; i < len; i += 1) {
          deepGet(value[i], path8, index + 1, i);
        }
      } else if (path8.length) {
        deepGet(value, path8, index + 1, arrayIndex);
      }
    }
  };
  deepGet(obj, isString(path7) ? path7.split(".") : path7, 0);
  return arr ? list : list[0];
}
var MatchOptions = {
  includeMatches: false,
  findAllMatches: false,
  minMatchCharLength: 1
};
var BasicOptions = {
  isCaseSensitive: false,
  ignoreDiacritics: false,
  includeScore: false,
  keys: [],
  shouldSort: true,
  sortFn: (a, b) => a.score === b.score ? a.idx < b.idx ? -1 : 1 : a.score < b.score ? -1 : 1
};
var FuzzyOptions = {
  location: 0,
  threshold: 0.6,
  distance: 100
};
var AdvancedOptions = {
  useExtendedSearch: false,
  useTokenSearch: false,
  getFn: get,
  ignoreLocation: false,
  ignoreFieldNorm: false,
  fieldNormWeight: 1
};
var Config = Object.freeze({
  ...BasicOptions,
  ...MatchOptions,
  ...FuzzyOptions,
  ...AdvancedOptions
});
var SPACE = /[^ ]+/g;
function norm(weight = 1, mantissa = 3) {
  const cache2 = /* @__PURE__ */ new Map();
  const m = Math.pow(10, mantissa);
  return {
    get(value) {
      const numTokens = value.match(SPACE).length;
      if (cache2.has(numTokens)) {
        return cache2.get(numTokens);
      }
      const norm2 = 1 / Math.pow(numTokens, 0.5 * weight);
      const n = parseFloat(Math.round(norm2 * m) / m);
      cache2.set(numTokens, n);
      return n;
    },
    clear() {
      cache2.clear();
    }
  };
}
var FuseIndex = class {
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
      if (isArray2(value)) {
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
var MAX_BITS = 32;
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
var NON_DECOMPOSABLE_MAP = {
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
var NON_DECOMPOSABLE_RE = new RegExp("[" + Object.keys(NON_DECOMPOSABLE_MAP).join("") + "]", "g");
var stripDiacritics = String.prototype.normalize ? (str2) => str2.normalize("NFD").replace(/[\u0300-\u036F\u0483-\u0489\u0591-\u05BD\u05BF\u05C1\u05C2\u05C4\u05C5\u05C7\u0610-\u061A\u064B-\u065F\u0670\u06D6-\u06DC\u06DF-\u06E4\u06E7\u06E8\u06EA-\u06ED\u0711\u0730-\u074A\u07A6-\u07B0\u07EB-\u07F3\u07FD\u0816-\u0819\u081B-\u0823\u0825-\u0827\u0829-\u082D\u0859-\u085B\u08D3-\u08E1\u08E3-\u0903\u093A-\u093C\u093E-\u094F\u0951-\u0957\u0962\u0963\u0981-\u0983\u09BC\u09BE-\u09C4\u09C7\u09C8\u09CB-\u09CD\u09D7\u09E2\u09E3\u09FE\u0A01-\u0A03\u0A3C\u0A3E-\u0A42\u0A47\u0A48\u0A4B-\u0A4D\u0A51\u0A70\u0A71\u0A75\u0A81-\u0A83\u0ABC\u0ABE-\u0AC5\u0AC7-\u0AC9\u0ACB-\u0ACD\u0AE2\u0AE3\u0AFA-\u0AFF\u0B01-\u0B03\u0B3C\u0B3E-\u0B44\u0B47\u0B48\u0B4B-\u0B4D\u0B56\u0B57\u0B62\u0B63\u0B82\u0BBE-\u0BC2\u0BC6-\u0BC8\u0BCA-\u0BCD\u0BD7\u0C00-\u0C04\u0C3E-\u0C44\u0C46-\u0C48\u0C4A-\u0C4D\u0C55\u0C56\u0C62\u0C63\u0C81-\u0C83\u0CBC\u0CBE-\u0CC4\u0CC6-\u0CC8\u0CCA-\u0CCD\u0CD5\u0CD6\u0CE2\u0CE3\u0D00-\u0D03\u0D3B\u0D3C\u0D3E-\u0D44\u0D46-\u0D48\u0D4A-\u0D4D\u0D57\u0D62\u0D63\u0D82\u0D83\u0DCA\u0DCF-\u0DD4\u0DD6\u0DD8-\u0DDF\u0DF2\u0DF3\u0E31\u0E34-\u0E3A\u0E47-\u0E4E\u0EB1\u0EB4-\u0EB9\u0EBB\u0EBC\u0EC8-\u0ECD\u0F18\u0F19\u0F35\u0F37\u0F39\u0F3E\u0F3F\u0F71-\u0F84\u0F86\u0F87\u0F8D-\u0F97\u0F99-\u0FBC\u0FC6\u102B-\u103E\u1056-\u1059\u105E-\u1060\u1062-\u1064\u1067-\u106D\u1071-\u1074\u1082-\u108D\u108F\u109A-\u109D\u135D-\u135F\u1712-\u1714\u1732-\u1734\u1752\u1753\u1772\u1773\u17B4-\u17D3\u17DD\u180B-\u180D\u1885\u1886\u18A9\u1920-\u192B\u1930-\u193B\u1A17-\u1A1B\u1A55-\u1A5E\u1A60-\u1A7C\u1A7F\u1AB0-\u1ABE\u1B00-\u1B04\u1B34-\u1B44\u1B6B-\u1B73\u1B80-\u1B82\u1BA1-\u1BAD\u1BE6-\u1BF3\u1C24-\u1C37\u1CD0-\u1CD2\u1CD4-\u1CE8\u1CED\u1CF2-\u1CF4\u1CF7-\u1CF9\u1DC0-\u1DF9\u1DFB-\u1DFF\u20D0-\u20F0\u2CEF-\u2CF1\u2D7F\u2DE0-\u2DFF\u302A-\u302F\u3099\u309A\uA66F-\uA672\uA674-\uA67D\uA69E\uA69F\uA6F0\uA6F1\uA802\uA806\uA80B\uA823-\uA827\uA880\uA881\uA8B4-\uA8C5\uA8E0-\uA8F1\uA8FF\uA926-\uA92D\uA947-\uA953\uA980-\uA983\uA9B3-\uA9C0\uA9E5\uAA29-\uAA36\uAA43\uAA4C\uAA4D\uAA7B-\uAA7D\uAAB0\uAAB2-\uAAB4\uAAB7\uAAB8\uAABE\uAABF\uAAC1\uAAEB-\uAAEF\uAAF5\uAAF6\uABE3-\uABEA\uABEC\uABED\uFB1E\uFE00-\uFE0F\uFE20-\uFE2F]/g, "").replace(NON_DECOMPOSABLE_RE, (ch) => NON_DECOMPOSABLE_MAP[ch]) : (str2) => str2;
var BitapSearch = class {
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
var BaseMatch = class {
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
function getMatch(pattern, exp) {
  const matches = pattern.match(exp);
  return matches ? matches[1] : null;
}
var ExactMatch = class extends BaseMatch {
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
var InverseExactMatch = class extends BaseMatch {
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
var PrefixExactMatch = class extends BaseMatch {
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
var InversePrefixExactMatch = class extends BaseMatch {
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
var SuffixExactMatch = class extends BaseMatch {
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
var InverseSuffixExactMatch = class extends BaseMatch {
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
var FuzzyMatch = class extends BaseMatch {
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
var IncludeMatch = class extends BaseMatch {
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
var searchers = [ExactMatch, IncludeMatch, PrefixExactMatch, InversePrefixExactMatch, InverseSuffixExactMatch, SuffixExactMatch, InverseExactMatch, FuzzyMatch];
var searchersLen = searchers.length;
var ESCAPED_PIPE = "\0";
var OR_TOKEN = "|";
function tokenize3(pattern) {
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
    const query = tokenize3(restored.trim()).filter((item2) => item2 && !!item2.trim());
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
var MultiMatchSet = /* @__PURE__ */ new Set([FuzzyMatch.type, IncludeMatch.type]);
var ExtendedSearch = class {
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
var registeredSearchers = [];
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
var LogicalOperator = {
  AND: "$and",
  OR: "$or"
};
var KeyType = {
  PATH: "$path",
  PATTERN: "$val"
};
var isExpression = (query) => !!(query[LogicalOperator.AND] || query[LogicalOperator.OR]);
var isPath = (query) => !!query[KeyType.PATH];
var isLeaf = (query) => !isArray2(query) && isObject(query) && !isExpression(query);
var convertToExplicit = (query) => ({
  [LogicalOperator.AND]: Object.keys(query).map((key) => ({
    [key]: query[key]
  }))
});
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
      if (isArray2(value)) {
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
var MaxHeap = class {
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
var WORD = /\b\w+\b/g;
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
var Fuse = class {
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
    if (isArray2(value)) {
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
var TokenSearch = class {
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

// src/main/extensions/raycastShim.ts
init_electron_shim();
var import_node_child_process7 = require("node:child_process");
var import_node_fs9 = require("node:fs");
var import_node_os5 = require("node:os");
var import_node_path9 = require("node:path");
var import_node_util6 = require("node:util");
var TOAST_STYLE = {
  Success: "success",
  Failure: "failure",
  Animated: "animated"
};
var execFileAsync5 = (0, import_node_util6.promisify)(import_node_child_process7.execFile);
async function runAppleScript(source) {
  if (process.platform !== "darwin") {
    throw new Error("AppleScript is only available on macOS");
  }
  if (typeof source !== "string" || source.trim().length === 0) {
    return "";
  }
  const { stdout } = await execFileAsync5("/usr/bin/osascript", ["-e", source], {
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
    get(_t, prop) {
      if (prop === Symbol.toPrimitive) return () => `[Raycast:${name}]`;
      if (prop === "displayName") return name;
      if (prop === "prototype") return {};
      if (typeof prop === "symbol") return void 0;
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
  const file = (0, import_node_path9.join)(packageRoot, "localStorage.json");
  const readAll2 = () => {
    try {
      const raw = (0, import_node_fs9.readFileSync)(file, "utf8");
      const parsed = JSON.parse(raw);
      return parsed && typeof parsed === "object" ? parsed : {};
    } catch {
      return {};
    }
  };
  const writeAll2 = (value) => {
    (0, import_node_fs9.mkdirSync)(packageRoot, { recursive: true });
    (0, import_node_fs9.writeFileSync)(file, JSON.stringify(value, null, 2), "utf8");
  };
  return {
    getItem: async (key) => readAll2()[key],
    setItem: async (key, value) => {
      const all = readAll2();
      all[key] = value;
      writeAll2(all);
    },
    removeItem: async (key) => {
      const all = readAll2();
      delete all[key];
      writeAll2(all);
    },
    clear: async () => {
      writeAll2({});
    },
    allItems: async () => readAll2()
  };
}
function readPreferences(packageRoot) {
  const file = (0, import_node_path9.join)(packageRoot, "preferences.json");
  try {
    const raw = (0, import_node_fs9.readFileSync)(file, "utf8");
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}
function createClipboardShim() {
  return {
    copy: async (value) => {
      if (value && typeof value === "object" && "text" in value) {
        const v = value.text;
        if (typeof v === "string") {
          clipboard.writeText(v);
          return;
        }
      }
      if (typeof value === "string") clipboard.writeText(value);
      else clipboard.writeText(String(value));
    },
    paste: async (value) => {
      if (typeof value === "string") clipboard.writeText(value);
      else if (value && typeof value === "object" && "text" in value) {
        const v = value.text;
        if (typeof v === "string") clipboard.writeText(v);
      }
    },
    readText: async () => clipboard.readText(),
    read: async () => ({
      text: clipboard.readText() || void 0
    }),
    clear: async () => clipboard.clear()
  };
}
function createEnvironment(ctx) {
  const supportPath = (0, import_node_path9.join)(ctx.packageRoot, "support");
  try {
    (0, import_node_fs9.mkdirSync)(supportPath, { recursive: true });
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
    assetsPath: (0, import_node_path9.join)(ctx.packageRoot, "assets"),
    launchType: "userInitiated",
    textSize: "medium"
  };
}
function createRaycastApi(ctx) {
  return {
    Toast: { Style: TOAST_STYLE },
    Icon: createRenderProxy("Icon"),
    Color: createRenderProxy("Color"),
    Image: createRenderProxy("Image"),
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
    getSelectedText: async () => "",
    getApplications: async () => [],
    runAppleScript,
    open: async (target) => {
      if (typeof target !== "string") return;
      if (/^[a-z][a-z0-9+.-]*:\/\//i.test(target) || target.startsWith("mailto:")) {
        await shell.openExternal(target);
      } else {
        const resolved = target.startsWith("~") ? target.replace(/^~/, (0, import_node_os5.homedir)()) : target;
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
    showInFinder: async (path7) => {
      if (typeof path7 !== "string") return;
      shell.showItemInFolder(path7);
    },
    confirmAlert: async () => true,
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
    runAppleScript,
    showFailureToast: (error) => {
      ctx.feedback.push({
        kind: "toast",
        style: "failure",
        title: error instanceof Error ? error.message : String(error)
      });
    },
    getFavicon: () => createRenderProxy("Icon")
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

// src/main/extensions/service.ts
var RAYCAST_EXTENSIONS_REPO = "https://github.com/raycast/extensions";
var RAYCAST_EXTENSIONS_REF = "c0e624ee0420679ed3aa296c25c1a6f29938c56a";
var RAYCAST_EXTENSIONS_PATH = "extensions";
var CATALOG_CACHE_TTL_MS = 10 * 6e4;
var RUNTIME_UNSUPPORTED_MODE = "RUNTIME_UNSUPPORTED_MODE";
var DEFAULT_DB = {
  installed: []
};
var catalogCache = null;
var commandCache = /* @__PURE__ */ new Map();
function getDbPath() {
  const dir = (0, import_node_path10.join)(app.getPath("userData"), "extensions");
  (0, import_node_fs10.mkdirSync)(dir, { recursive: true });
  return (0, import_node_path10.join)(dir, "installed.json");
}
function extensionsRootDir() {
  const dir = (0, import_node_path10.join)(app.getPath("userData"), "extensions");
  (0, import_node_fs10.mkdirSync)(dir, { recursive: true });
  return dir;
}
function installedPackageRoot(extensionId) {
  return (0, import_node_path10.join)(extensionsRootDir(), "packages", extensionId);
}
function packageJsonPathForInstalledExtension(extensionId) {
  return (0, import_node_path10.join)(installedPackageRoot(extensionId), "package.json");
}
function scriptPathForInstalledExtensionCommand(extensionId, commandName2) {
  return (0, import_node_path10.join)(installedPackageRoot(extensionId), ".sc-build", `${commandName2}.js`);
}
function metaPathForInstalledExtension(extensionId) {
  return (0, import_node_path10.join)(installedPackageRoot(extensionId), "meta.json");
}
function backupPackageRoot(extensionId) {
  return (0, import_node_path10.join)(extensionsRootDir(), "packages", `${extensionId}.backup`);
}
function readInstallMeta(extensionId) {
  const p = metaPathForInstalledExtension(extensionId);
  if (!(0, import_node_fs10.existsSync)(p)) return null;
  try {
    return JSON.parse((0, import_node_fs10.readFileSync)(p, "utf8"));
  } catch {
    return null;
  }
}
function writeInstallMeta(meta) {
  const p = metaPathForInstalledExtension(meta.extensionId);
  (0, import_node_fs10.mkdirSync)((0, import_node_path10.dirname)(p), { recursive: true });
  (0, import_node_fs10.writeFileSync)(p, JSON.stringify(meta, null, 2), "utf8");
}
function hashText(text) {
  return (0, import_node_crypto4.createHash)("sha256").update(text).digest("hex");
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
    if (!(0, import_node_fs10.existsSync)(scriptPath)) {
      missing.push(name);
      continue;
    }
    const expected = meta.scriptHashes[name];
    if (!expected) continue;
    try {
      const actual = hashText((0, import_node_fs10.readFileSync)(scriptPath, "utf8"));
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
  if (!(0, import_node_fs10.existsSync)(p)) return null;
  try {
    const raw = (0, import_node_fs10.readFileSync)(p, "utf8");
    return parseJsonSafe(raw);
  } catch {
    return null;
  }
}
function extensionSlugFromId(extensionId) {
  return extensionId.startsWith("raycast.") ? extensionId.slice("raycast.".length) : extensionId;
}
function readDb2() {
  const p = getDbPath();
  try {
    const raw = (0, import_node_fs10.readFileSync)(p, "utf8");
    const parsed = JSON.parse(raw);
    return {
      installed: Array.isArray(parsed.installed) ? parsed.installed : []
    };
  } catch {
    return DEFAULT_DB;
  }
}
function writeDb2(db) {
  const p = getDbPath();
  (0, import_node_fs10.writeFileSync)(p, JSON.stringify(db, null, 2), "utf8");
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
var AWESOME_RAYCAST_DATA = "https://raw.githubusercontent.com/j3lte/awesome-raycast/main/data/data.json";
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
  const staging = (0, import_node_fs10.mkdtempSync)((0, import_node_path10.join)((0, import_node_os6.tmpdir)(), `tezbar-ext-${extensionId}-`));
  const stagingBuild = (0, import_node_path10.join)(staging, ".sc-build");
  (0, import_node_fs10.mkdirSync)(stagingBuild, { recursive: true });
  (0, import_node_fs10.writeFileSync)((0, import_node_path10.join)(staging, "package.json"), JSON.stringify(pkg, null, 2), "utf8");
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
        (0, import_node_fs10.writeFileSync)((0, import_node_path10.join)(stagingBuild, `${entry.name}.js`), js, "utf8");
        scriptHashes[entry.name] = hashText(js);
      } catch {
        missingScripts.push(entry.name);
      }
    })
  );
  const root = installedPackageRoot(extensionId);
  const backup = backupPackageRoot(extensionId);
  if ((0, import_node_fs10.existsSync)(backup)) (0, import_node_fs10.rmSync)(backup, { recursive: true, force: true });
  try {
    if ((0, import_node_fs10.existsSync)(root)) (0, import_node_fs10.renameSync)(root, backup);
    (0, import_node_fs10.mkdirSync)((0, import_node_path10.dirname)(root), { recursive: true });
    (0, import_node_fs10.renameSync)(staging, root);
  } catch (error) {
    if ((0, import_node_fs10.existsSync)(backup) && !(0, import_node_fs10.existsSync)(root)) {
      try {
        (0, import_node_fs10.renameSync)(backup, root);
      } catch {
      }
    }
    (0, import_node_fs10.rmSync)(staging, { recursive: true, force: true });
    throw error;
  } finally {
    if ((0, import_node_fs10.existsSync)(backup)) (0, import_node_fs10.rmSync)(backup, { recursive: true, force: true });
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
var installErrors = /* @__PURE__ */ new Map();
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
async function executeNoViewScript(extensionId, commandName2, scriptPath, argumentValues) {
  const fileRequire = (0, import_node_module.createRequire)(scriptPath);
  const feedback = [];
  const packageRoot = installedPackageRoot(extensionId);
  const shimCtx = { extensionId, commandName: commandName2, packageRoot, feedback };
  const raycastApiShim = createRaycastApi(shimCtx);
  const raycastUtilsShim = createRaycastUtils(shimCtx);
  const builtinSet = new Set(import_node_module.builtinModules);
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
  const wrapper = new Function(
    "exports",
    "require",
    "module",
    "__filename",
    "__dirname",
    (0, import_node_fs10.readFileSync)(scriptPath, "utf8")
  );
  wrapper(mod.exports, customRequire, mod, scriptPath, (0, import_node_path10.dirname)(scriptPath));
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
async function executeExtensionCommandRuntime(extensionId, commandName2, argumentValues) {
  const pkg = await ensureExtensionBundle(extensionId);
  if (!pkg) {
    throw new Error(`Runtime bundle not available for extension: ${extensionId}`);
  }
  const commandMeta = (pkg.commands ?? []).find((command) => command.name === commandName2);
  if (!commandMeta) {
    throw new Error(`Command not found: ${commandName2}`);
  }
  const mode = (commandMeta.mode ?? "").toLowerCase();
  if (mode && mode !== "no-view") {
    throw unsupportedModeError();
  }
  const meta = readInstallMeta(extensionId);
  if (meta?.missingScripts.includes(commandName2)) {
    throw new Error(
      `No prebuilt script for ${commandName2}. This extension doesn't ship an executable .sc-build file for this command.`
    );
  }
  const scriptPath = scriptPathForInstalledExtensionCommand(extensionId, commandName2);
  if (!(0, import_node_fs10.existsSync)(scriptPath)) {
    throw new Error(`Missing command script: ${commandName2}.js`);
  }
  return await executeNoViewScript(extensionId, commandName2, scriptPath, argumentValues);
}
async function getStoreCatalog() {
  const now = Date.now();
  if (catalogCache && now - catalogCache.fetchedAt < CATALOG_CACHE_TTL_MS) {
    return catalogCache.catalog;
  }
  try {
    const catalog = await fetchRaycastCatalogFromAwesome();
    catalogCache = { fetchedAt: now, catalog };
    return catalog;
  } catch (error) {
    console.warn("[extensions] failed to refresh Raycast catalog from Awesome:", error);
    try {
      const catalog = await fetchRaycastCatalogFromGithub();
      if (catalog.length > 0) {
        catalogCache = { fetchedAt: now, catalog };
        return catalog;
      }
    } catch (innerError) {
      console.warn("[extensions] failed to refresh Raycast catalog from Github:", innerError);
    }
    return catalogCache?.catalog ?? [];
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
  const db = readDb2();
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
async function installExtension(extensionId) {
  const catalog = await getStoreCatalog();
  const manifest = catalog.find((item) => item.id === extensionId);
  if (!manifest) {
    throw new Error(`Extension not found in store: ${extensionId}`);
  }
  const db = readDb2();
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
  writeDb2(db);
  void ensureExtensionBundle(extensionId);
  return next;
}
function uninstallExtension(extensionId) {
  const db = readDb2();
  const before = db.installed.length;
  db.installed = db.installed.filter((item) => item.id !== extensionId);
  if (db.installed.length === before) return false;
  writeDb2(db);
  commandCache.delete(extensionId);
  installErrors.delete(extensionId);
  (0, import_node_fs10.rmSync)(installedPackageRoot(extensionId), { recursive: true, force: true });
  (0, import_node_fs10.rmSync)(backupPackageRoot(extensionId), { recursive: true, force: true });
  return true;
}

// src/main/ipc.ts
init_extension_registry();
init_extension_runner();

// src/main/search/service.ts
init_electron_shim();
var import_node_child_process12 = require("node:child_process");
var import_node_fs21 = require("node:fs");
var import_node_os11 = require("node:os");
var import_node_path22 = require("node:path");
var import_node_util11 = require("node:util");

// src/main/nativeCommands/executor.ts
var import_node_child_process9 = require("node:child_process");
var import_node_util8 = require("node:util");

// src/main/nativeCommands/registry.ts
var DESCRIPTORS = {
  "toggle-dark-mode": {
    id: "toggle-dark-mode",
    title: "Toggle Dark Mode",
    subtitle: "Switch between light and dark appearance.",
    category: "display",
    strategy: "applescript",
    keywords: ["dark", "light", "appearance", "theme", "mode"],
    macOnly: true
  },
  "start-screen-saver": {
    id: "start-screen-saver",
    title: "Start Screen Saver",
    subtitle: "Launch the screen saver now.",
    category: "display",
    strategy: "shell",
    keywords: ["screensaver", "screen", "saver", "lock"],
    macOnly: true
  },
  "sleep-display": {
    id: "sleep-display",
    title: "Sleep Display",
    subtitle: "Put just the display to sleep.",
    category: "power",
    strategy: "shell",
    keywords: ["sleep", "display", "screen", "off"],
    macOnly: true
  },
  "toggle-mute": {
    id: "toggle-mute",
    title: "Toggle Mute",
    subtitle: "Mute or unmute the system output volume.",
    category: "audio",
    strategy: "applescript",
    keywords: ["mute", "unmute", "sound", "audio", "volume"],
    macOnly: true
  },
  "volume-up": {
    id: "volume-up",
    title: "Volume Up",
    subtitle: "Raise system output volume by one step.",
    category: "audio",
    strategy: "applescript",
    keywords: ["volume", "louder", "up"],
    macOnly: true
  },
  "volume-down": {
    id: "volume-down",
    title: "Volume Down",
    subtitle: "Lower system output volume by one step.",
    category: "audio",
    strategy: "applescript",
    keywords: ["volume", "quieter", "down"],
    macOnly: true
  },
  "toggle-hide-desktop-icons": {
    id: "toggle-hide-desktop-icons",
    title: "Toggle Hide Desktop Icons",
    subtitle: "Hide or show files on the Finder desktop.",
    category: "desktop",
    strategy: "shell",
    keywords: ["hide", "desktop", "icons", "clean", "finder"],
    macOnly: true
  },
  "toggle-autohide-dock": {
    id: "toggle-autohide-dock",
    title: "Toggle Autohide Dock",
    subtitle: "Flip the Dock auto-hide preference.",
    category: "desktop",
    strategy: "shell",
    keywords: ["dock", "autohide", "hide", "bar"],
    macOnly: true
  },
  "toggle-autohide-menu-bar": {
    id: "toggle-autohide-menu-bar",
    title: "Toggle Autohide Menu Bar",
    subtitle: "Flip the macOS menu-bar auto-hide preference.",
    category: "desktop",
    strategy: "shell",
    keywords: ["menu", "bar", "autohide", "notch"],
    macOnly: true
  },
  "restart-dock": {
    id: "restart-dock",
    title: "Restart Dock",
    subtitle: "Relaunch the Dock process.",
    category: "desktop",
    strategy: "shell",
    keywords: ["dock", "restart", "relaunch"],
    macOnly: true
  },
  "restart-finder": {
    id: "restart-finder",
    title: "Restart Finder",
    subtitle: "Relaunch the Finder process.",
    category: "desktop",
    strategy: "shell",
    keywords: ["finder", "restart", "relaunch"],
    macOnly: true
  },
  "restart-menu-bar": {
    id: "restart-menu-bar",
    title: "Restart Menu Bar",
    subtitle: "Relaunch SystemUIServer (fixes frozen menu bar).",
    category: "desktop",
    strategy: "shell",
    keywords: ["menu", "bar", "restart", "systemuiserver"],
    macOnly: true
  },
  "start-keep-awake": {
    id: "start-keep-awake",
    title: "Keep Awake",
    subtitle: "Prevent system sleep until you stop it.",
    category: "power",
    strategy: "shell",
    keywords: ["keep", "awake", "caffeinate", "no", "sleep"],
    restoreId: "stop-keep-awake",
    macOnly: true
  },
  "stop-keep-awake": {
    id: "stop-keep-awake",
    title: "Stop Keep Awake",
    subtitle: "Allow the system to sleep again.",
    category: "power",
    strategy: "shell",
    keywords: ["stop", "awake", "caffeinate", "sleep"],
    macOnly: true
  },
  "sleep-system": {
    id: "sleep-system",
    title: "Sleep Mac",
    subtitle: "Put the Mac to sleep now.",
    category: "power",
    strategy: "applescript",
    keywords: ["sleep", "mac", "suspend", "idle"],
    macOnly: true
  },
  "toggle-bluetooth": {
    id: "toggle-bluetooth",
    title: "Toggle Bluetooth",
    subtitle: "Turn Bluetooth on or off (requires blueutil).",
    category: "network",
    strategy: "shell",
    keywords: ["bluetooth", "bt", "airpods", "wireless"],
    macOnly: true
  },
  "toggle-wifi": {
    id: "toggle-wifi",
    title: "Toggle Wi-Fi",
    subtitle: "Turn Wi-Fi on or off on the default interface.",
    category: "network",
    strategy: "shell",
    keywords: ["wifi", "wireless", "network", "toggle"],
    macOnly: true
  },
  "show-network-info": {
    id: "show-network-info",
    title: "Show Network Info",
    subtitle: "Display current IP addresses and Wi-Fi SSID.",
    category: "network",
    strategy: "shell",
    keywords: ["network", "ip", "wifi", "ssid", "info"],
    macOnly: true
  },
  "show-public-ip": {
    id: "show-public-ip",
    title: "Show Public IP",
    subtitle: "Look up the public IPv4 address of this connection.",
    category: "network",
    strategy: "shell",
    keywords: ["ip", "public", "external", "wan"],
    macOnly: false
  },
  "flush-dns-cache": {
    id: "flush-dns-cache",
    title: "Flush DNS Cache",
    subtitle: "Clear the macOS resolver and mDNSResponder caches.",
    category: "network",
    strategy: "shell",
    keywords: ["dns", "flush", "cache", "network", "resolver"],
    macOnly: true
  },
  "toggle-vpn-menu": {
    id: "toggle-vpn-menu",
    title: "Open VPN Menu",
    subtitle: "Open the menu-bar VPN/Network control.",
    category: "network",
    strategy: "shell",
    keywords: ["vpn", "network", "menu"],
    macOnly: true
  },
  "empty-trash": {
    id: "empty-trash",
    title: "Empty Trash",
    subtitle: "Permanently delete everything in the Trash.",
    category: "system",
    strategy: "applescript",
    keywords: ["trash", "empty", "delete", "clean"],
    destructive: true,
    macOnly: true
  },
  "lock-screen": {
    id: "lock-screen",
    title: "Lock Screen",
    subtitle: "Lock the current session.",
    category: "system",
    strategy: "applescript",
    keywords: ["lock", "screen", "session", "away"],
    macOnly: true
  },
  "open-downloads": {
    id: "open-downloads",
    title: "Open Downloads Folder",
    subtitle: "Reveal ~/Downloads in Finder.",
    category: "files",
    strategy: "shell",
    keywords: ["downloads", "folder", "finder"],
    macOnly: true
  },
  "open-applications": {
    id: "open-applications",
    title: "Open Applications Folder",
    subtitle: "Reveal /Applications in Finder.",
    category: "files",
    strategy: "shell",
    keywords: ["applications", "apps", "finder"],
    macOnly: true
  },
  "reveal-library": {
    id: "reveal-library",
    title: "Open ~/Library",
    subtitle: "Reveal the hidden Library folder in Finder.",
    category: "files",
    strategy: "shell",
    keywords: ["library", "hidden", "finder"],
    macOnly: true
  },
  "copy-current-path": {
    id: "copy-current-path",
    title: "Copy Path of Frontmost Finder Window",
    subtitle: "Copy the path of the folder open in Finder.",
    category: "files",
    strategy: "applescript",
    keywords: ["path", "finder", "copy", "directory"],
    macOnly: true
  },
  "quit-tezbar": {
    id: "quit-tezbar",
    title: "Quit Tezbar",
    subtitle: "Quit Tezbar and terminate all background processes.",
    category: "system",
    strategy: "native-helper",
    keywords: ["quit", "tezbar", "exit", "close", "shutdown", "terminate", "app"],
    macOnly: false
  },
  "show-macos-version": {
    id: "show-macos-version",
    title: "Show macOS Version",
    subtitle: "Print kernel, build, and macOS version.",
    category: "dev",
    strategy: "shell",
    keywords: ["macos", "version", "kernel", "build"],
    macOnly: true
  },
  "show-cpu-info": {
    id: "show-cpu-info",
    title: "Show CPU Info",
    subtitle: "Display CPU brand, cores, and load averages.",
    category: "dev",
    strategy: "shell",
    keywords: ["cpu", "processor", "cores", "load"],
    macOnly: true
  },
  "show-memory-info": {
    id: "show-memory-info",
    title: "Show Memory Pressure",
    subtitle: "Display current memory pressure and free memory.",
    category: "dev",
    strategy: "shell",
    keywords: ["memory", "ram", "pressure", "free"],
    macOnly: true
  },
  "show-disk-usage": {
    id: "show-disk-usage",
    title: "Show Disk Usage",
    subtitle: "Display disk capacity and free space.",
    category: "dev",
    strategy: "shell",
    keywords: ["disk", "storage", "free", "usage"],
    macOnly: true
  },
  "show-battery-status": {
    id: "show-battery-status",
    title: "Show Battery Status",
    subtitle: "Display battery capacity and charging state.",
    category: "dev",
    strategy: "shell",
    keywords: ["battery", "charge", "power", "percent"],
    macOnly: true
  },
  "list-listening-ports": {
    id: "list-listening-ports",
    title: "List Listening Ports",
    subtitle: "Open Port Manager with a structured list (same as Open Ports).",
    category: "dev",
    strategy: "shell",
    keywords: ["ports", "lsof", "listen", "listening", "tcp", "dev", "port manager"],
    macOnly: true
  },
  "git-root": {
    id: "git-root",
    title: "Git: Copy Repo Root",
    subtitle: "Copy the root of the git repo open in Finder.",
    category: "dev",
    strategy: "applescript",
    keywords: ["git", "root", "repo", "copy"],
    macOnly: true
  },
  "brew-outdated": {
    id: "brew-outdated",
    title: "Homebrew: Show Outdated",
    subtitle: "List formulae that have updates available.",
    category: "dev",
    strategy: "shell",
    keywords: ["brew", "homebrew", "outdated", "updates"],
    macOnly: true
  },
  "brew-update": {
    id: "brew-update",
    title: "Homebrew: Update",
    subtitle: "Refresh Homebrew formula metadata.",
    category: "dev",
    strategy: "shell",
    keywords: ["brew", "homebrew", "update", "refresh"],
    macOnly: true
  },
  // This command has no main-process implementation — it's intercepted in
  // the renderer and navigates to the dedicated clipboard surface. Keeping
  // it in the registry means it participates in ranking, intent routing,
  // and fuzzy search like every other command.
  "open-clipboard-history": {
    id: "open-clipboard-history",
    title: "Clipboard History",
    subtitle: "Browse everything you have copied \u2014 text, images, files.",
    category: "productivity",
    strategy: "native-helper",
    keywords: ["clipboard", "history", "paste", "copy", "pasteboard"],
    macOnly: false
  },
  "open-snippets": {
    id: "open-snippets",
    title: "Snippets",
    subtitle: "Browse, copy, and create your own text snippets (dates, UUIDs, templates, \u2026).",
    category: "productivity",
    strategy: "native-helper",
    keywords: ["snippet", "snippets", "template", "templates", "text", "boilerplate", "expander", "macro"],
    macOnly: false
  },
  "open-quick-notes": {
    id: "open-quick-notes",
    title: "Quick Notes",
    subtitle: "View and edit saved notes with rich text; first line is the title.",
    category: "productivity",
    strategy: "native-helper",
    keywords: ["notes", "quick notes", "notepad", "rich text", "memo", "jot"],
    macOnly: false
  },
  "open-emoji-picker": {
    id: "open-emoji-picker",
    title: "Emoji Picker",
    subtitle: "Browse and copy emojis by name, mood, and category.",
    category: "productivity",
    strategy: "native-helper",
    keywords: ["emoji", "smiley", "symbol", "icon", "face", "emoticon"],
    macOnly: false
  }
};
function getNativeCommand(id) {
  return Object.prototype.hasOwnProperty.call(DESCRIPTORS, id) ? DESCRIPTORS[id] : null;
}
function listNativeCommands() {
  return Object.values(DESCRIPTORS);
}

// src/main/nativeCommands/executor.ts
var execFileAsync8 = (0, import_node_util8.promisify)(import_node_child_process9.execFile);
async function runAppleScript3(source) {
  const { stdout } = await execFileAsync8("osascript", ["-e", source]);
  return stdout.trim();
}
async function runShell(script) {
  const { stdout } = await execFileAsync8("bash", ["-lc", script]);
  return stdout.trim();
}
var backgroundProcesses = /* @__PURE__ */ new Map();
function startBackground(key, command, args) {
  const existing = backgroundProcesses.get(key);
  if (existing && isProcessAlive(existing)) return;
  const child = (0, import_node_child_process9.spawn)(command, args, { detached: true, stdio: "ignore" });
  child.unref();
  if (child.pid) backgroundProcesses.set(key, child.pid);
}
function stopBackground(key) {
  const pid = backgroundProcesses.get(key);
  if (!pid) return false;
  try {
    process.kill(pid, "SIGTERM");
    backgroundProcesses.delete(key);
    return true;
  } catch {
    backgroundProcesses.delete(key);
    return false;
  }
}
function isProcessAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}
async function executeNativeCommand(id) {
  const descriptor = getNativeCommand(id);
  if (!descriptor) {
    return { ok: false, message: `Unknown command: ${id}` };
  }
  if (descriptor.macOnly && process.platform !== "darwin") {
    return { ok: false, message: `${descriptor.title} is only available on macOS.` };
  }
  try {
    switch (id) {
      case "toggle-dark-mode": {
        const script = 'tell application "System Events" to tell appearance preferences to set dark mode to not dark mode';
        await runAppleScript3(script);
        return { ok: true, message: "Toggled Dark Mode" };
      }
      case "toggle-mute": {
        await runAppleScript3("set volume output muted (not (output muted of (get volume settings)))");
        return { ok: true, message: "Toggled system mute" };
      }
      case "toggle-hide-desktop-icons": {
        const script = `current=$(defaults read com.apple.finder CreateDesktop 2>/dev/null || echo true); if [ "$current" = "false" ]; then defaults write com.apple.finder CreateDesktop true; else defaults write com.apple.finder CreateDesktop false; fi; killall Finder`;
        await runShell(script);
        return { ok: true, message: "Toggled desktop icons" };
      }
      case "toggle-autohide-dock": {
        const script = `current=$(defaults read com.apple.dock autohide 2>/dev/null || echo 0); if [ "$current" = "1" ]; then defaults write com.apple.dock autohide -bool false; else defaults write com.apple.dock autohide -bool true; fi; killall Dock`;
        await runShell(script);
        return { ok: true, message: "Toggled Dock auto-hide" };
      }
      case "toggle-autohide-menu-bar": {
        const script = `current=$(defaults read NSGlobalDomain _HIHideMenuBar 2>/dev/null || echo 0); if [ "$current" = "1" ]; then defaults write NSGlobalDomain _HIHideMenuBar -bool false; else defaults write NSGlobalDomain _HIHideMenuBar -bool true; fi; killall SystemUIServer`;
        await runShell(script);
        return { ok: true, message: "Toggled menu bar auto-hide" };
      }
      case "start-keep-awake": {
        startBackground("caffeinate", "caffeinate", ["-di"]);
        return { ok: true, message: "Keep Awake is on \u2014 system will not sleep." };
      }
      case "stop-keep-awake": {
        const stopped = stopBackground("caffeinate");
        return {
          ok: true,
          message: stopped ? "Keep Awake turned off." : "Keep Awake was not running."
        };
      }
      case "start-screen-saver": {
        await runShell("open -a ScreenSaverEngine");
        return { ok: true, message: "Started screen saver" };
      }
      case "toggle-bluetooth": {
        try {
          const current = await runShell("blueutil -p");
          const next = current === "1" ? "0" : "1";
          await runShell(`blueutil -p ${next}`);
          return { ok: true, message: `Bluetooth ${next === "1" ? "enabled" : "disabled"}` };
        } catch {
          return {
            ok: false,
            message: "Bluetooth control requires `blueutil`. Install with `brew install blueutil`."
          };
        }
      }
      case "show-network-info": {
        const script = `echo "IP: $(ipconfig getifaddr en0 2>/dev/null || echo n/a)"; echo "Wi-Fi: $(networksetup -getairportnetwork en0 2>/dev/null | sed 's/Current Wi-Fi Network: //')"`;
        const out = await runShell(script);
        return { ok: true, message: out || "No network info available" };
      }
      case "flush-dns-cache": {
        try {
          await runShell("sudo -n dscacheutil -flushcache && sudo -n killall -HUP mDNSResponder");
          return { ok: true, message: "Flushed DNS cache" };
        } catch {
          return {
            ok: false,
            message: "DNS flush requires `sudo`. Run `sudo dscacheutil -flushcache` in Terminal."
          };
        }
      }
      case "empty-trash": {
        await runAppleScript3('tell application "Finder" to empty the trash');
        return { ok: true, message: "Emptied Trash" };
      }
      case "lock-screen": {
        await runAppleScript3(
          'tell application "System Events" to keystroke "q" using {command down, control down}'
        );
        return { ok: true, message: "Screen locked" };
      }
      case "sleep-display": {
        await runShell("pmset displaysleepnow");
        return { ok: true, message: "Display sleeping" };
      }
      case "volume-up": {
        await runAppleScript3("set volume output volume (output volume of (get volume settings) + 10)");
        return { ok: true, message: "Volume up" };
      }
      case "volume-down": {
        await runAppleScript3("set volume output volume (output volume of (get volume settings) - 10)");
        return { ok: true, message: "Volume down" };
      }
      case "restart-dock": {
        await runShell("killall Dock");
        return { ok: true, message: "Dock relaunched" };
      }
      case "restart-finder": {
        await runShell("killall Finder");
        return { ok: true, message: "Finder relaunched" };
      }
      case "restart-menu-bar": {
        await runShell("killall SystemUIServer");
        return { ok: true, message: "Menu bar relaunched" };
      }
      case "sleep-system": {
        await runAppleScript3('tell application "System Events" to sleep');
        return { ok: true, message: "System sleeping" };
      }
      case "toggle-wifi": {
        const script = `iface=$(networksetup -listallhardwareports | awk '/Wi-Fi/{getline; print $2; exit}'); if [ -z "$iface" ]; then exit 1; fi; state=$(networksetup -getairportpower "$iface" | awk '{print $NF}'); if [ "$state" = "On" ]; then networksetup -setairportpower "$iface" off; echo off; else networksetup -setairportpower "$iface" on; echo on; fi`;
        const out = await runShell(script);
        return { ok: true, message: `Wi-Fi ${out || "toggled"}` };
      }
      case "show-public-ip": {
        const out = await runShell('curl -m 4 -fsS https://api.ipify.org || echo "(unreachable)"');
        return { ok: true, message: `Public IP: ${out}` };
      }
      case "toggle-vpn-menu": {
        await runShell('open "x-apple.systempreferences:com.apple.preference.network"');
        return { ok: true, message: "Opened Network preferences" };
      }
      case "open-downloads": {
        await runShell("open ~/Downloads");
        return { ok: true, message: "Opened Downloads" };
      }
      case "open-applications": {
        await runShell("open /Applications");
        return { ok: true, message: "Opened Applications" };
      }
      case "reveal-library": {
        await runShell("open ~/Library");
        return { ok: true, message: "Opened ~/Library" };
      }
      case "copy-current-path": {
        const path7 = await runAppleScript3(
          'tell application "Finder" to try\nset thePath to POSIX path of (target of front Finder window as alias)\nset the clipboard to thePath\nreturn thePath\non error\nreturn ""\nend try'
        );
        if (!path7) {
          return { ok: false, message: "No Finder window is open." };
        }
        return { ok: true, message: `Copied: ${path7}` };
      }
      case "show-macos-version": {
        const out = await runShell("sw_vers && uname -v");
        return { ok: true, message: out };
      }
      case "show-cpu-info": {
        const out = await runShell(
          `sysctl -n machdep.cpu.brand_string 2>/dev/null; echo "Cores: $(sysctl -n hw.ncpu)"; uptime | awk -F'load averages:' '{print "Load:"$2}'`
        );
        return { ok: true, message: out };
      }
      case "show-memory-info": {
        const out = await runShell(
          "memory_pressure | head -n 6; echo; vm_stat | awk 'NR<=6'"
        );
        return { ok: true, message: out };
      }
      case "show-disk-usage": {
        const out = await runShell("df -h / | tail -n 1");
        return { ok: true, message: out };
      }
      case "show-battery-status": {
        const out = await runShell("pmset -g batt | tail -n +2");
        return { ok: true, message: out || "No battery detected" };
      }
      case "list-listening-ports": {
        return {
          ok: true,
          message: "Use Port Manager \u2192 Open Ports in Tezbar for a structured, filterable list. (Raw lsof output is intentionally not shown here.)"
        };
      }
      case "git-root": {
        const path7 = await runAppleScript3(
          'tell application "Finder" to try\nset thePath to POSIX path of (target of front Finder window as alias)\nreturn thePath\non error\nreturn ""\nend try'
        );
        if (!path7) {
          return { ok: false, message: "No Finder window is open." };
        }
        try {
          const root = await runShell(`cd ${JSON.stringify(path7)} && git rev-parse --show-toplevel`);
          await runShell(`printf %s ${JSON.stringify(root)} | pbcopy`);
          return { ok: true, message: `Copied repo root: ${root}` };
        } catch {
          return { ok: false, message: `${path7} is not inside a git repo.` };
        }
      }
      case "brew-outdated": {
        try {
          const out = await runShell("brew outdated --quiet");
          return {
            ok: true,
            message: out.trim().length === 0 ? "All Homebrew formulae are up to date." : out
          };
        } catch {
          return { ok: false, message: "Homebrew is not installed or not in PATH." };
        }
      }
      case "open-clipboard-history": {
        return {
          ok: false,
          message: "Clipboard History is a UI navigation \u2014 open the launcher to browse it."
        };
      }
      case "open-snippets": {
        return {
          ok: false,
          message: "Snippets is a UI navigation \u2014 open the launcher to browse it."
        };
      }
      case "open-quick-notes": {
        return {
          ok: false,
          message: "Quick Notes is a UI navigation \u2014 open the launcher to browse it."
        };
      }
      case "open-emoji-picker": {
        return {
          ok: false,
          message: "Emoji Picker is a UI navigation \u2014 open the launcher to browse it."
        };
      }
      case "quit-tezbar": {
        return {
          ok: false,
          message: "Quit Tezbar is handled by the launcher so it can show the confirmation dialog."
        };
      }
      case "brew-update": {
        try {
          const out = await runShell("brew update");
          return { ok: true, message: out.slice(-400) || "Homebrew updated." };
        } catch {
          return { ok: false, message: "Homebrew is not installed or not in PATH." };
        }
      }
      default: {
        return { ok: false, message: `Command ${descriptor.title} is registered but has no executor yet.` };
      }
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return { ok: false, message: `${descriptor.title} failed: ${message}` };
  }
}

// src/main/search/service.ts
init_configStore();

// src/main/safety/confirm.ts
init_electron_shim();

// src/main/safety/registry.ts
var DESCRIPTORS2 = {
  "shell.run": {
    id: "shell.run",
    title: "Run shell command",
    summary: "Execute a shell command in your user environment.",
    risk: "high",
    requiresConfirmation: true
  },
  "process.kill": {
    id: "process.kill",
    title: "Kill process",
    summary: "Forcibly terminate a running process.",
    risk: "high",
    requiresConfirmation: true
  },
  "port.kill": {
    id: "port.kill",
    title: "Kill listener on port",
    summary: "Terminate the process listening on this TCP port.",
    risk: "medium",
    requiresConfirmation: true
  },
  "system.shutdown": {
    id: "system.shutdown",
    title: "Shut down Mac",
    summary: "Shut the computer down immediately.",
    risk: "high",
    requiresConfirmation: true
  },
  "system.restart": {
    id: "system.restart",
    title: "Restart Mac",
    summary: "Restart the computer immediately.",
    risk: "high",
    requiresConfirmation: true
  },
  "system.sleep": {
    id: "system.sleep",
    title: "Sleep Mac",
    summary: "Put the computer to sleep.",
    risk: "low",
    requiresConfirmation: false
  },
  "system.logout": {
    id: "system.logout",
    title: "Log out",
    summary: "Log out of the current macOS user.",
    risk: "high",
    requiresConfirmation: true
  },
  "trash.empty": {
    id: "trash.empty",
    title: "Empty Trash",
    summary: "Permanently delete everything in the Trash.",
    risk: "high",
    requiresConfirmation: true
  },
  "app.quit": {
    id: "app.quit",
    title: "Quit application",
    summary: "Quit a running application.",
    risk: "low",
    requiresConfirmation: false
  },
  "extension.install": {
    id: "extension.install",
    title: "Install extension",
    summary: "Download and install a Raycast extension.",
    risk: "medium",
    requiresConfirmation: false
  },
  "extension.uninstall": {
    id: "extension.uninstall",
    title: "Uninstall extension",
    summary: "Remove an installed extension and its files.",
    risk: "medium",
    requiresConfirmation: true
  },
  "native.command": {
    id: "native.command",
    title: "Run system command",
    summary: "Execute a built-in macOS control (toggle, query, helper).",
    risk: "low",
    requiresConfirmation: false
  }
};
function getSafetyDescriptor(id) {
  return Object.prototype.hasOwnProperty.call(DESCRIPTORS2, id) ? DESCRIPTORS2[id] : null;
}
function listSafetyDescriptors() {
  return Object.values(DESCRIPTORS2);
}

// src/main/safety/confirm.ts
var RISK_LABEL = {
  low: "Low risk",
  medium: "Use with care",
  high: "Destructive"
};
async function confirmSafetyAction(window2, descriptor, context, options) {
  if (!getSafetyDescriptor(descriptor.id)) {
    return { accepted: false };
  }
  if (!descriptor.requiresConfirmation && !options?.dryRun) {
    return { accepted: true };
  }
  const detailLines = [];
  if (options?.dryRun) {
    detailLines.push("Dry-run mode: no changes will be made.");
  }
  if (descriptor.details) detailLines.push(descriptor.details);
  if (context) {
    for (const [key, value] of Object.entries(context)) {
      if (value === void 0 || value === null || value === "") continue;
      detailLines.push(`${key}: ${String(value)}`);
    }
  }
  detailLines.push(`Risk: ${RISK_LABEL[descriptor.risk]}`);
  const primaryLabel = options?.dryRun ? `Preview: ${descriptor.title}` : descriptor.title;
  const opts = {
    type: descriptor.risk === "high" && !options?.dryRun ? "warning" : "question",
    buttons: ["Cancel", primaryLabel],
    defaultId: 0,
    cancelId: 0,
    title: primaryLabel,
    message: descriptor.summary,
    detail: detailLines.join("\n"),
    noLink: true
  };
  const response = window2 && !window2.isDestroyed() ? await dialog.showMessageBox(window2, opts) : await dialog.showMessageBox(opts);
  return { accepted: response.response === 1 };
}

// src/main/safety/log.ts
init_electron_shim();
var import_node_fs13 = require("node:fs");
var import_node_path13 = require("node:path");
var MAX_ENTRIES = 200;
var cache = null;
function logPath() {
  const dir = (0, import_node_path13.join)(app.getPath("userData"), "safety");
  (0, import_node_fs13.mkdirSync)(dir, { recursive: true });
  return (0, import_node_path13.join)(dir, "action-log.json");
}
function load() {
  if (cache) return cache;
  try {
    const raw = (0, import_node_fs13.readFileSync)(logPath(), "utf8");
    const parsed = JSON.parse(raw);
    cache = Array.isArray(parsed.entries) ? parsed.entries : [];
  } catch {
    cache = [];
  }
  return cache;
}
function persist() {
  if (!cache) return;
  try {
    (0, import_node_fs13.writeFileSync)(logPath(), JSON.stringify({ entries: cache }, null, 2), "utf8");
  } catch {
  }
}
function truncateContext(context) {
  if (!context) return void 0;
  const out = {};
  for (const [key, value] of Object.entries(context)) {
    if (typeof value === "string") {
      out[key] = value.length > 200 ? `${value.slice(0, 200)}\u2026` : value;
    } else if (value === null || ["number", "boolean"].includes(typeof value)) {
      out[key] = value;
    } else {
      try {
        const serialised = JSON.stringify(value);
        out[key] = serialised.length > 200 ? `${serialised.slice(0, 200)}\u2026` : serialised;
      } catch {
        out[key] = "[unserializable]";
      }
    }
  }
  return out;
}
function recordSafetyEntry(entry) {
  const full = {
    ...entry,
    id: `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`,
    at: Date.now(),
    context: truncateContext(entry.context)
  };
  const list = load();
  list.unshift(full);
  if (list.length > MAX_ENTRIES) list.length = MAX_ENTRIES;
  persist();
  return full;
}
function listSafetyLog() {
  return load().slice();
}
function clearSafetyLog() {
  cache = [];
  persist();
}

// src/main/search/commandBus.ts
var import_node_child_process10 = require("node:child_process");
var import_node_util9 = require("node:util");
var execFileAsync9 = (0, import_node_util9.promisify)(import_node_child_process10.execFile);
function osascriptCommandHandler(script, successMessage) {
  return async () => {
    await execFileAsync9("/usr/bin/osascript", ["-e", script]);
    return { ok: true, message: successMessage };
  };
}
var CommandBus = class {
  commands = /* @__PURE__ */ new Map();
  constructor() {
    this.registerBuiltins();
  }
  register(def) {
    this.commands.set(def.id, def);
  }
  registerBuiltins() {
    this.register({
      id: "system.dark-mode.on",
      title: "Enable dark mode",
      permission: "system-control",
      confirmation: "recommended",
      analyticsKey: "system.dark_mode_on",
      handler: osascriptCommandHandler(
        'tell application "System Events" to tell appearance preferences to set dark mode to true',
        "Dark mode enabled"
      )
    });
    this.register({
      id: "system.dark-mode.off",
      title: "Disable dark mode",
      permission: "system-control",
      confirmation: "recommended",
      analyticsKey: "system.dark_mode_off",
      handler: osascriptCommandHandler(
        'tell application "System Events" to tell appearance preferences to set dark mode to false',
        "Dark mode disabled"
      )
    });
    this.register({
      id: "speech.read-aloud",
      title: "Read text aloud",
      permission: "none",
      confirmation: "never",
      analyticsKey: "speech.read_aloud",
      handler: async (payload) => {
        const text = String(payload?.text ?? "").trim();
        if (!text) {
          return { ok: false, message: "No text provided for read-aloud" };
        }
        await execFileAsync9("say", [text]);
        return { ok: true, message: "Reading aloud" };
      }
    });
  }
  async execute(context) {
    const command = this.commands.get(context.commandId);
    if (!command) {
      return { ok: false, message: `Unknown command: ${context.commandId}` };
    }
    return command.handler(context.payload);
  }
};
var commandBus = new CommandBus();

// src/main/search/indexDb.ts
init_electron_shim();
var import_node_fs14 = require("node:fs");
var import_node_path14 = require("node:path");

// src/main/search/textMatch.ts
function tokenizeQuery(query) {
  return query.toLowerCase().trim().split(/\s+/).map((token) => token.replace(/[^a-z0-9._-]/g, "")).filter(Boolean);
}
function lexicalScore(text, query) {
  const t = text.toLowerCase();
  const q = query.toLowerCase().trim();
  if (!q) return 0;
  if (t === q) return 1;
  if (t.startsWith(q)) return 0.9;
  if (t.includes(q)) return 0.75;
  const tokens = tokenizeQuery(q);
  if (tokens.length === 0) return 0;
  let matched = 0;
  for (const token of tokens) {
    if (t.includes(token)) matched += 1;
  }
  return matched / tokens.length / 1.5;
}
function levenshteinDistance(left, right) {
  if (left === right) return 0;
  if (!left) return right.length;
  if (!right) return left.length;
  const a = left.toLowerCase();
  const b = right.toLowerCase();
  const prev = Array.from({ length: b.length + 1 }, (_, i) => i);
  const cur = new Array(b.length + 1).fill(0);
  for (let i = 1; i <= a.length; i += 1) {
    cur[0] = i;
    for (let j = 1; j <= b.length; j += 1) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      cur[j] = Math.min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + cost);
    }
    for (let j = 0; j <= b.length; j += 1) {
      prev[j] = cur[j];
    }
  }
  return prev[b.length];
}
function buildFtsQuery(query) {
  const tokens = query.toLowerCase().match(/[a-z0-9]+/g) ?? [];
  if (tokens.length === 0) return "";
  return tokens.map((token) => `${token}*`).join(" OR ");
}

// src/main/search/indexDb.ts
function dbPath2() {
  const dir = (0, import_node_path14.join)(app.getPath("userData"), "search");
  (0, import_node_fs14.mkdirSync)(dir, { recursive: true });
  return (0, import_node_path14.join)(dir, "index.sqlite3");
}
function safeJsonParse(value, fallback) {
  try {
    return JSON.parse(value);
  } catch {
    return fallback;
  }
}
var CLICK_EVENTS_RETAIN = 1e3;
var BENCHMARK_SNAPSHOTS_RETAIN = 50;
async function readBenchmarkHistory() {
  return [];
}
async function runOfflineBenchmarks(_searchFn, _db) {
}
var _instance = null;
function getInstance() {
  if (!_instance) {
    _instance = new SearchIndexDatabase();
  }
  return _instance;
}
var SearchIndexDatabase = class {
  _db = null;
  _initPromise = null;
  get db() {
    if (!this._db) {
      throw new Error("Database not initialized - call ensureInitialized() first");
    }
    return this._db;
  }
  async ensureInitialized() {
    if (this._initPromise) return this._initPromise;
    this._initPromise = new Promise((resolve4) => {
      setImmediate(() => {
        this._db = new better_sqlite3_shim_default(dbPath2());
        this._db.pragma("journal_mode = WAL");
        this._db.pragma("synchronous = NORMAL");
        this.bootstrap();
        resolve4();
      });
    });
    return this._initPromise;
  }
  bootstrap() {
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS documents (
        id TEXT PRIMARY KEY,
        category TEXT NOT NULL,
        title TEXT NOT NULL,
        subtitle TEXT NOT NULL,
        tokens TEXT NOT NULL,
        action_json TEXT NOT NULL,
        updated_at INTEGER NOT NULL,
        source_path TEXT,
        source_mtime INTEGER,
        popularity REAL NOT NULL DEFAULT 0
      );

      CREATE VIRTUAL TABLE IF NOT EXISTS documents_fts USING fts5(
        id UNINDEXED,
        title,
        subtitle,
        tokens,
        tokenize = 'unicode61'
      );

      CREATE TABLE IF NOT EXISTS action_stats (
        action_id TEXT PRIMARY KEY,
        frequency INTEGER NOT NULL DEFAULT 0,
        success_count INTEGER NOT NULL DEFAULT 0,
        total_count INTEGER NOT NULL DEFAULT 0,
        last_used_at INTEGER NOT NULL DEFAULT 0
      );

      CREATE TABLE IF NOT EXISTS benchmark_snapshots (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        created_at INTEGER NOT NULL,
        precision_at_5 REAL NOT NULL,
        precision_at_10 REAL NOT NULL,
        avg_click_rank REAL NOT NULL,
        benchmark_size INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS click_events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        created_at INTEGER NOT NULL,
        query TEXT NOT NULL,
        result_id TEXT NOT NULL,
        rank INTEGER NOT NULL,
        success INTEGER NOT NULL
      );
    `);
    this.ensureDocumentsSchema();
    this.pruneTelemetry();
  }
  /** Remove old click-events and benchmark snapshots so the DB doesn't grow
   *  without bound. Retention limits are conservative — enough for ranking
   *  learning and debugging without unbounded disk use. */
  pruneTelemetry() {
    try {
      this.db.prepare(
        `DELETE FROM click_events WHERE id NOT IN (
            SELECT id FROM click_events ORDER BY id DESC LIMIT ?
          )`
      ).run(CLICK_EVENTS_RETAIN);
      this.db.prepare(
        `DELETE FROM benchmark_snapshots WHERE id NOT IN (
            SELECT id FROM benchmark_snapshots ORDER BY id DESC LIMIT ?
          )`
      ).run(BENCHMARK_SNAPSHOTS_RETAIN);
    } catch (error) {
      console.warn("[SearchIndex] Telemetry pruning failed:", error);
    }
  }
  /** Run WAL checkpoint and VACUUM to reclaim disk space. */
  vacuum() {
    this.db.exec("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;");
  }
  /** Forward-compatible schema patching for users with older local DBs. */
  ensureDocumentsSchema() {
    const rows = this.db.prepare("PRAGMA table_info(documents)").all();
    const columns = new Set(rows.map((row) => row.name));
    if (!columns.has("source_path")) {
      this.db.exec("ALTER TABLE documents ADD COLUMN source_path TEXT");
    }
    if (!columns.has("source_mtime")) {
      this.db.exec("ALTER TABLE documents ADD COLUMN source_mtime INTEGER");
    }
    if (!columns.has("popularity")) {
      this.db.exec("ALTER TABLE documents ADD COLUMN popularity REAL NOT NULL DEFAULT 0");
    }
  }
  upsertDocuments(documents) {
    if (documents.length === 0) return;
    const upsertDoc = this.db.prepare(`
      INSERT INTO documents (id, category, title, subtitle, tokens, action_json, updated_at, source_path, source_mtime, popularity)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
      ON CONFLICT(id) DO UPDATE SET
        category = excluded.category,
        title = excluded.title,
        subtitle = excluded.subtitle,
        tokens = excluded.tokens,
        action_json = excluded.action_json,
        updated_at = excluded.updated_at,
        source_path = excluded.source_path,
        source_mtime = excluded.source_mtime,
        popularity = excluded.popularity
    `);
    const deleteFts = this.db.prepare("DELETE FROM documents_fts WHERE id = ?");
    const insertFts = this.db.prepare(
      "INSERT INTO documents_fts (id, title, subtitle, tokens) VALUES (?, ?, ?, ?)"
    );
    const upsertTx = this.db.transaction((rows) => {
      for (const row of rows) {
        upsertDoc.run(
          row.id,
          row.category,
          row.title,
          row.subtitle,
          row.tokens,
          JSON.stringify(row.action),
          Math.round(row.updatedAt || Date.now()),
          row.sourcePath ?? null,
          row.sourceMtime ? Math.round(row.sourceMtime) : null,
          row.popularity ?? 0
        );
        deleteFts.run(row.id);
        insertFts.run(row.id, row.title, row.subtitle, row.tokens);
      }
    });
    upsertTx(documents);
  }
  removeDocumentById(id) {
    this.db.prepare("DELETE FROM documents WHERE id = ?").run(id);
    this.db.prepare("DELETE FROM documents_fts WHERE id = ?").run(id);
  }
  removeDocumentsByCategory(category) {
    const ids = this.db.prepare("SELECT id FROM documents WHERE category = ?").all(category);
    if (ids.length === 0) return 0;
    const delDoc = this.db.prepare("DELETE FROM documents WHERE id = ?");
    const delFts = this.db.prepare("DELETE FROM documents_fts WHERE id = ?");
    const removeTx = this.db.transaction((rows) => {
      for (const row of rows) {
        delDoc.run(row.id);
        delFts.run(row.id);
      }
    });
    removeTx(ids);
    return ids.length;
  }
  replaceDocumentsByCategory(category, documents) {
    const deleteFts = this.db.prepare(
      "DELETE FROM documents_fts WHERE id IN (SELECT id FROM documents WHERE category = ?)"
    );
    const deleteDocuments = this.db.prepare("DELETE FROM documents WHERE category = ?");
    const replaceTx = this.db.transaction(() => {
      deleteFts.run(category);
      deleteDocuments.run(category);
      this.upsertDocuments(documents);
    });
    replaceTx();
    this.clearSearchCache();
  }
  search(query, limit) {
    const ftsQuery = buildFtsQuery(query);
    const trimmed = query.trim();
    if (!trimmed) return [];
    const candidateLimit = Math.max(limit * 2, 20);
    const rows = ftsQuery.length > 0 ? this.db.prepare(
      `
                SELECT d.id AS id,
                       d.category AS category,
                       d.title AS title,
                       d.subtitle AS subtitle,
                       d.action_json AS actionJson,
                       d.updated_at AS updatedAt,
                       d.popularity AS popularity,
                       bm25(documents_fts, 5.0, 2.0, 1.0) AS bm25Score
                FROM documents_fts
                JOIN documents d ON d.id = documents_fts.id
                WHERE documents_fts MATCH ?
                ORDER BY bm25Score ASC
                LIMIT ?
              `
    ).all(ftsQuery, candidateLimit) : [];
    const mapped = rows.map((row) => {
      const inverseBm25 = Number.isFinite(row.bm25Score) ? 1 / (1 + Math.max(row.bm25Score, 0)) : 0.5;
      const lexical = Math.max(inverseBm25, lexicalScore(`${row.title} ${row.subtitle}`, trimmed));
      return {
        id: row.id,
        category: row.category,
        title: row.title,
        subtitle: row.subtitle,
        actionJson: row.actionJson,
        updatedAt: row.updatedAt,
        lexical,
        popularity: row.popularity
      };
    });
    if (mapped.length >= candidateLimit) {
      return mapped.slice(0, candidateLimit);
    }
    return [...mapped, ...this.fuzzySearch(trimmed, candidateLimit - mapped.length)];
  }
  fuzzySearch(query, limit) {
    if (limit <= 0) return [];
    const rows = this.db.prepare(
      `
          SELECT id, category, title, subtitle, action_json AS actionJson, updated_at AS updatedAt, popularity
          FROM documents
          ORDER BY updated_at DESC
          LIMIT ?
        `
    ).all(Math.max(300, limit * 30));
    const scored = [];
    for (const row of rows) {
      const candidate = row.title.toLowerCase();
      const distance = levenshteinDistance(candidate, query.toLowerCase());
      if (distance > 3 && !candidate.includes(query.toLowerCase())) continue;
      const lexical = lexicalScore(`${row.title} ${row.subtitle}`, query);
      scored.push({
        id: row.id,
        category: row.category,
        title: row.title,
        subtitle: row.subtitle,
        actionJson: row.actionJson,
        updatedAt: row.updatedAt,
        lexical,
        fuzzyDistance: distance,
        popularity: row.popularity
      });
    }
    scored.sort((a, b) => {
      if (a.fuzzyDistance !== void 0 && b.fuzzyDistance !== void 0 && a.fuzzyDistance !== b.fuzzyDistance) {
        return a.fuzzyDistance - b.fuzzyDistance;
      }
      return b.lexical - a.lexical;
    });
    return scored.slice(0, limit);
  }
  parseAction(actionJson) {
    return safeJsonParse(actionJson, { type: "copy-text", text: "" });
  }
  getActionStats(actionIds) {
    if (actionIds.length === 0) return /* @__PURE__ */ new Map();
    const placeholders = actionIds.map(() => "?").join(",");
    const rows = this.db.prepare(
      `
          SELECT action_id AS actionId,
                 frequency AS frequency,
                 success_count AS successCount,
                 total_count AS totalCount,
                 last_used_at AS lastUsedAt
          FROM action_stats
          WHERE action_id IN (${placeholders})
        `
    ).all(...actionIds);
    return new Map(rows.map((row) => [row.actionId, row]));
  }
  getDocumentsByIds(ids) {
    if (ids.length === 0) return [];
    const placeholders = ids.map(() => "?").join(",");
    const rows = this.db.prepare(
      `
          SELECT id,
                 category,
                 title,
                 subtitle,
                 action_json AS actionJson,
                 updated_at AS updatedAt,
                 popularity
          FROM documents
          WHERE id IN (${placeholders})
        `
    ).all(...ids);
    return rows.map((row) => ({
      id: row.id,
      category: row.category,
      title: row.title,
      subtitle: row.subtitle,
      actionJson: row.actionJson,
      updatedAt: row.updatedAt,
      lexical: 0,
      popularity: row.popularity
    }));
  }
  listRecommendedDocuments(limit) {
    if (limit <= 0) return [];
    return this.db.prepare(
      `
          SELECT d.id AS id,
                 d.category AS category,
                 d.title AS title,
                 d.subtitle AS subtitle,
                 d.action_json AS actionJson,
                 d.updated_at AS updatedAt,
                 COALESCE(a.frequency, 0) AS frequency,
                 COALESCE(a.success_count, 0) AS successCount,
                 COALESCE(a.total_count, 0) AS totalCount,
                 COALESCE(a.last_used_at, 0) AS lastUsedAt
          FROM documents d
          LEFT JOIN action_stats a ON a.action_id = d.id
          WHERE d.category <> 'files'
          ORDER BY
            CASE WHEN COALESCE(a.last_used_at, 0) > 0 THEN 0 ELSE 1 END ASC,
            COALESCE(a.last_used_at, 0) DESC,
            COALESCE(a.frequency, 0) DESC,
            d.updated_at DESC
          LIMIT ?
        `
    ).all(limit);
  }
  recordAction(actionId, success) {
    const now = Date.now();
    this.db.prepare(
      `
          INSERT INTO action_stats (action_id, frequency, success_count, total_count, last_used_at)
          VALUES (?, 1, ?, 1, ?)
          ON CONFLICT(action_id) DO UPDATE SET
            frequency = action_stats.frequency + 1,
            success_count = action_stats.success_count + excluded.success_count,
            total_count = action_stats.total_count + 1,
            last_used_at = excluded.last_used_at
        `
    ).run(actionId, success ? 1 : 0, now);
  }
  recordClick(query, resultId, rank, success) {
    this.db.prepare(
      "INSERT INTO click_events (created_at, query, result_id, rank, success) VALUES (?, ?, ?, ?, ?)"
    ).run(Date.now(), query, resultId, rank, success ? 1 : 0);
  }
  readRecentClickAverage(limit = 200) {
    const rows = this.db.prepare("SELECT rank FROM click_events ORDER BY id DESC LIMIT ?").all(limit);
    if (rows.length === 0) return 0;
    const sum = rows.reduce((acc, row) => acc + row.rank, 0);
    return sum / rows.length;
  }
  writeBenchmarkSnapshot(precisionAt5, precisionAt10, benchmarkSize) {
    this.db.prepare(
      "INSERT INTO benchmark_snapshots (created_at, precision_at_5, precision_at_10, avg_click_rank, benchmark_size) VALUES (?, ?, ?, ?, ?)"
    ).run(Date.now(), precisionAt5, precisionAt10, this.readRecentClickAverage(), benchmarkSize);
  }
  readBenchmarkHistory(limit = 40) {
    return this.db.prepare(
      `SELECT created_at AS createdAt,
                precision_at_5 AS precisionAt5,
                precision_at_10 AS precisionAt10,
                avg_click_rank AS avgClickRank
        FROM benchmark_snapshots
        ORDER BY id DESC
        LIMIT ?
      `
    ).all(limit);
  }
  // Session cache for search results
  _searchCache = /* @__PURE__ */ new Map();
  _cacheTimestamp = /* @__PURE__ */ new Map();
  CACHE_TTL = 5 * 60 * 1e3;
  // 5 minutes
  getSearch(query, limit) {
    const now = Date.now();
    const cacheKey2 = `${query}:${limit}`;
    const lastUpdate = this._cacheTimestamp.get(cacheKey2);
    if (lastUpdate && now - lastUpdate < this.CACHE_TTL) {
      return this._searchCache.get(cacheKey2) || [];
    }
    const results = this.search(query, limit);
    this._searchCache.set(cacheKey2, results);
    this._cacheTimestamp.set(cacheKey2, now);
    return results;
  }
  clearSearchCache() {
    this._searchCache.clear();
    this._cacheTimestamp.clear();
  }
};

// src/main/search/providers/appsProvider.ts
var import_node_fs15 = require("node:fs");
var import_node_os8 = require("node:os");
var import_node_path15 = require("node:path");
function listApplications() {
  const roots = [
    "/Applications",
    "/Applications/Utilities",
    "/System/Applications",
    "/System/Applications/Utilities",
    "/System/Library/CoreServices/Applications",
    "/System/Library/CoreServices",
    (0, import_node_path15.join)((0, import_node_os8.homedir)(), "Applications")
  ];
  const out = [];
  const seen = /* @__PURE__ */ new Set();
  for (const root of roots) {
    try {
      for (const entry of (0, import_node_fs15.readdirSync)(root)) {
        if (!entry.endsWith(".app")) continue;
        const name = entry.replace(/\.app$/, "");
        if (seen.has(name)) continue;
        seen.add(name);
        out.push({
          name,
          path: (0, import_node_path15.join)(root, entry)
        });
      }
    } catch {
    }
  }
  return out;
}
var appsProvider = {
  providerId: "apps",
  async buildDocuments() {
    const now = Date.now();
    return listApplications().map((app2) => ({
      id: `app:${app2.path}`,
      category: "applications",
      title: app2.name,
      subtitle: app2.path,
      tokens: `${app2.name} ${app2.path}`,
      action: { type: "open-app", appName: app2.name },
      updatedAt: now,
      sourcePath: app2.path
    }));
  }
};

// src/main/search/providers/clipboardProvider.ts
init_electron_shim();
var import_node_crypto6 = require("node:crypto");
var import_node_fs16 = require("node:fs");
var import_node_path16 = require("node:path");
init_configStore();
var CLIPBOARD_LIMIT = 200;
var CLIPBOARD_WATCH_ENABLED_KEY = "clipboardWatchEnabled";
var CLIPBOARD_CAPTURE_IMAGES_KEY = "clipboardCaptureImages";
var CLIPBOARD_MAX_IMAGE_MEGAPIXELS_KEY = "clipboardMaxImageMegapixels";
var DEFAULT_CLIPBOARD_WATCH_ENABLED = true;
var DEFAULT_CLIPBOARD_CAPTURE_IMAGES = false;
var DEFAULT_CLIPBOARD_MAX_IMAGE_MEGAPIXELS = 2;
function getClipboardConfig() {
  const raw = readRawConfig();
  const watchEnabled = raw[CLIPBOARD_WATCH_ENABLED_KEY] ?? DEFAULT_CLIPBOARD_WATCH_ENABLED;
  const captureImages = raw[CLIPBOARD_CAPTURE_IMAGES_KEY] ?? DEFAULT_CLIPBOARD_CAPTURE_IMAGES;
  const maxImageMegapixels = Number(raw[CLIPBOARD_MAX_IMAGE_MEGAPIXELS_KEY] ?? DEFAULT_CLIPBOARD_MAX_IMAGE_MEGAPIXELS);
  return {
    watchEnabled: watchEnabled !== false,
    captureImages: captureImages === true,
    maxImageMegapixels: Number.isFinite(maxImageMegapixels) && maxImageMegapixels > 0 ? maxImageMegapixels : DEFAULT_CLIPBOARD_MAX_IMAGE_MEGAPIXELS
  };
}
function setClipboardConfig(patch) {
  const next = {};
  if (typeof patch.watchEnabled === "boolean") next[CLIPBOARD_WATCH_ENABLED_KEY] = patch.watchEnabled;
  if (typeof patch.captureImages === "boolean") next[CLIPBOARD_CAPTURE_IMAGES_KEY] = patch.captureImages;
  if (typeof patch.maxImageMegapixels === "number" && Number.isFinite(patch.maxImageMegapixels) && patch.maxImageMegapixels > 0) {
    next[CLIPBOARD_MAX_IMAGE_MEGAPIXELS_KEY] = patch.maxImageMegapixels;
  }
  writeConfigPatch(next);
}
function storeDir() {
  const dir = (0, import_node_path16.join)(app.getPath("userData"), "search");
  (0, import_node_fs16.mkdirSync)(dir, { recursive: true });
  return dir;
}
function imagesDir() {
  const dir = (0, import_node_path16.join)(storeDir(), "clipboard-images");
  (0, import_node_fs16.mkdirSync)(dir, { recursive: true });
  return dir;
}
function clipboardPath() {
  return (0, import_node_path16.join)(storeDir(), "clipboard.json");
}
var _readClipboardDb = null;
var _cacheTimestamp = 0;
var CACHE_TTL = 10 * 1e3;
async function ensureDbLoaded() {
  if (_readClipboardDb && Date.now() - _cacheTimestamp < CACHE_TTL) {
    return;
  }
  try {
    const raw = (0, import_node_fs16.readFileSync)(clipboardPath(), "utf8");
    const parsed = JSON.parse(raw);
    _readClipboardDb = {
      items: Array.isArray(parsed.items) ? parsed.items : []
    };
  } catch {
    _readClipboardDb = { items: [] };
  }
  _cacheTimestamp = Date.now();
}
function writeDb3(db) {
  (0, import_node_fs16.writeFileSync)(clipboardPath(), `${JSON.stringify(db, null, 2)}
`, "utf8");
  _readClipboardDb = db;
  _cacheTimestamp = Date.now();
}
function detectSensitiveValue(text) {
  const trimmed = text.trim();
  if (trimmed.length < 16) return false;
  if (/^sk-[A-Za-z0-9]{16,}$/.test(trimmed)) return true;
  if (/^gh[pousr]_[A-Za-z0-9_]{20,}$/.test(trimmed)) return true;
  if (/password\s*[=:]/i.test(trimmed)) return true;
  if (/api[_-]?key\s*[=:]/i.test(trimmed)) return true;
  if (/token\s*[=:]/i.test(trimmed)) return true;
  return false;
}
function sanitizeEntry(entry) {
  const base = {
    id: String(entry.id ?? ""),
    createdAt: Number(entry.createdAt ?? 0),
    pinned: Boolean(entry.pinned),
    isSecret: Boolean(entry.isSecret)
  };
  if (!base.id || !Number.isFinite(base.createdAt)) return null;
  switch (entry.kind) {
    case "text": {
      const text = String(entry.text ?? "");
      if (!text) return null;
      return {
        ...base,
        kind: "text",
        text,
        preview: String(entry.preview ?? previewFromText(text)),
        charCount: Number(entry.charCount ?? text.length),
        lineCount: Number(entry.lineCount ?? text.split("\n").length)
      };
    }
    case "image": {
      const imagePath = String(entry.imagePath ?? "");
      if (!imagePath || !(0, import_node_fs16.existsSync)(imagePath)) return null;
      return {
        ...base,
        kind: "image",
        imagePath,
        width: Number(entry.width ?? 0),
        height: Number(entry.height ?? 0),
        byteSize: Number(entry.byteSize ?? 0)
      };
    }
    case "file": {
      const paths = Array.isArray(entry.paths) ? entry.paths.map((p) => String(p)).filter(Boolean) : [];
      if (paths.length === 0) return null;
      return {
        ...base,
        kind: "file",
        paths,
        preview: paths.length === 1 ? (0, import_node_path16.basename)(paths[0]) : `${(0, import_node_path16.basename)(paths[0])} + ${paths.length - 1} more`
      };
    }
    default:
      return null;
  }
}
function normalizeDb(db) {
  return {
    items: db.items.map((item) => sanitizeEntry(item)).filter((item) => item !== null)
  };
}
function previewFromText(text) {
  const firstLine = text.split("\n").find((line) => line.trim().length > 0) ?? "";
  return firstLine.slice(0, 140);
}
function insertEntry(db, entry) {
  const pinned = db.items.filter((item) => item.pinned && item.id !== entry.id);
  const rest = db.items.filter((item) => !item.pinned && item.id !== entry.id);
  return { items: [...pinned, entry, ...rest].slice(0, CLIPBOARD_LIMIT) };
}
function hashKey(kind, payload) {
  return (0, import_node_crypto6.createHash)("sha1").update(`${kind}|${payload}`).digest("hex").slice(0, 16);
}
function readFileUrls() {
  if (process.platform !== "darwin") return [];
  const formats = clipboard.availableFormats();
  const hasFileUrl = formats.some(
    (f) => f === "public.file-url" || f === "NSFilenamesPboardType" || f === "Files"
  );
  if (!hasFileUrl) return [];
  try {
    const raw = clipboard.read("public.file-url");
    if (!raw) return [];
    const parts = raw.split(/\0|\r?\n/g).map((part) => part.trim()).filter(Boolean);
    const paths = parts.map((url) => {
      try {
        if (url.startsWith("file://")) {
          return decodeURIComponent(new URL(url).pathname);
        }
        return url;
      } catch {
        return "";
      }
    }).filter(Boolean);
    return Array.from(new Set(paths));
  } catch {
    return [];
  }
}
function idForText(text) {
  return `text:${hashKey("text", text).slice(0, 12)}`;
}
function idForFiles(paths) {
  return `file:${hashKey("file", paths.slice().sort().join("|")).slice(0, 12)}`;
}
function idForImage(hash) {
  return `image:${hash.slice(0, 12)}`;
}
function captureFileEntry(paths, now) {
  if (paths.length === 0) return null;
  return {
    id: idForFiles(paths),
    kind: "file",
    createdAt: now,
    pinned: false,
    isSecret: false,
    paths,
    preview: paths.length === 1 ? (0, import_node_path16.basename)(paths[0]) : `${(0, import_node_path16.basename)(paths[0])} + ${paths.length - 1} more`
  };
}
function resizeToMegapixels(image, maxMegapixels) {
  if (maxMegapixels <= 0) return image;
  const { width, height } = image.getSize();
  const megapixels = width * height / 1e6;
  if (megapixels <= maxMegapixels) return image;
  const scale = Math.sqrt(maxMegapixels / megapixels);
  const newWidth = Math.max(1, Math.round(width * scale));
  const newHeight = Math.max(1, Math.round(height * scale));
  return image.resize({ width: newWidth, height: newHeight, quality: "good" });
}
function captureImageEntry(now) {
  const config = getClipboardConfig();
  if (!config.captureImages) return null;
  const image = clipboard.readImage();
  if (image.isEmpty()) return null;
  const resized = resizeToMegapixels(image, config.maxImageMegapixels);
  const buffer = resized.toPNG();
  if (buffer.length === 0) return null;
  const hash = (0, import_node_crypto6.createHash)("sha1").update(buffer).digest("hex");
  const id = idForImage(hash);
  const file = (0, import_node_path16.join)(imagesDir(), `${hash}.png`);
  if (!(0, import_node_fs16.existsSync)(file)) {
    (0, import_node_fs16.writeFileSync)(file, buffer);
  }
  return {
    id,
    kind: "image",
    createdAt: now,
    pinned: false,
    isSecret: false,
    imagePath: file,
    width: resized.getSize().width,
    height: resized.getSize().height,
    byteSize: buffer.length
  };
}
function captureTextEntry(now) {
  const text = clipboard.readText();
  if (!text || !text.trim()) return null;
  return {
    id: idForText(text),
    kind: "text",
    createdAt: now,
    pinned: false,
    isSecret: detectSensitiveValue(text),
    text,
    preview: previewFromText(text),
    charCount: text.length,
    lineCount: text.split("\n").length
  };
}
function captureClipboardSnapshot() {
  const now = Date.now();
  ensureDbLoaded();
  if (!_readClipboardDb) return;
  const fileUrls = readFileUrls();
  const candidate = captureFileEntry(fileUrls, now) ?? captureImageEntry(now) ?? captureTextEntry(now);
  if (!candidate) return;
  const existing = _readClipboardDb.items.find((item) => item.id === candidate.id);
  const merged = existing ? {
    ...candidate,
    pinned: existing.pinned,
    createdAt: now
  } : candidate;
  if (_readClipboardDb.items[0]?.id === candidate.id && !existing?.pinned) {
    return;
  }
  const db = normalizeDb(insertEntry(_readClipboardDb, merged));
  writeDb3(db);
}
function listClipboardEntries() {
  return normalizeDb(_readClipboardDb || { items: [] }).items;
}
function getClipboardEntry(id) {
  return listClipboardEntries().find((item) => item.id === id) ?? null;
}
function deleteClipboardEntry(id) {
  const db = normalizeDb(_readClipboardDb || { items: [] });
  const entry = db.items.find((item) => item.id === id);
  if (!entry) return false;
  const next = db.items.filter((item) => item.id !== id);
  writeDb3({ items: next });
  if (entry.kind === "image") {
    const stillReferenced = next.some(
      (item) => item.kind === "image" && item.imagePath === entry.imagePath
    );
    if (!stillReferenced && (0, import_node_fs16.existsSync)(entry.imagePath)) {
      try {
        (0, import_node_fs16.rmSync)(entry.imagePath, { force: true });
      } catch {
      }
    }
  }
  return true;
}
function togglePinClipboardEntry(id) {
  const db = normalizeDb(_readClipboardDb || { items: [] });
  const entry = db.items.find((item) => item.id === id);
  if (!entry) return false;
  entry.pinned = !entry.pinned;
  const pinned = db.items.filter((item) => item.pinned);
  const rest = db.items.filter((item) => !item.pinned);
  writeDb3({ items: [...pinned, ...rest] });
  return true;
}
function clearClipboardHistory() {
  const db = normalizeDb(_readClipboardDb || { items: [] });
  for (const item of db.items) {
    if (item.kind === "image" && (0, import_node_fs16.existsSync)(item.imagePath)) {
      try {
        (0, import_node_fs16.rmSync)(item.imagePath, { force: true });
      } catch {
      }
    }
  }
  writeDb3({ items: [] });
}
async function cleanupOrphanClipboardImages() {
  await ensureDbLoaded();
  const dir = imagesDir();
  const db = normalizeDb(_readClipboardDb || { items: [] });
  const referenced = new Set(
    db.items.filter((item) => item.kind === "image").map((item) => item.imagePath)
  );
  let removed = 0;
  let freedBytes = 0;
  for (const entry of (0, import_node_fs16.readdirSync)(dir, { withFileTypes: true })) {
    if (!entry.isFile()) continue;
    const ext = (0, import_node_path16.extname)(entry.name).toLowerCase();
    if (ext !== ".png") continue;
    const fullPath = (0, import_node_path16.join)(dir, entry.name);
    if (referenced.has(fullPath)) continue;
    try {
      const stats = (0, import_node_fs16.statSync)(fullPath);
      (0, import_node_fs16.rmSync)(fullPath, { force: true });
      removed += 1;
      freedBytes += stats.size;
    } catch {
    }
  }
  return { removed, freedBytes };
}
async function clearClipboardImageHistory() {
  await ensureDbLoaded();
  const db = normalizeDb(_readClipboardDb || { items: [] });
  const imageEntries = db.items.filter((item) => item.kind === "image");
  const remaining = db.items.filter((item) => item.kind !== "image");
  writeDb3({ items: remaining });
  const cleanup2 = await cleanupOrphanClipboardImages();
  return {
    removed: imageEntries.length,
    freedBytes: cleanup2.freedBytes
  };
}
function getClipboardImagesDir() {
  return imagesDir();
}
function getClipboardStoreDir() {
  return storeDir();
}
function restoreClipboardEntry(id) {
  const entry = getClipboardEntry(id);
  if (!entry) return false;
  switch (entry.kind) {
    case "text":
      clipboard.writeText(entry.text);
      return true;
    case "image": {
      if (!(0, import_node_fs16.existsSync)(entry.imagePath)) return false;
      const img = nativeImage.createFromPath(entry.imagePath);
      if (img.isEmpty()) return false;
      clipboard.writeImage(img);
      return true;
    }
    case "file": {
      if (process.platform === "darwin") {
        const url = `file://${encodeURI(entry.paths[0])}`;
        clipboard.write({ text: entry.paths.join("\n"), bookmark: url });
      } else {
        clipboard.writeText(entry.paths.join("\n"));
      }
      return true;
    }
    default:
      return false;
  }
}
function revealClipboardEntryInFinder(id) {
  const entry = getClipboardEntry(id);
  if (!entry) return false;
  if (entry.kind === "image") {
    shell.showItemInFolder(entry.imagePath);
    return true;
  }
  if (entry.kind === "file" && entry.paths[0]) {
    shell.showItemInFolder(entry.paths[0]);
    return true;
  }
  return false;
}
function readClipboardImagePayload(id) {
  const entry = getClipboardEntry(id);
  if (!entry || entry.kind !== "image") return null;
  if (!(0, import_node_fs16.existsSync)(entry.imagePath)) return null;
  const bytes = (0, import_node_fs16.readFileSync)(entry.imagePath);
  return {
    dataUrl: `data:image/png;base64,${bytes.toString("base64")}`,
    width: entry.width,
    height: entry.height,
    byteSize: entry.byteSize
  };
}
var watcherHandle = null;
var watcherInactiveTicks = 0;
var WATCHER_DEFAULT_INTERVAL_MS = 750;
var WATCHER_IDLE_INTERVAL_MS = 2e3;
var WATCHER_IDLE_THRESHOLD_TICKS = 60;
function startClipboardWatcher(intervalMs = WATCHER_DEFAULT_INTERVAL_MS) {
  if (watcherHandle) return;
  const config = getClipboardConfig();
  if (!config.watchEnabled) return;
  void cleanupOrphanClipboardImages().catch(() => {
  });
  let lastTopId = "";
  watcherHandle = setInterval(() => {
    try {
      captureClipboardSnapshot();
      const db = _readClipboardDb;
      const topId = db?.items[0]?.id ?? "";
      if (topId === lastTopId) {
        watcherInactiveTicks += 1;
      } else {
        watcherInactiveTicks = 0;
        lastTopId = topId;
      }
      if (watcherInactiveTicks > WATCHER_IDLE_THRESHOLD_TICKS && watcherHandle && intervalMs < WATCHER_IDLE_INTERVAL_MS) {
        clearInterval(watcherHandle);
        watcherHandle = null;
        startClipboardWatcher(WATCHER_IDLE_INTERVAL_MS);
      }
    } catch {
    }
  }, intervalMs);
  if (typeof watcherHandle.unref === "function") {
    ;
    watcherHandle.unref();
  }
}
function stopClipboardWatcher() {
  if (!watcherHandle) return;
  clearInterval(watcherHandle);
  watcherHandle = null;
  watcherInactiveTicks = 0;
}
function restartClipboardWatcher() {
  stopClipboardWatcher();
  startClipboardWatcher();
}
var clipboardProvider = {
  providerId: "clipboard",
  async buildDocuments() {
    return [];
  }
};

// src/main/search/providers/commandsProvider.ts
function buildNativeCommandDocuments() {
  const now = Date.now();
  return listNativeCommands().filter((descriptor) => descriptor.id !== "list-listening-ports").map((descriptor) => ({
    id: `native:${descriptor.id}`,
    category: "native-command",
    title: descriptor.title,
    subtitle: descriptor.subtitle,
    tokens: [descriptor.title, descriptor.subtitle, descriptor.category, ...descriptor.keywords].join(" "),
    action: { type: "run-native-command", commandId: descriptor.id },
    updatedAt: now
  }));
}
function buildRaymesSurfaceDocuments() {
  const now = Date.now();
  return [
    {
      id: "command:open-settings",
      title: "Open Settings",
      subtitle: "Tezbar settings",
      keywords: ["settings", "preferences", "/settings"],
      commandId: "open-settings"
    },
    {
      id: "command:open-extensions",
      title: "Open Extensions",
      subtitle: "Tezbar extensions",
      keywords: ["extensions", "raycast", "/extensions"],
      commandId: "open-extensions"
    },
    {
      id: "command:open-snippets",
      title: "Open Snippets",
      subtitle: "Tezbar snippets",
      keywords: ["snippets", "text snippets", "/snippets"],
      commandId: "open-snippets"
    },
    {
      id: "command:open-notes",
      title: "Open Notes",
      subtitle: "Tezbar quick notes",
      keywords: ["notes", "quick notes", "/notes"],
      commandId: "open-notes"
    },
    {
      id: "command:open-emoji-picker",
      title: "Open Emoji Picker",
      subtitle: "Tezbar emoji picker",
      keywords: ["emoji", "symbols", "/emoji"],
      commandId: "open-emoji-picker"
    }
  ].map((item) => ({
    id: item.id,
    category: "commands",
    title: item.title,
    subtitle: item.subtitle,
    tokens: [item.title, item.subtitle, ...item.keywords].join(" "),
    action: { type: "invoke-command", commandId: item.commandId },
    updatedAt: now
  }));
}
var commandsProvider = {
  providerId: "commands",
  async buildDocuments() {
    return [...buildRaymesSurfaceDocuments(), ...buildNativeCommandDocuments()];
  }
};

// src/main/search/providers/extensionsProvider.ts
init_extension_registry();
var extensionsProvider = {
  providerId: "extensions",
  async buildDocuments() {
    const installed = listInstalledRegistryExtensions();
    if (installed.length === 0) return [];
    const out = [];
    for (const ext of installed.slice(0, 100)) {
      for (const cmd of ext.commands) {
        out.push({
          id: `extcmd:${ext.id}:${cmd.name}`,
          category: "extensions",
          title: cmd.title,
          subtitle: ext.name,
          tokens: `${cmd.title} ${cmd.name} ${ext.name} ${ext.slug} ${ext.id} ${ext.description || ""}`,
          action: {
            type: "run-extension-command",
            extensionId: ext.id,
            commandName: cmd.name,
            title: cmd.title,
            iconPath: ext.iconPath,
            commandArgumentDefinitions: cmd.argumentDefinitions
          },
          updatedAt: ext.installedAt,
          popularity: ext.downloadCount || 0
        });
      }
    }
    return out;
  }
};

// src/main/search/providers/filesProvider.ts
var import_node_child_process11 = require("node:child_process");
var import_node_fs17 = require("node:fs");
var import_node_os9 = require("node:os");
var import_node_path17 = require("node:path");
var import_node_util10 = require("node:util");
var execFileAsync10 = (0, import_node_util10.promisify)(import_node_child_process11.execFile);
var ALLOWED_EXTENSIONS = /* @__PURE__ */ new Set([
  "",
  ".md",
  ".txt",
  ".json",
  ".yaml",
  ".yml",
  ".ts",
  ".tsx",
  ".js",
  ".jsx",
  ".go",
  ".py",
  ".rs",
  ".swift",
  ".pdf",
  ".png",
  ".jpg"
]);
var SKIP_NAMES = /* @__PURE__ */ new Set([
  "node_modules",
  ".git",
  ".next",
  ".cache",
  "Library",
  "build",
  "coverage",
  "dist",
  "out",
  "target"
]);
function isAllowedFile(path7) {
  const ext = (0, import_node_path17.extname)(path7).toLowerCase();
  return ALLOWED_EXTENSIONS.has(ext);
}
function containsSkippedDirectory(path7) {
  return path7.split(import_node_path17.sep).some((part) => SKIP_NAMES.has(part));
}
function makeFileDocument(path7) {
  try {
    const stat2 = (0, import_node_fs17.statSync)(path7);
    if (!stat2.isFile()) return null;
    if (!isAllowedFile(path7)) return null;
    const title = path7.split("/").pop() ?? path7;
    return {
      id: `file:${path7}`,
      category: "files",
      title,
      subtitle: path7,
      tokens: `${title} ${path7}`,
      action: { type: "open-file", path: path7 },
      updatedAt: stat2.mtimeMs,
      sourcePath: path7,
      sourceMtime: stat2.mtimeMs
    };
  } catch {
    return null;
  }
}
function initialRoots() {
  const home = (0, import_node_os9.homedir)();
  return [(0, import_node_path17.join)(home, "Desktop"), (0, import_node_path17.join)(home, "Documents"), (0, import_node_path17.join)(home, "Downloads")].filter((root) => (0, import_node_fs17.existsSync)(root));
}
async function collectInitialFileDocuments(limit = 4e3) {
  const roots = initialRoots();
  if (roots.length === 0) return [];
  const out = [];
  const queue = [...roots];
  let visitedEntries = 0;
  while (queue.length > 0 && out.length < limit) {
    const current = queue.shift();
    if (!current) continue;
    try {
      const entries = (0, import_node_fs17.readdirSync)(current, { withFileTypes: true });
      for (const entry of entries) {
        if (out.length >= limit) break;
        visitedEntries += 1;
        if (visitedEntries % 250 === 0) {
          await new Promise((resolve4) => setImmediate(resolve4));
        }
        const absolute = (0, import_node_path17.join)(current, entry.name);
        if (entry.isDirectory()) {
          if (!SKIP_NAMES.has(entry.name)) queue.push(absolute);
          continue;
        }
        const doc = makeFileDocument(absolute);
        if (doc) out.push(doc);
      }
    } catch {
    }
  }
  return out;
}
function startFileWatcher(listener) {
  const roots = initialRoots();
  const unsubs = [];
  const pendingUpserts = /* @__PURE__ */ new Map();
  const pendingRemovals = /* @__PURE__ */ new Set();
  let flushTimer = null;
  const flush = () => {
    flushTimer = null;
    if (pendingUpserts.size === 0 && pendingRemovals.size === 0) return;
    listener({
      upserts: Array.from(pendingUpserts.values()),
      removeIds: Array.from(pendingRemovals)
    });
    pendingUpserts.clear();
    pendingRemovals.clear();
  };
  const scheduleFlush = () => {
    if (flushTimer) return;
    flushTimer = setTimeout(flush, 200);
    flushTimer.unref();
  };
  for (const root of roots) {
    try {
      const watcher = (0, import_node_fs17.watch)(root, { recursive: true }, (_event, filename) => {
        if (!filename) return;
        const relative = filename.toString();
        if (containsSkippedDirectory(relative)) return;
        const absolute = (0, import_node_path17.join)(root, relative);
        const doc = makeFileDocument(absolute);
        if (doc) {
          pendingRemovals.delete(doc.id);
          pendingUpserts.set(doc.id, doc);
          scheduleFlush();
          return;
        }
        if (!(0, import_node_fs17.existsSync)(absolute)) {
          const id = `file:${absolute}`;
          pendingUpserts.delete(id);
          pendingRemovals.add(id);
          scheduleFlush();
        }
      });
      unsubs.push(() => watcher.close());
    } catch {
    }
  }
  return () => {
    if (flushTimer) clearTimeout(flushTimer);
    flush();
    for (const stop of unsubs) stop();
  };
}
async function spotlightFallback(query, limit = 8) {
  const trimmed = query.trim();
  if (!trimmed) return [];
  const INTERNAL_PATH_PATTERNS = [
    "/extension-registry/",
    "/extensions/packages/",
    "/HTTPStorages/",
    "/Application Support/tezbar/"
  ];
  try {
    const { stdout } = await execFileAsync10("mdfind", ["-name", trimmed, "-onlyin", (0, import_node_os9.homedir)()]);
    return stdout.split("\n").map((line) => line.trim()).filter(Boolean).filter((path7) => !INTERNAL_PATH_PATTERNS.some((pattern) => path7.includes(pattern))).slice(0, limit).map((path7, index) => ({
      id: `spotlight:${path7}`,
      title: path7.split("/").pop() ?? path7,
      subtitle: path7,
      category: "files",
      score: 150 - index,
      action: { type: "open-file", path: path7 }
    }));
  } catch {
    return [];
  }
}

// src/main/search/providers/notesProvider.ts
init_electron_shim();
var import_node_fs18 = require("node:fs");
var import_node_path18 = require("node:path");
var NOTES_LIMIT = 250;
function stripMarkdownSyntax(text) {
  return text.replace(/\*\*([^*\n]+)\*\*/g, "$1").replace(/__([^_\n]+)__/g, "$1").replace(/\*([^*\n]+)\*/g, "$1").replace(/_([^_\n]+)_/g, "$1").replace(/`([^`\n]+)`/g, "$1").replace(/\[([^\]]+)\]\([^)]+\)/g, "$1").replace(/^#{1,6}\s+/gm, "").replace(/^\s*[-*+]\s+/gm, "").replace(/^\s*\d+\.\s+/gm, "");
}
function decodeBasicEntities(text) {
  return text.replace(/&nbsp;/gi, " ").replace(/&amp;/gi, "&").replace(/&lt;/gi, "<").replace(/&gt;/gi, ">").replace(/&quot;/gi, '"').replace(/&#039;/gi, "'");
}
function notePlainText(text) {
  const withoutHtml = text.replace(/<br\s*\/?\s*>/gi, "\n").replace(/<\/(div|p|li|h[1-6])>/gi, "\n").replace(/<li>/gi, "- ").replace(/<[^>]+>/g, "");
  return stripMarkdownSyntax(decodeBasicEntities(withoutHtml)).replace(/\r/g, "").replace(/\u00a0/g, " ").replace(/[ \t]+\n/g, "\n").replace(/\n{3,}/g, "\n\n").trim();
}
function notesPath() {
  const dir = (0, import_node_path18.join)(app.getPath("userData"), "search");
  (0, import_node_fs18.mkdirSync)(dir, { recursive: true });
  return (0, import_node_path18.join)(dir, "notes.json");
}
function migrateNote(raw) {
  if (!raw || typeof raw !== "object") return null;
  const o = raw;
  if (typeof o.text !== "string" || typeof o.createdAt !== "number") return null;
  const updatedAt = typeof o.updatedAt === "number" ? o.updatedAt : o.createdAt;
  return {
    text: o.text,
    createdAt: o.createdAt,
    updatedAt
  };
}
function readNotesDb() {
  try {
    const raw = (0, import_node_fs18.readFileSync)(notesPath(), "utf8");
    const parsed = JSON.parse(raw);
    const notes = [];
    if (Array.isArray(parsed.notes)) {
      for (const row of parsed.notes) {
        const m = migrateNote(row);
        if (m) notes.push(m);
      }
    }
    return { notes };
  } catch {
    return { notes: [] };
  }
}
function writeNotesDb(db) {
  (0, import_node_fs18.writeFileSync)(notesPath(), `${JSON.stringify(db, null, 2)}
`, "utf8");
}
function listQuickNotes() {
  return readNotesDb().notes;
}
function addQuickNote(text) {
  const trimmed = text.trim();
  if (!trimmed) return null;
  const now = Date.now();
  const entry = { text: trimmed, createdAt: now, updatedAt: now };
  const db = readNotesDb();
  db.notes = [entry, ...db.notes].slice(0, NOTES_LIMIT);
  writeNotesDb(db);
  return entry;
}
function updateQuickNote(createdAt, text) {
  const db = readNotesDb();
  const idx = db.notes.findIndex((n) => n.createdAt === createdAt);
  if (idx < 0) return false;
  db.notes[idx] = {
    ...db.notes[idx],
    text,
    updatedAt: Date.now()
  };
  writeNotesDb(db);
  return true;
}
function deleteQuickNote(createdAt) {
  const db = readNotesDb();
  const next = db.notes.filter((n) => n.createdAt !== createdAt);
  if (next.length === db.notes.length) return false;
  db.notes = next;
  writeNotesDb(db);
  return true;
}
var notesProvider = {
  providerId: "notes",
  async buildDocuments() {
    return listQuickNotes().map((note) => {
      const plain = notePlainText(note.text);
      return {
        id: `note:${note.createdAt}`,
        category: "quick-notes",
        title: plain.split("\n")[0]?.trim().slice(0, 100) || "(note)",
        subtitle: "Quick note",
        tokens: plain,
        action: { type: "copy-text", text: plain },
        updatedAt: note.updatedAt
      };
    });
  }
};

// src/main/search/providers/quickLinksProvider.ts
init_electron_shim();
var import_node_fs19 = require("node:fs");
var import_node_path19 = require("node:path");
function quickLinksPath() {
  const dir = (0, import_node_path19.join)(app.getPath("userData"), "search");
  (0, import_node_fs19.mkdirSync)(dir, { recursive: true });
  return (0, import_node_path19.join)(dir, "quick-links.json");
}
function readQuickLinksDb() {
  try {
    const raw = (0, import_node_fs19.readFileSync)(quickLinksPath(), "utf8");
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed.links)) return { links: [] };
    return { links: parsed.links };
  } catch {
    const db = {
      links: [
        {
          id: "ql:google",
          name: "Google Search",
          template: "https://www.google.com/search?q={query}",
          createdAt: Date.now()
        }
      ]
    };
    (0, import_node_fs19.writeFileSync)(quickLinksPath(), `${JSON.stringify(db, null, 2)}
`, "utf8");
    return db;
  }
}
function fillTemplate(template, query) {
  const q = encodeURIComponent(query.trim());
  return template.split("{query}").join(q);
}
var quickLinksProvider = {
  providerId: "quick-links",
  async buildDocuments() {
    return readQuickLinksDb().links.map((link) => ({
      id: link.id,
      category: "quick-links",
      title: link.name,
      subtitle: link.profile ? `Quick link (${link.profile})` : "Quick link",
      tokens: `${link.name} ${link.template}`,
      action: { type: "open-url", url: fillTemplate(link.template, "") },
      updatedAt: link.createdAt
    }));
  }
};

// src/main/search/providers/snippetsProvider.ts
init_electron_shim();
var import_node_crypto7 = require("node:crypto");
var import_node_fs20 = require("node:fs");
var import_node_os10 = require("node:os");
var import_node_path20 = require("node:path");
function snippetsPath() {
  const dir = (0, import_node_path20.join)(app.getPath("userData"), "search");
  (0, import_node_fs20.mkdirSync)(dir, { recursive: true });
  return (0, import_node_path20.join)(dir, "snippets.json");
}
function defaultSnippets() {
  const t = Date.now();
  let i = 0;
  const next = () => t - i++;
  return [
    {
      id: "snippet:today",
      label: "Get today's date",
      trigger: "today",
      body: "Today is ${date}.",
      createdAt: next()
    },
    {
      id: "snippet:time",
      label: "Get current time",
      trigger: "time",
      body: "Current time: ${time}",
      createdAt: next()
    },
    {
      id: "snippet:datetime",
      label: "Get date and time",
      trigger: "datetime",
      body: "${datetime}",
      createdAt: next()
    },
    {
      id: "snippet:iso",
      label: "Insert ISO 8601 timestamp",
      trigger: "iso",
      body: "${iso}",
      createdAt: next()
    },
    {
      id: "snippet:year",
      label: "Insert current year",
      trigger: "year",
      body: "${year}",
      createdAt: next()
    },
    {
      id: "snippet:epoch",
      label: "Insert Unix timestamp (seconds)",
      trigger: "epoch",
      body: "${timestamp}",
      createdAt: next()
    },
    {
      id: "snippet:uuid",
      label: "Insert random UUID",
      trigger: "uuid",
      body: "${uuid}",
      createdAt: next()
    },
    {
      id: "snippet:hostname",
      label: "Insert this computer\u2019s hostname",
      trigger: "hostname",
      body: "${hostname}",
      createdAt: next()
    },
    {
      id: "snippet:public-ip",
      label: "Show public IP (Terminal command)",
      trigger: "myip",
      body: "# Prints your public IPv4 \u2014 paste into Terminal and press Enter\ncurl -4s https://api.ipify.org\n\n# Alternative (IPv4 or IPv6 depending on your network)\n# curl -s https://ifconfig.me\n",
      createdAt: next()
    },
    {
      id: "snippet:local-ip",
      label: "Show local IP on macOS (Terminal)",
      trigger: "localip",
      body: '# Wi\u2011Fi (usually en0 on MacBooks)\nipconfig getifaddr en0\n\n# If empty, try Ethernet or other interface\n# ipconfig getifaddr en1\n\n# List all IPv4 addresses on the machine\n# ifconfig | grep "inet "\n',
      createdAt: next()
    },
    {
      id: "snippet:signed",
      label: "Email sign-off (professional)",
      trigger: "signed",
      body: "Thank you for your time and for looking into this.\n\nIf anything is unclear or you would like more detail, please let me know \u2014 I am happy to jump on a quick call or thread.\n\nBest regards,\n\n[Your name]\n[Role / team \u2014 optional]\n\n\u2014\n[Optional: direct line \xB7 Slack @handle \xB7 calendar link]\n",
      createdAt: next()
    },
    {
      id: "snippet:thanks",
      label: "Short thank-you (chat / email)",
      trigger: "thanks",
      body: "Thanks a lot \u2014 I really appreciate the quick help on this.\n\n[Your name]\n",
      createdAt: next()
    },
    {
      id: "snippet:commit",
      label: "Conventional commit message (full template)",
      trigger: "commit",
      body: "feat(your-scope): short imperative summary (aim for ~50\u201372 chars)\n\nExplain why this change exists and the approach you took. Wrap lines\naround ~72 characters so `git log` stays easy to read in a terminal.\n\n- user-visible or technical bullet\n- tests / docs / migration notes if relevant\n\nRefs: #123\n# Co-authored-by: Name <name@example.com>\n",
      createdAt: next()
    },
    {
      id: "snippet:mdtask",
      label: "Markdown unchecked task",
      trigger: "task",
      body: "- [ ] ",
      createdAt: next()
    },
    {
      id: "snippet:mdcheck",
      label: "Markdown checked task",
      trigger: "done",
      body: "- [x] ",
      createdAt: next()
    },
    {
      id: "snippet:standup",
      label: "Daily stand-up update",
      trigger: "standup",
      body: "**Yesterday**\n- \n\n**Today**\n- \n\n**Blockers**\n- None\n",
      createdAt: next()
    },
    {
      id: "snippet:meeting",
      label: "Meeting notes template",
      trigger: "meeting",
      body: "# Meeting \u2014 ${datetime}\n\n**Attendees:** \n**Goal:** \n\n## Agenda\n1. \n\n## Discussion & decisions\n- \n\n## Action items\n| Owner | Task | Due |\n|-------|------|-----|\n|       |      |     |\n",
      createdAt: next()
    },
    {
      id: "snippet:blocker",
      label: "Slack / Teams \u2014 blocked message",
      trigger: "blocked",
      body: "Hi \u2014 I am blocked on **<short summary>**.\n\n**What I tried:**\n- \n\n**What I need from you:**\n- \n\nHappy to pair or jump on a quick call. Thanks!\n",
      createdAt: next()
    },
    {
      id: "snippet:pr",
      label: "Pull request description (full)",
      trigger: "pr",
      body: "## Summary\nWhat does this PR change, and why should reviewers care?\n\n## Type of change\n- [ ] Bug fix (non-breaking)\n- [ ] New feature\n- [ ] Breaking change / migration\n- [ ] Docs only\n\n## How to test\n1. \n2. \n\n## Screenshots / recordings\n\n\n## Rollout & risk\n- Feature flags:\n- Database / cache / infra:\n\n## Checklist\n- [ ] I self-reviewed the diff\n- [ ] Tests added or updated where it matters\n- [ ] Docs / changelog updated if user-facing\n",
      createdAt: next()
    },
    {
      id: "snippet:issue",
      label: "Bug report (GitHub / Jira style)",
      trigger: "bugreport",
      body: "## Summary\nOne sentence: what is broken or wrong?\n\n## Expected behavior\n\n\n## Actual behavior\n\n\n## Steps to reproduce\n1. \n2. \n3. \n\n## Environment\n| Item | Version / details |\n|------|-------------------|\n| OS / device | |\n| Browser (if web) | |\n| App / API / commit | |\n\n## Logs, screenshots, or recordings\n\n\n## Severity / impact\nWho is affected and how badly (blocks release, workaround exists, \u2026)?\n",
      createdAt: next()
    },
    {
      id: "snippet:changelog",
      label: "Changelog unreleased entry",
      trigger: "changelog",
      body: "## [Unreleased]\n\n### Added\n- \n\n### Changed\n- \n\n### Fixed\n- \n\n### Removed\n- \n",
      createdAt: next()
    },
    {
      id: "snippet:curl-json",
      label: "curl POST with JSON (template)",
      trigger: "curljson",
      body: `curl -sS -X POST 'https://api.example.com/v1/resource' \\
  -H 'Content-Type: application/json' \\
  -H 'Authorization: Bearer YOUR_TOKEN_HERE' \\
  -d '{"key": "value"}'
`,
      createdAt: next()
    },
    {
      id: "snippet:sql-select",
      label: "SQL SELECT skeleton",
      trigger: "sql",
      body: "SELECT\n  *\nFROM your_table\nWHERE 1 = 1\n  -- AND some_column = :value\nORDER BY created_at DESC\nLIMIT 100;\n",
      createdAt: next()
    },
    {
      id: "snippet:api-error-json",
      label: "JSON API error shape",
      trigger: "apierror",
      body: '{\n  "error": {\n    "code": "VALIDATION_FAILED",\n    "message": "Human-readable summary for clients.",\n    "details": [\n      { "field": "email", "issue": "must be a valid email" }\n    ]\n  }\n}\n',
      createdAt: next()
    },
    {
      id: "snippet:review",
      label: "Code review comment (constructive)",
      trigger: "review",
      body: "Nice work on this part \u2014 the approach reads clearly.\n\nOne suggestion: **<topic>** could be simplified by <idea>, because <reason>. Totally optional if you are tight on time.\n\nLet me know if you want to pair on it.\n",
      createdAt: next()
    },
    {
      id: "snippet:localhost",
      label: "IPv4 localhost",
      trigger: "localhost",
      body: "127.0.0.1",
      createdAt: next()
    },
    {
      id: "snippet:localurl",
      label: "Local dev URL (HTTPS)",
      trigger: "localurl",
      body: "https://127.0.0.1:3000",
      createdAt: next()
    },
    {
      id: "snippet:docker-logs",
      label: "docker compose logs (follow)",
      trigger: "dlogs",
      body: "docker compose logs -f --tail=200 SERVICE_NAME\n",
      createdAt: next()
    },
    {
      id: "snippet:shrug",
      label: "Shrug emoji",
      trigger: "shrug",
      body: "\xAF\\_(\u30C4)_/\xAF",
      createdAt: next()
    }
  ];
}
function mergeMissingBuiltins(existing, builtins) {
  const ids = new Set(existing.map((s) => s.id));
  const merged = [...existing];
  for (const b of builtins) {
    if (!ids.has(b.id)) {
      merged.push({ ...b, createdAt: Date.now() });
      ids.add(b.id);
    }
  }
  return merged;
}
function readSnippetsDb() {
  const builtins = defaultSnippets();
  try {
    const raw = (0, import_node_fs20.readFileSync)(snippetsPath(), "utf8");
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed.snippets)) return { snippets: builtins };
    return { snippets: mergeMissingBuiltins(parsed.snippets, builtins) };
  } catch {
    const db = { snippets: builtins };
    (0, import_node_fs20.writeFileSync)(snippetsPath(), `${JSON.stringify(db, null, 2)}
`, "utf8");
    return db;
  }
}
function getBuiltinSnippetIds() {
  return new Set(defaultSnippets().map((s) => s.id));
}
function isBuiltinSnippetId(id) {
  return getBuiltinSnippetIds().has(id);
}
function persistSnippetsDb(snippets) {
  const dir = (0, import_node_path20.join)(app.getPath("userData"), "search");
  (0, import_node_fs20.mkdirSync)(dir, { recursive: true });
  (0, import_node_fs20.writeFileSync)(snippetsPath(), `${JSON.stringify({ snippets }, null, 2)}
`, "utf8");
}
var SNIPPET_LABEL_MAX = 200;
var SNIPPET_TRIGGER_MAX = 48;
var SNIPPET_BODY_MAX = 1e5;
function normalizeSnippetBody(body) {
  return body.replace(/\r\n/g, "\n");
}
function validateSnippetWritePayload(label, trigger, body) {
  const tLabel = label.trim();
  const tTrigger = trigger.trim();
  const tBody = normalizeSnippetBody(body);
  if (tLabel.length === 0) return { ok: false, message: "Title is required" };
  if (tLabel.length > SNIPPET_LABEL_MAX) return { ok: false, message: `Title must be at most ${SNIPPET_LABEL_MAX} characters` };
  if (tTrigger.length === 0) return { ok: false, message: "Trigger is required" };
  if (tTrigger.length > SNIPPET_TRIGGER_MAX) {
    return { ok: false, message: `Trigger must be at most ${SNIPPET_TRIGGER_MAX} characters` };
  }
  if (/[\n\r]/.test(tTrigger)) return { ok: false, message: "Trigger must be a single line" };
  if (tBody.trim().length === 0) return { ok: false, message: "Body cannot be empty" };
  if (tBody.length > SNIPPET_BODY_MAX) return { ok: false, message: `Body must be at most ${SNIPPET_BODY_MAX} characters` };
  return { ok: true };
}
function triggerTaken(snippets, trigger, excludeId) {
  const want = trigger.trim().toLowerCase();
  return snippets.some((s) => s.id !== excludeId && s.trigger.trim().toLowerCase() === want);
}
function addUserSnippet(payload) {
  const v = validateSnippetWritePayload(payload.label, payload.trigger, payload.body);
  if (!v.ok) return { ok: false, message: v.message };
  const db = readSnippetsDb();
  const label = payload.label.trim();
  const trigger = payload.trigger.trim();
  const body = normalizeSnippetBody(payload.body);
  if (triggerTaken(db.snippets, trigger, null)) {
    return { ok: false, message: "Another snippet already uses this trigger" };
  }
  const id = `snippet:user:${(0, import_node_crypto7.randomUUID)()}`;
  const createdAt = Date.now();
  const next = [...db.snippets, { id, label, trigger, body, createdAt }];
  persistSnippetsDb(next);
  return { ok: true, message: "Snippet saved", id };
}
function updateUserSnippet(id, payload) {
  if (isBuiltinSnippetId(id)) {
    return { ok: false, message: "Built-in snippets cannot be edited" };
  }
  const v = validateSnippetWritePayload(payload.label, payload.trigger, payload.body);
  if (!v.ok) return { ok: false, message: v.message };
  const db = readSnippetsDb();
  const idx = db.snippets.findIndex((s) => s.id === id);
  if (idx < 0) return { ok: false, message: "Snippet not found" };
  const label = payload.label.trim();
  const trigger = payload.trigger.trim();
  const body = normalizeSnippetBody(payload.body);
  if (triggerTaken(db.snippets, trigger, id)) {
    return { ok: false, message: "Another snippet already uses this trigger" };
  }
  const next = db.snippets.map(
    (s, i) => i === idx ? { ...s, label, trigger, body, createdAt: s.createdAt } : s
  );
  persistSnippetsDb(next);
  return { ok: true, message: "Snippet updated" };
}
function deleteUserSnippet(id) {
  if (isBuiltinSnippetId(id)) {
    return { ok: false, message: "Built-in snippets cannot be deleted" };
  }
  const db = readSnippetsDb();
  const next = db.snippets.filter((s) => s.id !== id);
  if (next.length === db.snippets.length) return { ok: false, message: "Snippet not found" };
  persistSnippetsDb(next);
  return { ok: true, message: "Snippet removed" };
}
function formatDate(date) {
  return date.toISOString().slice(0, 10);
}
function formatTime(date) {
  return date.toTimeString().slice(0, 8);
}
function interpolateSnippet(input, now = /* @__PURE__ */ new Date()) {
  return input.split("${date}").join(formatDate(now)).split("${time}").join(formatTime(now)).split("${datetime}").join(`${formatDate(now)} ${formatTime(now)}`).split("${iso}").join(now.toISOString()).split("${year}").join(String(now.getFullYear())).split("${timestamp}").join(String(Math.floor(now.getTime() / 1e3))).split("${hostname}").join((0, import_node_os10.hostname)()).replace(/\$\{uuid\}/g, () => (0, import_node_crypto7.randomUUID)());
}
function friendlyTriggerDisplay(trigger) {
  const stripped = trigger.replace(/^;/, "").trim();
  if (!stripped) return trigger.trim();
  return stripped.charAt(0).toUpperCase() + stripped.slice(1);
}
function resolvedSnippetLabel(snippet) {
  const fromFile = snippet.label?.trim();
  if (fromFile) return fromFile;
  if (snippet.id === "snippet:today") return "Get today's date";
  if (snippet.id === "snippet:time") return "Get current time";
  if (snippet.id === "snippet:issue") return "Bug report (GitHub / Jira issue template)";
  return void 0;
}
function snippetBodyPreview(body) {
  const oneLine = body.replace(/\s+/g, " ").trim();
  return oneLine.length > 72 ? `${oneLine.slice(0, 69)}\u2026` : oneLine;
}
function snippetRowSubtitle(snippet, title) {
  const preview = snippetBodyPreview(snippet.body);
  const trigRaw = snippet.trigger.trim();
  const trigHint = friendlyTriggerDisplay(snippet.trigger);
  let line;
  if (title === trigRaw) {
    line = preview.length > 0 ? preview : "Copies text to the clipboard";
  } else {
    line = preview.length > 0 ? `${trigHint} \xB7 ${preview}` : trigHint;
  }
  if (snippet.scope && snippet.scope !== "global") {
    line = line.length > 0 ? `${line} \xB7 ${snippet.scope}` : snippet.scope;
  }
  return line;
}
function listSnippetsForUi() {
  const db = readSnippetsDb();
  const builtinIds = getBuiltinSnippetIds();
  return db.snippets.map((snippet) => {
    const label = resolvedSnippetLabel(snippet);
    const title = (label ?? snippet.trigger).trim() || snippet.trigger;
    return {
      id: snippet.id,
      title,
      subtitle: snippetRowSubtitle(snippet, title),
      trigger: snippet.trigger,
      bodyTemplate: snippet.body,
      resolvedPreview: interpolateSnippet(snippet.body),
      readonly: builtinIds.has(snippet.id)
    };
  });
}
function copySnippetById(id) {
  const db = readSnippetsDb();
  const snippet = db.snippets.find((s) => s.id === id);
  if (!snippet) return { ok: false, message: "Snippet not found" };
  const text = interpolateSnippet(snippet.body);
  clipboard.writeText(text);
  captureClipboardSnapshot();
  return { ok: true, message: "Copied to clipboard" };
}
var snippetsProvider = {
  providerId: "snippets",
  async buildDocuments() {
    const db = readSnippetsDb();
    return db.snippets.map((snippet) => {
      const label = resolvedSnippetLabel(snippet);
      const title = (label ?? snippet.trigger).trim() || snippet.trigger;
      const tokens = [snippet.trigger, snippet.body, label].filter(Boolean).join(" ");
      return {
        id: snippet.id,
        category: "snippets",
        title,
        subtitle: snippetRowSubtitle(snippet, title),
        tokens,
        action: { type: "copy-text", text: interpolateSnippet(snippet.body) },
        updatedAt: snippet.createdAt
      };
    });
  }
};

// src/main/search/ranker.ts
var CATEGORY_PRIOR = {
  applications: 0.72,
  files: 0.6,
  clipboard: 0.45,
  /** Was 0.4 (lowest), which pushed real quick notes below random `*notes*` files. */
  "quick-notes": 0.68,
  extensions: 0.68,
  store: 0.25,
  "mac-cli": 0.46,
  "native-command": 0.7,
  commands: 0.66,
  snippets: 0.58,
  "quick-links": 0.55,
  calculator: 0.9,
  "color-converter": 0.9
};
function normalizeRecency(ms) {
  if (ms <= 0) return 0;
  const oneDay = 24 * 60 * 60 * 1e3;
  const ageDays = ms / oneDay;
  return 1 / (1 + ageDays);
}
function normalizeFrequency(frequency) {
  if (frequency <= 0) return 0;
  return Math.min(1, Math.log10(frequency + 1) / 2);
}
function fuzzyBonus(distance) {
  if (distance === void 0) return 0;
  if (distance <= 0) return 0.08;
  if (distance === 1) return 0.05;
  if (distance === 2) return 0.02;
  return 0;
}
function computeWeightedScore(input) {
  const lexical = Math.max(0, Math.min(1, input.lexical));
  const recency = normalizeRecency(input.recencyMs);
  const frequency = normalizeFrequency(input.frequency);
  const success = Math.max(0, Math.min(1, input.successRate));
  const prior = CATEGORY_PRIOR[input.category] ?? 0.35;
  const fuzzy = fuzzyBonus(input.fuzzyDistance);
  const popularity = input.popularity ? Math.min(1, Math.log10(input.popularity + 1) / 7) : 0;
  const weighted = lexical * 0.6 + recency * 0.1 + frequency * 0.1 + success * 0.05 + prior * 0.05 + fuzzy + popularity * 0.1;
  return Math.round(weighted * 1e3);
}
function shouldPreferRecent(leftScore, leftAgeMs, rightScore, rightAgeMs) {
  const gap = Math.abs(leftScore - rightScore);
  if (gap > 20) return false;
  return leftAgeMs < rightAgeMs;
}

// src/main/search/directoryRecommendations.ts
var import_node_path21 = require("node:path");
function visitScore(visit, now) {
  const ageDays = Math.max(0, (now - visit.lastVisitedAt) / 864e5);
  const recencyBoost = Math.max(0, 14 - ageDays);
  return visit.count * 10 + recencyBoost;
}
function rankDirectoryRecommendations(visits, options = {}) {
  const now = options.now ?? Date.now();
  const limit = options.limit ?? 5;
  const siblingThreshold = options.siblingThreshold ?? 3;
  const excluded = new Set(options.excludedPaths ?? []);
  const validVisits = Object.entries(visits).filter(
    ([path7, visit]) => path7.startsWith("/") && visit !== null && typeof visit === "object" && Number.isFinite(visit.count) && visit.count > 0 && Number.isFinite(visit.lastVisitedAt)
  );
  const childrenByParent = /* @__PURE__ */ new Map();
  for (const entry of validVisits) {
    const parent = (0, import_node_path21.dirname)(entry[0]);
    const siblings = childrenByParent.get(parent) ?? [];
    siblings.push(entry);
    childrenByParent.set(parent, siblings);
  }
  const collapsedParents = new Set(
    Array.from(childrenByParent.entries()).filter(([parent, children]) => !excluded.has(parent) && children.length >= siblingThreshold).map(([parent]) => parent)
  );
  const recommendations = /* @__PURE__ */ new Map();
  for (const [path7, visit] of validVisits) {
    const parent = (0, import_node_path21.dirname)(path7);
    const recommendationPath = collapsedParents.has(parent) ? parent : path7;
    if (excluded.has(recommendationPath)) continue;
    const existing = recommendations.get(recommendationPath);
    const score = visitScore(visit, now);
    recommendations.set(recommendationPath, {
      path: recommendationPath,
      count: (existing?.count ?? 0) + visit.count,
      lastVisitedAt: Math.max(existing?.lastVisitedAt ?? 0, visit.lastVisitedAt),
      score: (existing?.score ?? 0) + score
    });
  }
  const ranked = Array.from(recommendations.values()).sort(
    (a, b) => b.score - a.score || b.lastVisitedAt - a.lastVisitedAt || a.path.localeCompare(b.path)
  );
  return ranked.filter((candidate, index) => {
    return !ranked.slice(0, index).some((stronger) => {
      return candidate.path.startsWith(`${stronger.path}/`) || stronger.path.startsWith(`${candidate.path}/`);
    });
  }).slice(0, Math.max(0, limit));
}

// src/main/search/service.ts
var execFileAsync11 = (0, import_node_util11.promisify)(import_node_child_process12.execFile);
var MAX_RESULTS = 80;
var PROVIDER_REFRESH_MIN_AGE_MS = 1e4;
var FILE_INDEX_LIMIT = 4e3;
var SHELL_METACHAR_RE = /[;|&`$(){}[\]\n\r<>\\]/;
function validateShellCommand(command) {
  const trimmed = command.trim();
  if (!trimmed) return { ok: false, message: "Empty shell command" };
  if (SHELL_METACHAR_RE.test(trimmed)) {
    return {
      ok: false,
      message: "Shell metacharacters are not allowed in run-shell commands."
    };
  }
  return { ok: true };
}
function safetyForAction(action) {
  if (action.type === "run-shell") {
    return { id: "shell.run", context: { command: action.command } };
  }
  if (action.type === "install-extension") {
    return { id: "extension.install", context: { extensionId: action.extensionId } };
  }
  if (action.type === "run-native-command") {
    if (action.commandId === "empty-trash") {
      return { id: "trash.empty", context: { commandId: action.commandId } };
    }
    if (action.commandId === "sleep-system") {
      return { id: "system.sleep", context: { commandId: action.commandId } };
    }
    if (action.commandId === "quit-tezbar") {
      return { id: "app.quit", context: { commandId: action.commandId } };
    }
    return { id: "native.command", context: { commandId: action.commandId } };
  }
  return null;
}
async function runWithSafety(safetyId, context, run) {
  const descriptor = getSafetyDescriptor(safetyId);
  if (!descriptor) {
    return { ok: false, message: `Safety descriptor missing: ${safetyId}` };
  }
  const dryRun = getSafetyDryRun();
  const window2 = BrowserWindow.getFocusedWindow() ?? BrowserWindow.getAllWindows()[0] ?? null;
  const { accepted } = await confirmSafetyAction(window2, descriptor, context, { dryRun });
  if (!accepted) {
    recordSafetyEntry({
      action: safetyId,
      title: descriptor.title,
      risk: descriptor.risk,
      ok: false,
      message: "Cancelled by user",
      context: { ...context, dryRun }
    });
    return { ok: false, message: "Cancelled" };
  }
  if (dryRun) {
    const message = `Dry run: would have ${descriptor.title.toLowerCase()}.`;
    recordSafetyEntry({
      action: safetyId,
      title: descriptor.title,
      risk: descriptor.risk,
      ok: true,
      message,
      context: { ...context, dryRun: true }
    });
    return { ok: true, message };
  }
  const result = await run();
  recordSafetyEntry({
    action: safetyId,
    title: descriptor.title,
    risk: descriptor.risk,
    ok: result.ok,
    message: result.message,
    context
  });
  return result;
}
var indexDb = getInstance();
var bootstrapPromise = null;
var fileBootstrapPromise = null;
var volatileRefreshPromise = null;
var stopFileWatcher = null;
var providerRefreshTimer = null;
var lastExtensionRefreshAt = 0;
var lastVolatileRefreshAt = 0;
function isPresent(value) {
  return value !== null && value !== void 0;
}
function uniqById(items) {
  const seen = /* @__PURE__ */ new Set();
  const out = [];
  for (const item of items) {
    if (seen.has(item.id)) continue;
    seen.add(item.id);
    out.push(item);
  }
  return out;
}
function attachSearchResultIcons(items) {
  return items.map(
    (item) => item.action.type === "open-file" ? { ...item, iconDataUrl: fileIconDataUrl(item.action.path) } : item
  );
}
function actionIdFromResult(action, resultId) {
  if (resultId) return resultId;
  switch (action.type) {
    case "open-app":
      return `open-app:${action.appName}`;
    case "open-file":
      return `open-file:${action.path}`;
    case "open-with-app":
      return `open-with-app:${action.appName ?? "default"}:${action.path}`;
    case "copy-text":
      return `copy-text:${action.text.slice(0, 64)}`;
    case "copy-and-paste-text":
      return `copy-and-paste-text:${action.text.slice(0, 64)}`;
    case "add-note":
      return `add-note:${action.text.slice(0, 64)}`;
    case "open-url":
      return `open-url:${action.url}`;
    case "install-extension":
      return `install-extension:${action.extensionId}`;
    case "run-extension-command":
      return `extcmd:${action.extensionId}:${action.commandName}`;
    case "run-shell":
      return `run-shell:${action.command}`;
    case "invoke-command":
      return `command:${action.commandId}`;
    case "run-native-command":
      return `native:${action.commandId}`;
    default:
      return "unknown-action";
  }
}
async function upsertProvider(provider) {
  const docs = await provider.buildDocuments();
  if (provider.providerId === "commands") {
    indexDb.removeDocumentsByCategory("commands");
    indexDb.removeDocumentsByCategory("native-command");
  } else if (provider.providerId === "clipboard") {
    indexDb.removeDocumentsByCategory("clipboard");
  } else if (provider.providerId === "notes") {
    indexDb.removeDocumentsByCategory("quick-notes");
  } else if (provider.providerId === "snippets") {
    indexDb.removeDocumentsByCategory("snippets");
  } else if (provider.providerId === "quick-links") {
    indexDb.removeDocumentsByCategory("quick-links");
  } else if (provider.providerId === "apps") {
    indexDb.removeDocumentsByCategory("applications");
  } else if (provider.providerId === "extensions") {
    indexDb.removeDocumentsByCategory("extensions");
  }
  if (docs.length > 0) {
    indexDb.upsertDocuments(docs);
  }
}
async function refreshAllProviders() {
  await Promise.all([
    upsertProvider(commandsProvider),
    upsertProvider(clipboardProvider),
    upsertProvider(notesProvider),
    upsertProvider(snippetsProvider),
    upsertProvider(quickLinksProvider),
    upsertProvider(appsProvider)
  ]);
  indexDb.clearSearchCache();
  void upsertProvider(extensionsProvider).then(() => {
    lastExtensionRefreshAt = Date.now();
    indexDb.clearSearchCache();
  }).catch((error) => {
    console.warn("[Search] Failed to build extension index:", error);
  });
}
async function refreshVolatileProviders() {
  if (volatileRefreshPromise) return volatileRefreshPromise;
  volatileRefreshPromise = (async () => {
    captureClipboardSnapshot();
    await Promise.all([
      upsertProvider(commandsProvider),
      upsertProvider(clipboardProvider),
      upsertProvider(notesProvider),
      upsertProvider(snippetsProvider),
      upsertProvider(quickLinksProvider)
    ]);
    const now = Date.now();
    if (now - lastExtensionRefreshAt > 3e4) {
      lastExtensionRefreshAt = now;
      await upsertProvider(extensionsProvider);
    }
    lastVolatileRefreshAt = Date.now();
    indexDb.clearSearchCache();
  })().finally(() => {
    volatileRefreshPromise = null;
  });
  return volatileRefreshPromise;
}
function refreshVolatileProvidersIfStale() {
  if (Date.now() - lastVolatileRefreshAt < PROVIDER_REFRESH_MIN_AGE_MS) return;
  void refreshVolatileProviders().catch((error) => {
    console.warn("[Search] Failed to refresh providers:", error);
  });
}
function startBackgroundFileIndexing() {
  if (fileBootstrapPromise) return;
  fileBootstrapPromise = (async () => {
    const fileDocs = await collectInitialFileDocuments(FILE_INDEX_LIMIT);
    indexDb.replaceDocumentsByCategory("files", fileDocs);
    stopFileWatcher = startFileWatcher(({ upserts, removeIds }) => {
      if (upserts.length > 0) indexDb.upsertDocuments(upserts);
      for (const removeId of removeIds) {
        indexDb.removeDocumentById(removeId);
      }
      if (upserts.length > 0 || removeIds.length > 0) {
        indexDb.clearSearchCache();
      }
    });
  })().catch((error) => {
    fileBootstrapPromise = null;
    console.warn("[Search] Failed to build file index:", error);
  });
}
async function bootstrapSearchIndex() {
  if (bootstrapPromise) {
    return bootstrapPromise;
  }
  bootstrapPromise = (async () => {
    await indexDb.ensureInitialized();
    indexDb.removeDocumentsByCategory("clipboard");
    captureClipboardSnapshot();
    await refreshAllProviders();
    lastVolatileRefreshAt = Date.now();
    startBackgroundFileIndexing();
    app.once("before-quit", () => {
      stopFileWatcher?.();
      stopFileWatcher = null;
      if (providerRefreshTimer) {
        clearInterval(providerRefreshTimer);
        providerRefreshTimer = null;
      }
    });
  })();
  return bootstrapPromise;
}
async function reindexQuickNotes() {
  await bootstrapSearchIndex();
  await upsertProvider(notesProvider);
  indexDb.clearSearchCache();
}
async function reindexSnippets() {
  await bootstrapSearchIndex();
  await upsertProvider(snippetsProvider);
  indexDb.clearSearchCache();
}
async function reindexExtensions() {
  await bootstrapSearchIndex();
  await upsertProvider(extensionsProvider);
  lastExtensionRefreshAt = Date.now();
  indexDb?.clearSearchCache();
}
function internalSurfaceBoost(category, title, query, subtitle) {
  const hit = category === "native-command" || category === "commands" || category === "extensions" || category === "applications" || category === "quick-notes";
  if (!hit) return 0;
  const normalizedTitle = title.toLowerCase();
  const normalizedQuery = query.trim().toLowerCase();
  if (!normalizedQuery) return 0;
  let boost = 0;
  if (normalizedTitle === normalizedQuery) {
    boost = 600;
  } else if (normalizedTitle.startsWith(normalizedQuery)) {
    boost = 420;
  } else {
    const titleWords = normalizedTitle.split(/\s+/);
    if (titleWords.some((word) => word.startsWith(normalizedQuery))) {
      boost = 300;
    } else if (normalizedTitle.includes(normalizedQuery)) {
      boost = 150;
    }
  }
  if (category === "extensions" && subtitle) {
    const parts = subtitle.split(" \xB7 ");
    const extName = parts[0]?.toLowerCase();
    if (extName) {
      const slugName = extName.replace(/\s+/g, "");
      if (extName === normalizedQuery || slugName === normalizedQuery) boost = Math.max(boost, 1200);
      else if (extName.startsWith(normalizedQuery) || slugName.startsWith(normalizedQuery))
        boost = Math.max(boost, 800);
      else if (extName.includes(normalizedQuery) || slugName.includes(normalizedQuery))
        boost = Math.max(boost, 400);
    }
  }
  return boost;
}
function recentQuickNoteBoost(category, updatedAt, now) {
  if (category !== "quick-notes") return 0;
  const ageMs = now - updatedAt;
  if (ageMs < 9e4) return 1100;
  if (ageMs < 5 * 60 * 1e3) return 520;
  if (ageMs < 30 * 60 * 1e3) return 140;
  return 0;
}
function exactRecentQuickNoteBoost(category, title, query, updatedAt, now) {
  if (category !== "quick-notes") return 0;
  const ageMs = now - updatedAt;
  if (ageMs > 5 * 60 * 1e3) return 0;
  const normalizedTitle = title.trim().toLowerCase();
  const normalizedQuery = query.trim().toLowerCase();
  if (!normalizedQuery || !normalizedTitle) return 0;
  if (normalizedTitle === normalizedQuery) return 1800;
  if (normalizedTitle.startsWith(normalizedQuery)) return 1400;
  if (normalizedTitle.includes(normalizedQuery)) return 900;
  return 0;
}
function rankRows(query, docs) {
  const now = Date.now();
  const stats = indexDb?.getActionStats(docs.map((entry) => entry.doc.id)) ?? /* @__PURE__ */ new Map();
  const ranked = docs.map((entry) => {
    const actionStat = stats.get(entry.doc.id);
    const frequency = actionStat?.frequency ?? 0;
    const totalCount = actionStat?.totalCount ?? 0;
    const successCount = actionStat?.successCount ?? 0;
    const successRate = totalCount > 0 ? successCount / totalCount : 0;
    const activityAt = actionStat?.lastUsedAt && actionStat.lastUsedAt > 0 ? actionStat.lastUsedAt : entry.doc.updatedAt;
    const score = computeWeightedScore({
      lexical: entry.lexical,
      recencyMs: now - activityAt,
      frequency,
      successRate,
      category: entry.doc.category,
      fuzzyDistance: entry.fuzzyDistance,
      popularity: entry.doc.popularity
    }) + internalSurfaceBoost(entry.doc.category, entry.doc.title, query, entry.doc.subtitle) + recentQuickNoteBoost(entry.doc.category, entry.doc.updatedAt, now) + exactRecentQuickNoteBoost(
      entry.doc.category,
      entry.doc.title,
      query,
      entry.doc.updatedAt,
      now
    ) + (() => {
      const q = query.trim().toLowerCase();
      if (!q) return 120;
      if (/\bnotes?\b/.test(q) || q.includes("quick note")) return 780;
      return 120;
    })();
    return {
      id: entry.doc.id,
      title: entry.doc.title,
      subtitle: entry.doc.subtitle,
      category: entry.doc.category,
      score,
      action: entry.doc.action,
      updatedAt: activityAt
    };
  });
  ranked.sort((left, right) => {
    if (left.score !== right.score) {
      const preferRecent = shouldPreferRecent(
        left.score,
        now - left.updatedAt,
        right.score,
        now - right.updatedAt
      );
      if (preferRecent) return -1;
      const reversePreferRecent = shouldPreferRecent(
        right.score,
        now - right.updatedAt,
        left.score,
        now - left.updatedAt
      );
      if (reversePreferRecent) return 1;
      return right.score - left.score;
    }
    return right.updatedAt - left.updatedAt;
  });
  return ranked;
}
function recommendationBoost(id) {
  if (id === "native:open-clipboard-history") return 900;
  if (id === "native:open-snippets") return 880;
  if (id === "extcmd:raycast.kill-process:index") return 860;
  if (id === "extcmd:raycast.port-manager:kill-listening-process") return 760;
  if (id === "extcmd:raycast.port-manager:open-ports") return 720;
  if (id === "extcmd:raycast.port-manager:open-ports-menu-bar") return 700;
  return 0;
}
function buildRecommendations() {
  const now = Date.now();
  const seeds = indexDb.listRecommendedDocuments(MAX_RESULTS).map((row) => {
    const totalCount = row.totalCount > 0 ? row.totalCount : 0;
    const successRate = totalCount > 0 ? row.successCount / totalCount : 0;
    return {
      id: row.id,
      category: row.category,
      title: row.title,
      subtitle: row.subtitle,
      action: indexDb.parseAction(row.actionJson),
      updatedAt: row.updatedAt,
      frequency: row.frequency,
      successRate,
      lastUsedAt: row.lastUsedAt
    };
  });
  const pinnedOrder = [
    "native:open-clipboard-history",
    "native:open-snippets",
    "extcmd:raycast.port-manager:kill-listening-process",
    "extcmd:raycast.kill-process:index",
    "extcmd:raycast.port-manager:open-ports",
    "extcmd:raycast.port-manager:open-ports-menu-bar"
  ];
  const pinnedRows = indexDb.getDocumentsByIds(pinnedOrder);
  const existingIds = new Set(seeds.map((seed) => seed.id));
  for (const row of pinnedRows) {
    if (existingIds.has(row.id)) continue;
    seeds.push({
      id: row.id,
      category: row.category,
      title: row.title,
      subtitle: row.subtitle,
      action: indexDb.parseAction(row.actionJson),
      updatedAt: row.updatedAt,
      frequency: 0,
      successRate: 0,
      lastUsedAt: 0
    });
  }
  return seeds.map((seed) => {
    const activityAt = seed.lastUsedAt > 0 ? seed.lastUsedAt : seed.updatedAt;
    const score = computeWeightedScore({
      lexical: 0.92,
      recencyMs: now - activityAt,
      frequency: seed.frequency,
      successRate: seed.successRate,
      category: seed.category
    }) + recommendationBoost(seed.id);
    return {
      id: seed.id,
      title: seed.title,
      subtitle: seed.subtitle,
      category: seed.category,
      score,
      action: seed.action
    };
  }).sort((a, b) => b.score - a.score).slice(0, MAX_RESULTS);
}
function decodeLsofCommandName(value) {
  return value.replace(
    /\\x([0-9a-fA-F]{2})/g,
    (_, hex) => String.fromCharCode(Number.parseInt(hex, 16))
  );
}
function displayProcessNameFromCommand(command) {
  const trimmed = command.trim();
  if (!trimmed) return "";
  const parts = trimmed.split("/").filter(Boolean);
  return decodeLsofCommandName(parts.at(-1) ?? trimmed);
}
function parseProcessNameMap(stdout) {
  const names = /* @__PURE__ */ new Map();
  for (const line of stdout.split("\n")) {
    const match = line.match(/^\s*(\d+)\s+(.+?)\s*$/);
    if (!match) continue;
    const name = displayProcessNameFromCommand(match[2]);
    if (name) names.set(match[1], name);
  }
  return names;
}
async function readProcessNameMap() {
  try {
    const { stdout } = await execFileAsync11("/bin/ps", ["-axo", "pid=,comm="]);
    return parseProcessNameMap(stdout);
  } catch {
    return /* @__PURE__ */ new Map();
  }
}
function parseOpenPortProcesses(stdout, processNames = /* @__PURE__ */ new Map()) {
  const lines = stdout.split("\n").map((line) => line.trim()).filter(Boolean);
  if (lines.length <= 1) return [];
  const grouped = /* @__PURE__ */ new Map();
  for (const line of lines) {
    const parts = line.split(/\s+/);
    if (parts.length < 3) continue;
    const match = line.match(/:(\d+)\s+\(LISTEN\)$/);
    if (!match) continue;
    const port = Number(match[1]);
    if (!Number.isFinite(port)) continue;
    const pid = parts[1] ?? "?";
    const process2 = processNames.get(pid) ?? decodeLsofCommandName(parts[0] ?? "unknown");
    const user = parts[2] ?? "unknown";
    const key = `${process2}:${pid}:${user}`;
    const existing = grouped.get(key);
    if (existing) {
      existing.ports.add(port);
      continue;
    }
    grouped.set(key, {
      process: process2,
      user,
      pid,
      ports: /* @__PURE__ */ new Set([port])
    });
  }
  return Array.from(grouped.values()).map((entry) => ({
    process: entry.process,
    user: entry.user,
    pid: entry.pid,
    ports: Array.from(entry.ports).sort((a, b) => a - b)
  })).sort((a, b) => a.process.localeCompare(b.process) || a.pid.localeCompare(b.pid));
}
async function searchEverything(query) {
  await bootstrapSearchIndex();
  refreshVolatileProvidersIfStale();
  const trimmed = query.trim();
  if (!trimmed) {
    return attachSearchResultIcons(buildRecommendations());
  }
  const rows = indexDb.getSearch(trimmed, MAX_RESULTS);
  const docs = rows.map(
    (row) => ({
      doc: {
        id: row.id,
        category: row.category,
        title: row.title,
        subtitle: row.subtitle,
        tokens: `${row.title} ${row.subtitle}`,
        action: indexDb.parseAction(row.actionJson),
        updatedAt: row.updatedAt,
        popularity: row.popularity
      },
      lexical: row.lexical,
      fuzzyDistance: row.fuzzyDistance
    })
  );
  const ranked = rankRows(trimmed, docs);
  const asResults = ranked.map((item) => ({
    id: item.id,
    title: item.title,
    subtitle: item.subtitle,
    category: item.category,
    score: item.score,
    action: item.action
  }));
  const resultsWithoutFiles = asResults.filter((result) => {
    if (result.category === "files") return false;
    return true;
  });
  const fileResults = asResults.filter((result) => result.category === "files");
  let fallbackFiles = [];
  if (trimmed.length > 0 && fileResults.length < 2) {
    fallbackFiles = await spotlightFallback(trimmed);
  }
  const emojiPickerResult = buildEmojiPickerSearchResult(trimmed);
  const openPortResults = await searchPortManagerOpenPorts(trimmed);
  function quickNoteAddScore(query2) {
    const q = query2.trim().toLowerCase();
    if (!q) return 120;
    if (/\bnotes?\b/.test(q) || q.includes("quick note")) return 780;
    return 120;
  }
  const noteAdd = trimmed ? [
    {
      id: `note-add:${trimmed}`,
      title: `Add quick note: ${trimmed.slice(0, 64)}`,
      subtitle: "Quick notes",
      category: "quick-notes",
      score: quickNoteAddScore(trimmed),
      action: { type: "add-note", text: trimmed }
    }
  ] : [];
  const results = uniqById([
    ...resultsWithoutFiles,
    ...emojiPickerResult,
    ...fileResults,
    ...fallbackFiles,
    ...openPortResults,
    ...noteAdd
  ]).sort((a, b) => b.score - a.score).slice(0, MAX_RESULTS);
  return attachSearchResultIcons(results);
}
function expandUserPath(input) {
  if (input === "~") return (0, import_node_os11.homedir)();
  if (input.startsWith("~/")) return (0, import_node_path22.join)((0, import_node_os11.homedir)(), input.slice(2));
  return input;
}
function resolveSlashPathInput(input) {
  if (!input.startsWith("/")) return expandUserPath(input);
  const absolutePrefixes = [
    "/Users/",
    "/Volumes/",
    "/private/",
    "/tmp/",
    "/var/",
    "/System/",
    "/Library/"
  ];
  if (absolutePrefixes.some((prefix) => input.startsWith(prefix))) {
    return input;
  }
  if (input === "/Users" || input === "/Volumes") {
    return input;
  }
  return (0, import_node_path22.join)((0, import_node_os11.homedir)(), input.slice(1));
}
function displayUserPath(path7) {
  const home = (0, import_node_os11.homedir)();
  if (path7 === home) return "~";
  if (path7.startsWith(`${home}/`)) return `~/${path7.slice(home.length + 1)}`;
  return path7;
}
function splitPathCompletionQuery(raw) {
  const query = raw.trimStart();
  const body = query === "/" ? "" : query;
  const expandedBody = resolveSlashPathInput(body);
  let appMode = false;
  let targetPart = expandedBody;
  let appTerm = "";
  const trimmedBody = expandedBody.trimEnd();
  if (expandedBody.endsWith(" ") && trimmedBody && (0, import_node_fs21.existsSync)(trimmedBody)) {
    appMode = true;
    targetPart = trimmedBody;
    appTerm = "";
  } else if (!(0, import_node_fs21.existsSync)(expandedBody)) {
    let splitAt = -1;
    for (let index = expandedBody.length - 1; index >= 0; index--) {
      if (expandedBody[index] !== " ") continue;
      const beforeSpace = expandedBody.slice(0, index).trimEnd();
      if (beforeSpace && (0, import_node_fs21.existsSync)(beforeSpace)) {
        splitAt = index;
        break;
      }
    }
    if (splitAt >= 0) {
      appMode = true;
      targetPart = expandedBody.slice(0, splitAt).trimEnd();
      appTerm = expandedBody.slice(splitAt + 1).trimStart();
    }
  }
  if (!targetPart) {
    return { targetPath: (0, import_node_os11.homedir)(), appTerm, appMode };
  }
  if (targetPart.startsWith("/")) {
    return { targetPath: targetPart, appTerm, appMode };
  }
  if (targetPart.startsWith("~")) {
    return { targetPath: expandUserPath(targetPart), appTerm, appMode };
  }
  return { targetPath: (0, import_node_path22.resolve)((0, import_node_os11.homedir)(), targetPart), appTerm, appMode };
}
function pathCompletionBase(targetPath) {
  if (targetPath.endsWith("/")) return { directory: targetPath, prefix: "" };
  try {
    if ((0, import_node_fs21.existsSync)(targetPath) && (0, import_node_fs21.statSync)(targetPath).isDirectory()) {
      return { directory: targetPath, prefix: "" };
    }
  } catch {
  }
  return { directory: (0, import_node_path22.dirname)(targetPath), prefix: (0, import_node_path22.basename)(targetPath) };
}
function directoryVisitStorePath() {
  return (0, import_node_path22.join)(app.getPath("userData"), "directory-visits.json");
}
function readDirectoryVisitStore() {
  try {
    const parsed = JSON.parse((0, import_node_fs21.readFileSync)(directoryVisitStorePath(), "utf8"));
    if (!parsed || parsed.version !== 1 || typeof parsed.visits !== "object") {
      return { version: 1, visits: {} };
    }
    return parsed;
  } catch {
    return { version: 1, visits: {} };
  }
}
function recordDirectoryVisit(path7) {
  try {
    const normalized = (0, import_node_path22.resolve)(path7);
    if (!(0, import_node_fs21.statSync)(normalized).isDirectory()) return;
    const store2 = readDirectoryVisitStore();
    const existing = store2.visits[normalized];
    store2.visits[normalized] = {
      count: (existing?.count ?? 0) + 1,
      lastVisitedAt: Date.now()
    };
    const storePath2 = directoryVisitStorePath();
    (0, import_node_fs21.mkdirSync)((0, import_node_path22.dirname)(storePath2), { recursive: true });
    (0, import_node_fs21.writeFileSync)(storePath2, JSON.stringify(store2), "utf8");
  } catch (error) {
    console.warn("[Search] Failed to record directory visit:", error);
  }
}
function recommendedDirectories() {
  return rankDirectoryRecommendations(readDirectoryVisitStore().visits, {
    limit: 50,
    excludedPaths: [(0, import_node_os11.homedir)()]
  }).filter((item) => {
    try {
      return (0, import_node_fs21.statSync)(item.path).isDirectory();
    } catch {
      return false;
    }
  }).slice(0, 5).map((item, index) => ({
    id: `path-recommended:${item.path}`,
    title: (0, import_node_path22.basename)(item.path),
    subtitle: displayUserPath(item.path),
    kind: "directory",
    section: "recommended",
    badge: "Recommended",
    iconDataUrl: folderIconDataUrl,
    value: `${item.path}/`,
    path: item.path,
    score: 5e3 - index
  }));
}
function openWithUsageStorePath() {
  return (0, import_node_path22.join)(app.getPath("userData"), "open-with-usage.json");
}
function readOpenWithUsageStore() {
  try {
    const parsed = JSON.parse((0, import_node_fs21.readFileSync)(openWithUsageStorePath(), "utf8"));
    if (!parsed || typeof parsed !== "object" || parsed.version !== 1 || typeof parsed.keys !== "object") {
      return { version: 1, keys: {} };
    }
    if (!parsed.aliases || typeof parsed.aliases !== "object") {
      parsed.aliases = {};
    }
    return parsed;
  } catch {
    return { version: 1, keys: {} };
  }
}
function writeOpenWithUsageStore(store2) {
  const path7 = openWithUsageStorePath();
  (0, import_node_fs21.mkdirSync)((0, import_node_path22.dirname)(path7), { recursive: true });
  (0, import_node_fs21.writeFileSync)(path7, JSON.stringify(store2), "utf8");
}
function openWithUsageKeysForPath(targetPath) {
  try {
    const stat2 = (0, import_node_fs21.statSync)(targetPath);
    if (stat2.isDirectory()) {
      return [`folder:${targetPath}`, `sibling-folder:${(0, import_node_path22.dirname)(targetPath)}`];
    }
  } catch {
  }
  const ext = (0, import_node_path22.extname)(targetPath).toLowerCase();
  const parent = (0, import_node_path22.dirname)(targetPath);
  const keys = [`parent:${parent}`];
  if (ext) {
    keys.push(`parent-ext:${parent}:${ext}`, `ext:${ext}`);
  }
  return keys;
}
function recordOpenWithUsage(targetPath, appName) {
  const cleanAppName = appName.trim();
  if (!targetPath || !cleanAppName) return;
  try {
    const store2 = readOpenWithUsageStore();
    const now = Date.now();
    for (const key of openWithUsageKeysForPath(targetPath)) {
      const bucket = store2.keys[key] ??= {};
      const existing = bucket[cleanAppName];
      bucket[cleanAppName] = {
        count: (existing?.count ?? 0) + 1,
        lastUsedAt: now
      };
    }
    writeOpenWithUsageStore(store2);
  } catch (error) {
    console.warn("[Search] Failed to record open-with usage:", error);
  }
}
function normalizeAppSearchTerm(value) {
  return value.trim().toLowerCase().replace(/&/g, "and").replace(/[^a-z0-9]+/g, "");
}
function appAcronym(name) {
  return name.split(/[^a-zA-Z0-9]+/).filter(Boolean).map((part) => part[0]?.toLowerCase() ?? "").join("");
}
function builtInAppAliases(appName) {
  const n = appName.toLowerCase();
  if (n === "visual studio code") return ["vscode", "vsc", "code", "vs code"];
  if (n === "quicktime player") return ["quicktime", "qt", "qtp"];
  if (n === "activity monitor") return ["activity", "taskmanager", "task monitor"];
  if (n === "terminal") return ["term", "shell"];
  if (n === "finder") return ["files", "filemanager"];
  return [];
}
function appMatchesTerm(appName, term, learnedAliases) {
  const normalizedTerm = normalizeAppSearchTerm(term);
  if (!normalizedTerm) return true;
  const normalizedName = normalizeAppSearchTerm(appName);
  if (normalizedName.includes(normalizedTerm)) return true;
  if (appAcronym(appName).includes(normalizedTerm)) return true;
  if (builtInAppAliases(appName).some(
    (alias) => normalizeAppSearchTerm(alias).includes(normalizedTerm)
  )) {
    return true;
  }
  return Boolean(learnedAliases?.[appName]);
}
function recordOpenWithAlias(term, appName) {
  const alias = normalizeAppSearchTerm(term);
  const cleanAppName = appName.trim();
  if (!alias || !cleanAppName) return;
  if (normalizeAppSearchTerm(cleanAppName).includes(alias)) return;
  try {
    const store2 = readOpenWithUsageStore();
    const aliases = store2.aliases ??= {};
    const bucket = aliases[alias] ??= {};
    const existing = bucket[cleanAppName];
    bucket[cleanAppName] = {
      count: (existing?.count ?? 0) + 1,
      lastUsedAt: Date.now()
    };
    writeOpenWithUsageStore(store2);
  } catch (error) {
    console.warn("[Search] Failed to record open-with alias:", error);
  }
}
function learnedAliasScores(term) {
  const alias = normalizeAppSearchTerm(term);
  if (!alias) return void 0;
  return readOpenWithUsageStore().aliases?.[alias];
}
function recommendedOpenWithApps(targetPath) {
  const store2 = readOpenWithUsageStore();
  const weights = /* @__PURE__ */ new Map();
  const now = Date.now();
  openWithUsageKeysForPath(targetPath).forEach((key, index) => {
    const bucket = store2.keys[key];
    if (!bucket) return;
    const keyWeight = index === 0 ? 5 : index === 1 ? 3 : 1;
    for (const [appName, entry] of Object.entries(bucket)) {
      const ageDays = Math.max(0, (now - entry.lastUsedAt) / 864e5);
      const recencyBoost = Math.max(0, 14 - ageDays);
      weights.set(appName, (weights.get(appName) ?? 0) + keyWeight * entry.count + recencyBoost);
    }
  });
  return Array.from(weights.entries()).map(([appName, score]) => ({ appName, score })).sort((a, b) => b.score - a.score || a.appName.localeCompare(b.appName));
}
function isApplicationsDirectory(path7) {
  const normalized = path7.replace(/\/+$/, "");
  return normalized === "/Applications" || normalized === "/System/Applications" || normalized === "/System/Applications/Utilities" || normalized === (0, import_node_path22.join)((0, import_node_os11.homedir)(), "Applications");
}
function inferredDefaultAppName(targetPath) {
  try {
    if ((0, import_node_fs21.statSync)(targetPath).isDirectory()) return "Finder";
  } catch {
  }
  const ext = (0, import_node_path22.extname)(targetPath).toLowerCase();
  const parent = (0, import_node_path22.dirname)(targetPath).toLowerCase();
  if (/\b(movie|movies|video|videos)\b/.test(parent) && [".ts", ".m2ts", ".mts"].includes(ext)) {
    return "QuickTime Player";
  }
  if ([".png", ".jpg", ".jpeg", ".gif", ".heic", ".webp", ".tiff", ".bmp", ".pdf"].includes(ext)) {
    return "Preview";
  }
  if ([".mov", ".mp4", ".m4v", ".avi", ".mkv", ".m2ts", ".mts"].includes(ext)) {
    return "QuickTime Player";
  }
  return "Default App";
}
function applicationCompletionItem(targetPath, appInfo, index, section, score) {
  return {
    id: `path-app:${section}:${appInfo.path}`,
    title: appInfo.name,
    subtitle: `Open ${displayUserPath(targetPath)} with ${appInfo.name}`,
    kind: "application",
    section,
    badge: section === "recommended" ? "Recommended" : "Open With",
    value: `${targetPath} ${appInfo.name}`,
    path: targetPath,
    appPath: appInfo.path,
    appName: appInfo.name,
    applicationAction: "open-with",
    score: score - index
  };
}
function installedApplicationItem(appInfo, index) {
  return {
    id: `path-installed-app:${appInfo.path}`,
    title: appInfo.name,
    subtitle: displayUserPath(appInfo.path),
    kind: "application",
    badge: "Application",
    value: appInfo.path,
    path: appInfo.path,
    appPath: appInfo.path,
    appName: appInfo.name,
    applicationAction: "open",
    score: 2e3 - index
  };
}
async function completePath(query, limit = 50) {
  const applicationQuery = query.trimStart();
  if (applicationQuery.startsWith("`")) {
    const appTerm2 = applicationQuery.slice(1).trim();
    const apps = listApplications().filter((item) => appMatchesTerm(item.name, appTerm2)).sort((a, b) => a.name.localeCompare(b.name)).slice(0, limit);
    return apps.map((item, index) => installedApplicationItem(item, index));
  }
  const { targetPath, appTerm, appMode } = splitPathCompletionQuery(query);
  if (!appMode && isApplicationsDirectory(targetPath)) {
    const apps = listApplications().sort((a, b) => a.name.localeCompare(b.name)).slice(0, limit);
    return apps.map((item, index) => installedApplicationItem(item, index));
  }
  if (appMode) {
    const learnedAliases = learnedAliasScores(appTerm);
    const allApps = listApplications().sort((a, b) => a.name.localeCompare(b.name)).filter((item) => appMatchesTerm(item.name, appTerm, learnedAliases));
    const appsByName = new Map(allApps.map((item) => [item.name, item]));
    const learnedRecommended = Object.entries(learnedAliases ?? {}).sort((a, b) => b[1].count - a[1].count || b[1].lastUsedAt - a[1].lastUsedAt).map(([appName]) => appsByName.get(appName)).filter(isPresent);
    const usageRecommended = recommendedOpenWithApps(targetPath).map((item) => appsByName.get(item.appName)).filter(isPresent);
    const recommended = [...learnedRecommended, ...usageRecommended].filter(
      (item, index, items) => items.findIndex((other) => other.name === item.name) === index
    ).slice(0, 5);
    const recommendedNames = new Set(recommended.map((item) => item.name));
    const rest = allApps.filter((item) => !recommendedNames.has(item.name)).slice(0, limit);
    const recommendedItems = recommended.map(
      (item, index) => applicationCompletionItem(targetPath, item, index, "recommended", 4e3)
    );
    const appItems = rest.slice(0, Math.max(0, limit - recommendedItems.length - 1)).map(
      (item, index) => applicationCompletionItem(targetPath, item, index, "applications", 1e3)
    );
    const defaultItem = {
      id: `path-default:${targetPath}`,
      title: `Open in ${inferredDefaultAppName(targetPath)}`,
      subtitle: `Open ${displayUserPath(targetPath)}`,
      kind: "application",
      section: "default",
      badge: "Default",
      value: `${targetPath} `,
      path: targetPath,
      applicationAction: "open-with",
      score: 2e3
    };
    if (appTerm.trim()) {
      return [...recommendedItems, ...appItems, defaultItem];
    }
    return [...recommendedItems, defaultItem, ...appItems];
  }
  const { directory, prefix } = pathCompletionBase(targetPath);
  const normalizedPrefix = prefix.toLowerCase();
  try {
    const entries = (0, import_node_fs21.readdirSync)(directory, { withFileTypes: true });
    const recommended = query.trim() === "/" ? recommendedDirectories() : [];
    const recommendedPaths = new Set(recommended.map((item) => item.path));
    const regular = entries.filter((entry) => !entry.name.startsWith(".")).filter((entry) => !normalizedPrefix || entry.name.toLowerCase().includes(normalizedPrefix)).map((entry) => {
      const absolute = (0, import_node_path22.join)(directory, entry.name);
      const isDirectory = entry.isDirectory();
      const isFile = entry.isFile();
      if (!isDirectory && !isFile) return null;
      const kind = isDirectory ? "directory" : "file";
      const lowerName = entry.name.toLowerCase();
      return {
        id: `path:${absolute}`,
        title: entry.name,
        subtitle: displayUserPath(absolute),
        kind,
        value: isDirectory ? `${absolute}/` : absolute,
        path: absolute,
        iconDataUrl: isDirectory ? folderIconDataUrl : fileIconDataUrl(absolute),
        score: (isDirectory ? 1e3 : 500) + (lowerName === normalizedPrefix ? 1e3 : lowerName.startsWith(normalizedPrefix) ? 100 : 0)
      };
    }).filter(isPresent).filter((item) => !recommendedPaths.has(item.path)).sort((a, b) => {
      if (a.kind !== b.kind) return a.kind === "directory" ? -1 : 1;
      return b.score - a.score || a.title.localeCompare(b.title);
    }).slice(0, Math.max(0, limit - recommended.length));
    return [...recommended, ...regular];
  } catch {
    return [];
  }
}
async function searchPortManagerOpenPorts(query) {
  const normalizedQuery = query.trim().toLowerCase();
  if (!normalizedQuery) return [];
  const mentionsPort = /(port|ports|open|listen|listening)/.test(normalizedQuery);
  const mentionsKill = /(kill|stop|terminate|process)/.test(normalizedQuery);
  if (!/(port|ports|open|listen|kill|\d{2,5})/.test(normalizedQuery)) {
    return [];
  }
  const processes = await listOpenPorts();
  if (processes.length === 0) return [];
  return processes.flatMap(
    (entry) => entry.ports.map((port) => {
      let score = -1;
      const processName = entry.process.toLowerCase();
      const userName = entry.user.toLowerCase();
      if (normalizedQuery.includes(String(port))) {
        score = 430;
      } else if (mentionsPort || mentionsKill) {
        score = 280;
      } else if (processName.includes(normalizedQuery) || userName.includes(normalizedQuery)) {
        score = 220;
      }
      if (score < 0) return null;
      return {
        id: `port-listener:${entry.pid}:${port}`,
        title: `Open Port ${port}`,
        subtitle: `${entry.process} (PID ${entry.pid}) \xB7 ${entry.user} \xB7 Enter to kill listener`,
        category: "extensions",
        score,
        action: {
          type: "run-extension-command",
          extensionId: "raycast.port-manager",
          commandName: "kill-listening-process",
          title: "Kill Process Listening On",
          argumentValues: { port: String(port) }
        }
      };
    })
  ).filter(isPresent).sort((a, b) => b.score - a.score).slice(0, 12);
}
function buildEmojiPickerSearchResult(query) {
  const n = query.trim().toLowerCase();
  if (!n) return [];
  const emojiActionStats = indexDb.getActionStats(["native:open-emoji-picker"]).get("native:open-emoji-picker");
  const recentUseBoost = (() => {
    if (!emojiActionStats?.lastUsedAt) return 0;
    const ageMs = Date.now() - emojiActionStats.lastUsedAt;
    if (ageMs < 5 * 60 * 1e3) return 1e3;
    if (ageMs < 60 * 60 * 1e3) return 550;
    if (ageMs < 24 * 60 * 60 * 1e3) return 180;
    return 0;
  })();
  const shortPrefixBoost = n === "e" ? 920 : n.startsWith("em") ? 760 : n.startsWith("emo") ? 920 : 0;
  const shouldShow = n.includes("emoji") || n.startsWith("emo") || n === "e" || n.includes("smiley") || n.includes("emoticon") || n.includes("symbol") || n === "/emoji";
  if (!shouldShow) return [];
  return [
    {
      id: "native:open-emoji-picker",
      title: "Emoji Picker",
      subtitle: "Browse and copy emojis by name, mood, and category.",
      category: "native-command",
      score: 2600 + shortPrefixBoost + recentUseBoost,
      action: { type: "run-native-command", commandId: "open-emoji-picker" }
    }
  ];
}
async function listOpenPorts() {
  try {
    const { stdout } = await execFileAsync11("/usr/sbin/lsof", ["-nP", "-iTCP", "-sTCP:LISTEN"]);
    const processNames = await readProcessNameMap();
    return parseOpenPortProcesses(stdout, processNames);
  } catch (error) {
    console.error("[OpenPorts] Failed to list listening ports:", error);
    try {
      const { stdout } = await execFileAsync11("/usr/sbin/lsof", ["-nP", "-iTCP", "-sTCP:LISTEN"]);
      return parseOpenPortProcesses(stdout);
    } catch (fallbackError) {
      console.error("[OpenPorts] Fallback listing failed:", fallbackError);
      return [];
    }
  }
}
async function executeActionInner(action) {
  switch (action.type) {
    case "open-app": {
      await execFileAsync11("open", ["-a", action.appName]);
      return { ok: true, message: `Opened ${action.appName}` };
    }
    case "open-file": {
      const opened = await shell.openPath(action.path);
      if (opened) {
        return { ok: false, message: opened };
      }
      return { ok: true, message: "Opened file" };
    }
    case "open-with-app": {
      if (action.appName) {
        await execFileAsync11("open", ["-a", action.appName, action.path]);
        recordOpenWithUsage(action.path, action.appName);
        return { ok: true, message: `Opened with ${action.appName}` };
      }
      const opened = await shell.openPath(action.path);
      if (opened) {
        return { ok: false, message: opened };
      }
      return { ok: true, message: "Opened" };
    }
    case "copy-text": {
      clipboard.writeText(action.text);
      return { ok: true, message: "Copied to clipboard" };
    }
    case "copy-and-paste-text": {
      clipboard.writeText(action.text);
      await new Promise((resolve4) => setTimeout(resolve4, 120));
      app.hide();
      await new Promise((resolve4) => setTimeout(resolve4, 50));
      await execFileAsync11("osascript", [
        "-e",
        'tell application "System Events" to keystroke "v" using {command down}'
      ]);
      return { ok: true, message: "Pasted emoji" };
    }
    case "add-note": {
      const entry = addQuickNote(action.text);
      await reindexQuickNotes();
      return entry ? { ok: true, message: "Saved to Quick Notes" } : { ok: false, message: "Could not save quick note" };
    }
    case "open-url": {
      await shell.openExternal(action.url);
      return { ok: true, message: "Opened URL" };
    }
    case "install-extension": {
      await installExtension(action.extensionId);
      return { ok: true, message: `Installing ${action.extensionId}` };
    }
    case "run-extension-command": {
      const argumentValues = {
        ...action.argumentValues ?? {}
      };
      if (action.argumentName && action.argumentValue && !argumentValues[action.argumentName]) {
        argumentValues[action.argumentName] = action.argumentValue;
      }
      try {
        const result = await executeExtensionCommandRuntime(
          action.extensionId,
          action.commandName,
          argumentValues
        );
        return result;
      } catch (error) {
        if (isUnsupportedRuntimeModeError(error)) {
          return {
            ok: false,
            message: "This extension command requires view runtime support. Use extension:run-command to render it in the Tezbar extension surface."
          };
        }
        throw error;
      }
    }
    case "invoke-command": {
      return commandBus.execute({
        commandId: action.commandId,
        payload: action.payload
      });
    }
    case "run-shell": {
      const command = String(action.command ?? "").trim();
      const validation = validateShellCommand(command);
      if (!validation.ok) {
        return { ok: false, message: validation.message };
      }
      const { stdout } = await execFileAsync11("bash", ["-lc", command]);
      const message = stdout.trim();
      return { ok: true, message: message || "Command completed" };
    }
    case "run-native-command": {
      return executeNativeCommand(action.commandId);
    }
    default: {
      return { ok: false, message: "Unsupported action type" };
    }
  }
}
var _benchmarkPromise = null;
async function runSearchBenchmarks() {
  if (_benchmarkPromise) {
    return _benchmarkPromise;
  }
  _benchmarkPromise = (async () => {
    await indexDb.ensureInitialized();
    await runOfflineBenchmarks(searchEverything, indexDb);
  })();
  return _benchmarkPromise;
}
async function getSearchBenchmarkHistory() {
  return readBenchmarkHistory();
}
async function executeSearchAction(action, context) {
  let result;
  try {
    const safety = safetyForAction(action);
    result = safety ? await runWithSafety(safety.id, safety.context, () => executeActionInner(action)) : await executeActionInner(action);
  } catch (error) {
    result = {
      ok: false,
      message: error instanceof Error ? error.message : String(error)
    };
  }
  const actionId = actionIdFromResult(action, context?.resultId);
  indexDb.recordAction(actionId, result.ok);
  if (result.ok && action.type === "open-with-app" && action.appName && context?.query) {
    const parsed = splitPathCompletionQuery(context.query);
    if (parsed.appMode && parsed.appTerm) {
      recordOpenWithAlias(parsed.appTerm, action.appName);
    }
  }
  if (context?.query && typeof context.rank === "number" && Number.isFinite(context.rank)) {
    indexDb.recordClick(context.query, actionId, context.rank, result.ok);
  }
  return result;
}

// src/main/currency/frankfurter.ts
var cachedBase = null;
var cachedRates = null;
var cachedDate = "";
var cachedFetchedAt = 0;
var CACHE_TTL_MS = 6 * 60 * 60 * 1e3;
var TAG = "[currency/main]";
async function fetchOpenErApi(base) {
  const url = `https://open.er-api.com/v6/latest/${encodeURIComponent(base)}`;
  console.debug(TAG, "fetch", url);
  const res = await fetch(url);
  console.debug(TAG, "response", res.status);
  if (!res.ok) {
    throw new Error(`open.er-api HTTP ${res.status}`);
  }
  const data = await res.json();
  if (data.result && data.result !== "success") {
    throw new Error(`open.er-api error: ${data.result}`);
  }
  const rates = data.rates;
  if (!rates || typeof rates !== "object") {
    throw new Error("open.er-api: invalid response");
  }
  const date = typeof data.time_last_update_utc === "string" ? data.time_last_update_utc : "";
  return { base, date, rates };
}
async function fetchFrankfurter(base) {
  const url = `https://api.frankfurter.app/latest?from=${encodeURIComponent(base)}`;
  console.debug(TAG, "fetch fallback", url);
  const res = await fetch(url);
  console.debug(TAG, "response (frankfurter)", res.status);
  if (!res.ok) {
    throw new Error(`Frankfurter HTTP ${res.status}`);
  }
  const data = await res.json();
  const rates = data.rates;
  if (!rates || typeof rates !== "object") {
    throw new Error("Frankfurter: invalid response");
  }
  return {
    base,
    date: typeof data.date === "string" ? data.date : "",
    rates
  };
}
async function fetchFrankfurterLatest(from) {
  const a = from.trim().toUpperCase();
  if (!/^[A-Z]{3}$/.test(a)) {
    throw new Error(`Invalid currency code: ${from}`);
  }
  const now = Date.now();
  if (cachedBase === a && cachedRates && now - cachedFetchedAt < CACHE_TTL_MS) {
    console.debug(TAG, "cache hit", { base: a, date: cachedDate });
    return { base: a, date: cachedDate, rates: cachedRates };
  }
  let payload;
  try {
    payload = await fetchOpenErApi(a);
  } catch (primaryErr) {
    console.warn(TAG, "open.er-api failed \u2014 falling back to Frankfurter", primaryErr);
    payload = await fetchFrankfurter(a);
  }
  cachedBase = a;
  cachedRates = payload.rates;
  cachedDate = payload.date;
  cachedFetchedAt = now;
  console.debug(TAG, "ok", {
    base: a,
    date: payload.date,
    sampleRUB: payload.rates.RUB,
    sampleKZT: payload.rates.KZT,
    sampleEUR: payload.rates.EUR
  });
  return payload;
}

// src/main/portManager/namedPortsStore.ts
var import_node_crypto8 = require("node:crypto");
var import_node_fs22 = require("node:fs");
var import_node_path23 = require("node:path");
init_electron_shim();
function storePath() {
  return `${app.getPath("userData")}/named-ports.json`;
}
function readAll() {
  const path7 = storePath();
  if (!(0, import_node_fs22.existsSync)(path7)) return [];
  try {
    const raw = (0, import_node_fs22.readFileSync)(path7, "utf8");
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.map((row) => {
      if (!row || typeof row !== "object") return null;
      const o = row;
      const id = typeof o.id === "string" ? o.id : "";
      const name = typeof o.name === "string" ? o.name.trim() : "";
      const port = typeof o.port === "number" ? o.port : Number(o.port);
      if (!id || !name || !Number.isFinite(port) || port < 1 || port > 65535) return null;
      return { id, name, port: Math.floor(port) };
    }).filter((x) => x !== null);
  } catch {
    return [];
  }
}
function writeAll(entries) {
  const path7 = storePath();
  (0, import_node_fs22.mkdirSync)((0, import_node_path23.dirname)(path7), { recursive: true });
  (0, import_node_fs22.writeFileSync)(path7, `${JSON.stringify(entries, null, 2)}
`, "utf8");
}
function listNamedPorts() {
  return readAll().sort((a, b) => a.name.localeCompare(b.name) || a.port - b.port);
}
function addNamedPort(name, port) {
  const trimmed = name.trim();
  if (!trimmed || port < 1 || port > 65535) return null;
  const entries = readAll();
  const next = { id: (0, import_node_crypto8.randomUUID)(), name: trimmed, port };
  entries.push(next);
  writeAll(entries);
  return next;
}
function removeNamedPort(id) {
  const entries = readAll();
  const next = entries.filter((e) => e.id !== id);
  if (next.length === entries.length) return false;
  writeAll(next);
  return true;
}

// src/main/llm/actionMode.ts
init_registry();
function redactContext(input) {
  return input.replace(/(sk-[A-Za-z0-9]{12,})/g, "[REDACTED_API_KEY]").replace(/(gh[pousr]_[A-Za-z0-9_]{12,})/g, "[REDACTED_TOKEN]").replace(/(password\s*[=:]\s*[^\s]+)/gi, "password=[REDACTED]");
}
async function runAiActionMode(req, options) {
  const provider = getProviderForTask("action");
  const selectedText = req.selectedText ? req.redactSensitive === false ? req.selectedText : redactContext(req.selectedText) : "";
  const appContext = req.appContext ? req.redactSensitive === false ? req.appContext : redactContext(req.appContext) : "";
  const messages = [
    {
      role: "system",
      content: "You are Tezbar Action Mode. Produce concise, executable steps and concrete output. Never execute system actions unless explicitly allowed."
    },
    {
      role: "user",
      content: [
        `Instruction: ${req.instruction}`,
        req.allowAutomation ? "Automation permission: granted" : "Automation permission: denied",
        selectedText ? `Selected text:
${selectedText}` : "Selected text: (none)",
        appContext ? `App context:
${appContext}` : "App context: (none)"
      ].join("\n\n")
    }
  ];
  const stream = await provider.chat(messages, void 0, { signal: options?.signal });
  let output = "";
  for await (const delta of stream) {
    if (delta.text) output += delta.text;
  }
  return {
    ok: true,
    output: output.trim()
  };
}

// src/main/voice/service.ts
init_electron_shim();
var import_node_child_process13 = require("node:child_process");
var import_node_fs23 = require("node:fs");
var import_node_fs24 = require("node:fs");
var import_node_path24 = require("node:path");
var import_node_events2 = require("node:events");
var import_node_util12 = require("node:util");
init_configStore();
var execFileAsync12 = (0, import_node_util12.promisify)(import_node_child_process13.execFile);
var activeSpeech = null;
var cachedLoginPath = null;
async function getLoginPath() {
  if (cachedLoginPath !== null) return cachedLoginPath;
  try {
    const { stdout } = await execFileAsync12("bash", ["-lc", 'echo -n "$PATH"']);
    const fromShell = stdout.trim();
    cachedLoginPath = fromShell || process.env["PATH"] || "";
  } catch {
    cachedLoginPath = process.env["PATH"] || "";
  }
  const extras = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"];
  const existing = new Set(cachedLoginPath.split(":").filter(Boolean));
  for (const e of extras) {
    if (!existing.has(e)) cachedLoginPath += `:${e}`;
  }
  return cachedLoginPath;
}
async function execWithUserPath(file, args, options = {}) {
  const path7 = await getLoginPath();
  return execFileAsync12(file, args, {
    maxBuffer: options.maxBuffer ?? 16 * 1024 * 1024,
    env: { ...process.env, PATH: path7 }
  });
}
var MODEL_CATALOG = [
  {
    id: "moonshine-base-en",
    name: "Moonshine Base (English)",
    family: "moonshine",
    description: "Low-latency Moonshine STT model from Moonshine AI.",
    homepageUrl: "https://github.com/moonshine-ai/moonshine",
    estimatedSizeMb: 140,
    runtime: "moonshine-python",
    assets: [
      {
        fileName: "encoder_model.ort",
        url: "https://download.moonshine.ai/model/base-en/quantized/base-en/encoder_model.ort"
      },
      {
        fileName: "decoder_model_merged.ort",
        url: "https://download.moonshine.ai/model/base-en/quantized/base-en/decoder_model_merged.ort"
      },
      {
        fileName: "tokenizer.bin",
        url: "https://download.moonshine.ai/model/base-en/quantized/base-en/tokenizer.bin"
      }
    ]
  },
  {
    id: "whisper-base",
    name: "Whisper Base (English, whisper.cpp)",
    family: "whisper",
    description: "Fast whisper.cpp ggml model \u2014 good for quick dictation.",
    homepageUrl: "https://huggingface.co/ggerganov/whisper.cpp",
    estimatedSizeMb: 150,
    runtime: "whisper-cpp",
    assets: [
      {
        fileName: "ggml-base.en.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin"
      }
    ]
  },
  {
    id: "whisper-small",
    name: "Whisper Small (English, whisper.cpp)",
    family: "whisper",
    description: "Higher-accuracy whisper.cpp ggml model \u2014 a bit slower, noticeably better.",
    homepageUrl: "https://huggingface.co/ggerganov/whisper.cpp",
    estimatedSizeMb: 490,
    runtime: "whisper-cpp",
    assets: [
      {
        fileName: "ggml-small.en.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin"
      }
    ]
  }
];
var activeDownloads = /* @__PURE__ */ new Map();
var VOICE_MODEL_CONFIG_KEY = "voiceSttModelId";
function voiceModelsRootDir() {
  const dir = (0, import_node_path24.join)(app.getPath("userData"), "voice-models");
  (0, import_node_fs23.mkdirSync)(dir, { recursive: true });
  return dir;
}
function modelDir(modelId) {
  return (0, import_node_path24.join)(voiceModelsRootDir(), modelId);
}
function modelAssetPath(modelId, fileName) {
  return (0, import_node_path24.join)(modelDir(modelId), fileName);
}
function findModel(modelId) {
  const model = MODEL_CATALOG.find((entry) => entry.id === modelId);
  if (!model) {
    throw new Error(`Unknown voice model: ${modelId}`);
  }
  return model;
}
function readSelectedModelId() {
  const config = readRawConfig();
  const raw = config[VOICE_MODEL_CONFIG_KEY];
  if (raw === "moonshine-base-en" || raw === "whisper-base" || raw === "whisper-small") {
    return raw;
  }
  return "moonshine-base-en";
}
function fileSizeOrZero(path7) {
  try {
    return (0, import_node_fs23.statSync)(path7).size;
  } catch {
    return 0;
  }
}
function modelDownloadedBytes(model) {
  return model.assets.reduce((acc, asset) => acc + fileSizeOrZero(modelAssetPath(model.id, asset.fileName)), 0);
}
function isModelFullyDownloaded(model) {
  return model.assets.every((asset) => {
    const path7 = modelAssetPath(model.id, asset.fileName);
    return (0, import_node_fs23.existsSync)(path7) && fileSizeOrZero(path7) > 0;
  });
}
async function probeRuntime(kind) {
  if (kind === "whisper-cpp") {
    const ready2 = await hasBinary("whisper-cli") || await hasBinary("whisper-cpp");
    return {
      ready: ready2,
      label: "whisper.cpp",
      installCommand: "brew install whisper-cpp"
    };
  }
  const python = await hasBinary("python3");
  if (!python) {
    return {
      ready: false,
      label: "Moonshine (Python)",
      installCommand: "brew install python && python3 -m pip install --user moonshine-voice",
      message: "python3 was not found on your PATH."
    };
  }
  const ready = await hasMoonshinePython();
  return {
    ready,
    label: "Moonshine (Python)",
    installCommand: "python3 -m pip install --user moonshine-voice onnxruntime"
  };
}
async function runLoginShell(command) {
  const path7 = await getLoginPath();
  const { stdout, stderr } = await execFileAsync12("bash", ["-lc", command], {
    maxBuffer: 32 * 1024 * 1024,
    env: {
      ...process.env,
      PATH: path7,
      HOMEBREW_NO_AUTO_UPDATE: "1",
      HOMEBREW_NO_ANALYTICS: "1",
      HOMEBREW_NO_INSTALL_CLEANUP: "1",
      PIP_DISABLE_PIP_VERSION_CHECK: "1"
    }
  });
  return { stdout, stderr };
}
async function installRuntime(kind) {
  if (kind === "whisper-cpp") {
    if (!await hasBinary("brew")) {
      throw new Error(
        "Homebrew is required to install whisper.cpp automatically.\nInstall Homebrew from https://brew.sh and try again, or install whisper.cpp manually:\n  brew install whisper-cpp"
      );
    }
    console.info("[stt][main] installing whisper-cpp via Homebrew \u2014 this can take a few minutes");
    try {
      const { stdout, stderr } = await runLoginShell("brew install whisper-cpp");
      console.info("[stt][main] brew stdout:\n" + stdout.trim());
      if (stderr.trim()) console.info("[stt][main] brew stderr:\n" + stderr.trim());
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      throw new Error(`brew install whisper-cpp failed: ${msg}`);
    }
    return;
  }
  if (!await hasBinary("python3")) {
    throw new Error(
      "python3 was not found. Install Python first (e.g. `brew install python`) and try again."
    );
  }
  console.info("[stt][main] installing moonshine-voice via pip (user site) \u2014 this can take a minute");
  try {
    const { stdout, stderr } = await runLoginShell(
      "python3 -m pip install --user --upgrade moonshine-voice onnxruntime"
    );
    console.info("[stt][main] pip stdout:\n" + stdout.trim());
    if (stderr.trim()) console.info("[stt][main] pip stderr:\n" + stderr.trim());
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    throw new Error(`pip install moonshine-voice failed: ${msg}`);
  }
}
function buildEmptyRuntimeView(kind) {
  return {
    label: kind === "whisper-cpp" ? "whisper.cpp" : "Moonshine (Python)",
    ready: false,
    installCommand: kind === "whisper-cpp" ? "brew install whisper-cpp" : "python3 -m pip install --user moonshine-voice onnxruntime"
  };
}
var runtimeCache = /* @__PURE__ */ new Map();
async function cachedProbeRuntime(kind) {
  const cached = runtimeCache.get(kind);
  if (cached) return cached;
  const probe = await probeRuntime(kind);
  runtimeCache.set(kind, probe);
  return probe;
}
function invalidateRuntimeCache(kind) {
  runtimeCache.delete(kind);
}
async function toVoiceModelView(model, selectedId) {
  const active2 = activeDownloads.get(model.id);
  const weightsDownloaded = isModelFullyDownloaded(model);
  const diskBytes = modelDownloadedBytes(model);
  const runtime = await cachedProbeRuntime(model.runtime);
  const runtimeView = {
    label: runtime.label,
    ready: runtime.ready,
    installCommand: runtime.installCommand,
    message: runtime.message
  };
  const selected = model.id === selectedId;
  if (weightsDownloaded && runtime.ready) {
    return {
      id: model.id,
      name: model.name,
      family: model.family,
      description: model.description,
      homepageUrl: model.homepageUrl,
      estimatedSizeMb: model.estimatedSizeMb,
      status: "downloaded",
      stage: "idle",
      progress: 1,
      downloadedBytes: diskBytes,
      totalBytes: diskBytes,
      selected,
      runtime: runtimeView
    };
  }
  if (active2) {
    return {
      id: model.id,
      name: model.name,
      family: model.family,
      description: model.description,
      homepageUrl: model.homepageUrl,
      estimatedSizeMb: model.estimatedSizeMb,
      status: active2.status,
      stage: active2.stage,
      progress: active2.progress,
      downloadedBytes: active2.downloadedBytes,
      totalBytes: active2.totalBytes,
      selected,
      errorMessage: active2.errorMessage,
      runtime: runtimeView
    };
  }
  return {
    id: model.id,
    name: model.name,
    family: model.family,
    description: model.description,
    homepageUrl: model.homepageUrl,
    estimatedSizeMb: model.estimatedSizeMb,
    status: "not-downloaded",
    stage: "idle",
    progress: 0,
    downloadedBytes: diskBytes,
    totalBytes: null,
    selected,
    runtime: runtimeView
  };
}
async function downloadAssetWithProgress(url, destinationPath, onProgress) {
  const response = await fetch(url, { method: "GET", redirect: "follow" });
  if (!response.ok || !response.body) {
    throw new Error(`Download failed (${response.status}): ${url}`);
  }
  await import_node_fs24.promises.mkdir((0, import_node_path24.dirname)(destinationPath), { recursive: true });
  const tempPath = `${destinationPath}.part`;
  const total = Number(response.headers.get("content-length") ?? "");
  const totalBytes = Number.isFinite(total) && total > 0 ? total : null;
  const writer = (0, import_node_fs23.createWriteStream)(tempPath);
  const reader = response.body.getReader();
  let downloaded = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      if (!value) continue;
      downloaded += value.byteLength;
      if (!writer.write(Buffer.from(value))) {
        await (0, import_node_events2.once)(writer, "drain");
      }
      onProgress(downloaded, totalBytes);
    }
    writer.end();
    await (0, import_node_events2.once)(writer, "finish");
    await import_node_fs24.promises.rename(tempPath, destinationPath);
  } catch (error) {
    writer.destroy();
    await import_node_fs24.promises.rm(tempPath, { force: true });
    throw error;
  }
}
async function runModelDownload(modelId) {
  const model = findModel(modelId);
  const destinationRoot = modelDir(modelId);
  await import_node_fs24.promises.mkdir(destinationRoot, { recursive: true });
  let baselineBytes = modelDownloadedBytes(model);
  const runtimeNeeded = !(await probeRuntime(model.runtime)).ready;
  const missingAssets = model.assets.filter((asset) => !(0, import_node_fs23.existsSync)(modelAssetPath(modelId, asset.fileName)));
  if (!runtimeNeeded && missingAssets.length === 0) {
    activeDownloads.delete(modelId);
    return;
  }
  activeDownloads.set(modelId, {
    status: "downloading",
    stage: runtimeNeeded ? "installing-runtime" : "downloading-weights",
    downloadedBytes: baselineBytes,
    totalBytes: null,
    progress: null
  });
  try {
    if (runtimeNeeded) {
      await installRuntime(model.runtime);
      invalidateRuntimeCache(model.runtime);
      const reProbe = await probeRuntime(model.runtime);
      if (!reProbe.ready) {
        throw new Error(
          `Installed the runtime but could not detect it afterwards (${reProbe.label}). Try running manually: ${reProbe.installCommand}`
        );
      }
      activeDownloads.set(modelId, {
        status: "downloading",
        stage: "downloading-weights",
        downloadedBytes: baselineBytes,
        totalBytes: null,
        progress: null
      });
    }
    for (const asset of missingAssets) {
      const destination = modelAssetPath(modelId, asset.fileName);
      await downloadAssetWithProgress(asset.url, destination, (assetBytes, assetTotal) => {
        const state2 = activeDownloads.get(modelId);
        if (!state2 || state2.status !== "downloading") return;
        const downloadedBytes = baselineBytes + assetBytes;
        const progress = assetTotal && assetTotal > 0 ? Math.min(assetBytes / assetTotal, 0.999) : null;
        activeDownloads.set(modelId, {
          ...state2,
          stage: "downloading-weights",
          downloadedBytes,
          totalBytes: assetTotal,
          progress
        });
      });
      baselineBytes = modelDownloadedBytes(model);
      const state = activeDownloads.get(modelId);
      if (state && state.status === "downloading") {
        activeDownloads.set(modelId, {
          ...state,
          stage: "downloading-weights",
          downloadedBytes: baselineBytes,
          totalBytes: null,
          progress: null
        });
      }
    }
    activeDownloads.delete(modelId);
    invalidateRuntimeCache(model.runtime);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.error("[stt][main] model provisioning failed:", message);
    activeDownloads.set(modelId, {
      status: "error",
      stage: runtimeNeeded ? "installing-runtime" : "downloading-weights",
      downloadedBytes: baselineBytes,
      totalBytes: null,
      progress: null,
      errorMessage: message
    });
  }
}
function getSelectedVoiceModelId() {
  return readSelectedModelId();
}
function setSelectedVoiceModelId(modelId) {
  findModel(modelId);
  writeConfigPatch({ [VOICE_MODEL_CONFIG_KEY]: modelId });
  return modelId;
}
async function listVoiceModels() {
  await cleanupStaleVoiceModelAssets();
  try {
    const selected = readSelectedModelId();
    return Promise.all(MODEL_CATALOG.map((model) => toVoiceModelView(model, selected)));
  } catch (err) {
    console.warn("[stt][main] listVoiceModels fallback:", err instanceof Error ? err.message : err);
    return MODEL_CATALOG.map((model, index) => ({
      id: model.id,
      name: model.name,
      family: model.family,
      description: model.description,
      homepageUrl: model.homepageUrl,
      estimatedSizeMb: model.estimatedSizeMb,
      status: "not-downloaded",
      stage: "idle",
      progress: 0,
      downloadedBytes: 0,
      totalBytes: null,
      selected: index === 0,
      runtime: buildEmptyRuntimeView(model.runtime)
    }));
  }
}
var LEGACY_WHISPER_ASSET_NAMES = [
  "model.safetensors",
  "config.json",
  "generation_config.json",
  "merges.txt",
  "preprocessor_config.json",
  "tokenizer.json",
  "tokenizer_config.json",
  "vocab.json"
];
var staleVoiceCleanupPromise = null;
async function cleanupStaleVoiceModelAssets() {
  if (staleVoiceCleanupPromise) return staleVoiceCleanupPromise;
  staleVoiceCleanupPromise = (async () => {
    const whisperModels = MODEL_CATALOG.filter((model) => model.family === "whisper");
    for (const model of whisperModels) {
      const currentAssets = new Set(model.assets.map((asset) => asset.fileName));
      for (const fileName of LEGACY_WHISPER_ASSET_NAMES) {
        if (currentAssets.has(fileName)) continue;
        const fullPath = modelAssetPath(model.id, fileName);
        try {
          await import_node_fs24.promises.stat(fullPath);
          await import_node_fs24.promises.rm(fullPath, { force: true });
          console.log("[stt][main] removed stale voice model asset:", fullPath);
        } catch {
        }
      }
    }
  })();
  return staleVoiceCleanupPromise;
}
async function downloadVoiceModel(modelId) {
  const model = findModel(modelId);
  const active2 = activeDownloads.get(modelId);
  if (!active2 || active2.status !== "downloading") {
    activeDownloads.set(modelId, {
      status: "downloading",
      stage: "installing-runtime",
      downloadedBytes: modelDownloadedBytes(model),
      totalBytes: null,
      progress: null
    });
    void runModelDownload(modelId);
  }
  const selected = readSelectedModelId();
  return toVoiceModelView(model, selected);
}
async function speakText(text) {
  const trimmed = text.trim();
  if (!trimmed) return;
  stopSpeaking();
  activeSpeech = (0, import_node_child_process13.spawn)("say", [trimmed], {
    stdio: "ignore"
  });
  activeSpeech.on("exit", () => {
    activeSpeech = null;
  });
}
function stopSpeaking() {
  if (!activeSpeech) return;
  activeSpeech.kill("SIGTERM");
  activeSpeech = null;
}
async function hasBinary(binary) {
  try {
    const path7 = await getLoginPath();
    await execFileAsync12("bash", ["-lc", `command -v ${binary}`], {
      env: { ...process.env, PATH: path7 }
    });
    return true;
  } catch {
    return false;
  }
}
async function listSttModes() {
  const modes = [];
  const models = await listVoiceModels();
  if (models.some((model) => model.status === "downloaded")) {
    modes.push("local-model-assets");
  }
  if (await hasBinary("whisper-cli")) {
    modes.push("local-whisper-cli");
  } else if (await hasBinary("whisper-cpp")) {
    modes.push("local-whisper-cpp");
  }
  if (await hasMoonshinePython()) {
    modes.push("local-moonshine-python");
  }
  return modes;
}
async function hasMoonshinePython() {
  try {
    await execWithUserPath("python3", ["-c", "import moonshine_voice"]);
    return true;
  } catch {
    return false;
  }
}
function preferredMoonshineModelPath() {
  const selected = readSelectedModelId();
  if (selected === "moonshine-base-en" && isModelFullyDownloaded(findModel(selected))) {
    return modelDir(selected);
  }
  const fallback = findModel("moonshine-base-en");
  if (isModelFullyDownloaded(fallback)) {
    return modelDir("moonshine-base-en");
  }
  throw new Error("Moonshine model files are not downloaded yet. Download Moonshine Base first.");
}
async function convertToWav(inputPath, outputPath) {
  await execFileAsync12("ffmpeg", [
    "-y",
    "-i",
    inputPath,
    "-ac",
    "1",
    "-ar",
    "16000",
    "-f",
    "wav",
    outputPath
  ]);
}
async function runMoonshineTranscription(wavPath, language) {
  const modelPath = preferredMoonshineModelPath();
  const lang = (language || "en").trim() || "en";
  const script = [
    "import json, sys, traceback",
    "try:",
    "    from moonshine_voice import Transcriber, ModelArch",
    "    try:",
    "        from moonshine_voice.utils import load_wav_file",
    "    except ImportError:",
    "        from moonshine_voice import load_wav_file",
    "",
    "    wav_path = sys.argv[1]",
    "    model_path = sys.argv[2]",
    '    language = sys.argv[3] if len(sys.argv) > 3 else "en"',
    "",
    "    lower_path = model_path.lower()",
    '    if "tiny" in lower_path and hasattr(ModelArch, "TINY"):',
    "        arch = ModelArch.TINY",
    '    elif "small" in lower_path and hasattr(ModelArch, "SMALL_STREAMING"):',
    "        arch = ModelArch.SMALL_STREAMING",
    '    elif hasattr(ModelArch, "BASE"):',
    "        arch = ModelArch.BASE",
    "    else:",
    "        # Very old versions exposed only integer model archs.",
    "        arch = 1",
    "",
    "    options = {}",
    '    if language.lower() not in ("en", "english", "es", "spanish"):',
    '        options["max_tokens_per_second"] = "13.0"',
    "",
    "    transcriber = Transcriber(",
    "        model_path=model_path,",
    "        model_arch=arch,",
    "        options=options or None,",
    "    )",
    "    try:",
    "        audio_data, sample_rate = load_wav_file(wav_path)",
    "        transcript = transcriber.transcribe_without_streaming(",
    "            audio_data=audio_data,",
    "            sample_rate=sample_rate,",
    "        )",
    '        lines = getattr(transcript, "lines", None) or []',
    '        text = " ".join((getattr(l, "text", "") or "").strip() for l in lines).strip()',
    "        sys.stdout.write(text)",
    "        sys.stdout.flush()",
    "    finally:",
    "        try:",
    "            transcriber.close()",
    "        except Exception:",
    "            pass",
    "except Exception as exc:",
    "    traceback.print_exc(file=sys.stderr)",
    '    sys.stderr.write("\\nMOONSHINE_ERROR: " + repr(exc) + "\\n")',
    "    sys.exit(1)",
    ""
  ].join("\n");
  console.info("[stt][main] invoking moonshine-voice, model=", modelPath, "lang=", lang);
  try {
    const { stdout, stderr } = await execWithUserPath("python3", ["-c", script, wavPath, modelPath, lang]);
    if (stderr.trim()) {
      console.info("[stt][main] moonshine stderr:\n" + stderr.trim());
    }
    return stdout.trim();
  } catch (err) {
    const e = err;
    const detail = (e.stderr ?? "").trim() || (e.stdout ?? "").trim() || e.message || String(err);
    throw new Error(detail);
  }
}
function extensionFromMime(mime) {
  if (!mime) return "bin";
  if (mime.includes("wav")) return "wav";
  if (mime.includes("webm")) return "webm";
  if (mime.includes("ogg")) return "ogg";
  if (mime.includes("mp4") || mime.includes("m4a") || mime.includes("aac")) return "m4a";
  if (mime.includes("mpeg") || mime.includes("mp3")) return "mp3";
  if (mime.includes("flac")) return "flac";
  return "bin";
}
async function findWhisperCliModel() {
  const envPath = process.env["RAYMES_WHISPER_MODEL"];
  if (envPath && (0, import_node_fs23.existsSync)(envPath)) return envPath;
  const selected = readSelectedModelId();
  const preferredDirs = selected === "whisper-base" || selected === "whisper-small" ? [modelDir(selected), modelDir(selected === "whisper-base" ? "whisper-small" : "whisper-base")] : [modelDir("whisper-base"), modelDir("whisper-small")];
  for (const dir of preferredDirs) {
    try {
      const inner = await import_node_fs24.promises.readdir(dir);
      const match = inner.find((f) => f.startsWith("ggml-") && f.endsWith(".bin"));
      if (match) return (0, import_node_path24.join)(dir, match);
    } catch {
    }
  }
  const candidates = [
    "/opt/homebrew/share/whisper-cpp/ggml-base.en.bin",
    "/opt/homebrew/share/whisper-cpp/ggml-base.bin",
    "/usr/local/share/whisper-cpp/ggml-base.en.bin",
    "/usr/local/share/whisper-cpp/ggml-base.bin"
  ];
  for (const c of candidates) {
    if ((0, import_node_fs23.existsSync)(c)) return c;
  }
  return null;
}
async function runWhisperCli(wavPath, language) {
  const binary = await hasBinary("whisper-cli") ? "whisper-cli" : await hasBinary("whisper-cpp") ? "whisper-cpp" : null;
  if (!binary) return null;
  const model = await findWhisperCliModel();
  if (!model) return null;
  const args = ["-m", model, "-f", wavPath, "-l", language?.trim() || "en", "-otxt", "-of", wavPath.replace(/\.wav$/, "")];
  console.info("[stt][main] whisper-cli:", binary, args.join(" "));
  try {
    const { stderr } = await execWithUserPath(binary, args);
    if (stderr.trim()) console.info("[stt][main] whisper-cli stderr:\n" + stderr.trim());
    const txtPath = wavPath.replace(/\.wav$/, ".txt");
    const text = await import_node_fs24.promises.readFile(txtPath, "utf-8").catch(() => "");
    await import_node_fs24.promises.rm(txtPath, { force: true }).catch(() => void 0);
    return text.trim();
  } catch (err) {
    const e = err;
    const detail = (e.stderr ?? "").trim() || e.message || String(err);
    console.warn("[stt][main] whisper-cli failed:", detail);
    throw new Error(detail);
  }
}
var cachedEngineProbe = null;
var ENGINE_PROBE_TTL_MS = 5 * 60 * 1e3;
async function probeEngineBinaries() {
  if (cachedEngineProbe && Date.now() - cachedEngineProbe.cachedAt < ENGINE_PROBE_TTL_MS) {
    return cachedEngineProbe;
  }
  const [whichPython, whichWhisper, moonshinePath] = await Promise.all([
    execWithUserPath("bash", ["-lc", "command -v python3 || true"]).then((r) => r.stdout.trim()).catch(() => ""),
    execWithUserPath("bash", ["-lc", "command -v whisper-cli || command -v whisper-cpp || true"]).then((r) => r.stdout.trim()).catch(() => ""),
    execWithUserPath("python3", ["-c", "import moonshine_voice, sys; sys.stdout.write(moonshine_voice.__file__)"]).then((r) => r.stdout.trim()).catch(() => "")
  ]);
  cachedEngineProbe = {
    python3: whichPython,
    whisper: whichWhisper,
    moonshine: moonshinePath,
    cachedAt: Date.now()
  };
  return cachedEngineProbe;
}
async function transcribeAudio(req) {
  const tempRoot = (0, import_node_path24.join)(app.getPath("temp"), "tezbar-voice");
  await import_node_fs24.promises.mkdir(tempRoot, { recursive: true });
  const token = `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  const ext = extensionFromMime(req.mimeType);
  const sourcePath = (0, import_node_path24.join)(tempRoot, `input-${token}.${ext}`);
  const wavPath = ext === "wav" ? sourcePath : (0, import_node_path24.join)(tempRoot, `input-${token}.wav`);
  try {
    await import_node_fs24.promises.writeFile(sourcePath, Buffer.from(req.audioBytes));
    console.info(
      "[stt][main] received audio",
      JSON.stringify({
        bytes: req.audioBytes.byteLength,
        mime: req.mimeType ?? "unknown",
        language: req.language ?? "auto"
      })
    );
    try {
      const probe = await probeEngineBinaries();
      console.info("[stt][main] engine probe", JSON.stringify(probe));
    } catch (err) {
      console.info("[stt][main] engine probe skipped:", err instanceof Error ? err.message : err);
    }
    if (ext !== "wav") {
      if (!await hasBinary("ffmpeg")) {
        return {
          ok: false,
          error: "Audio arrived in a compressed format but ffmpeg is not installed.",
          hint: "Install ffmpeg (`brew install ffmpeg`) or enable Web Audio encoding in the renderer."
        };
      }
      await convertToWav(sourcePath, wavPath);
    }
    let firstFailure = null;
    const whisperAvailable = await hasBinary("whisper-cli") || await hasBinary("whisper-cpp");
    if (whisperAvailable) {
      try {
        const whisperText = await runWhisperCli(wavPath, req.language);
        if (whisperText && whisperText.length > 0) {
          console.info("[stt][main] whisper-cli produced", whisperText.length, "chars");
          return { ok: true, text: whisperText, engine: "whisper-cli" };
        }
        if (whisperText !== null && !firstFailure) {
          firstFailure = { engine: "whisper.cpp", message: "whisper-cli returned no text." };
        }
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        console.warn("[stt][main] whisper-cli threw:", message);
        if (!firstFailure) firstFailure = { engine: "whisper.cpp", message };
      }
    }
    const moonshineAvailable = await hasBinary("python3") && await hasMoonshinePython();
    if (moonshineAvailable) {
      try {
        const text = await runMoonshineTranscription(wavPath, req.language);
        if (text.length > 0) {
          console.info("[stt][main] moonshine produced", text.length, "chars");
          return { ok: true, text, engine: "moonshine-python" };
        }
        if (!firstFailure) {
          firstFailure = { engine: "Moonshine", message: "Moonshine returned no text." };
        }
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        console.warn("[stt][main] moonshine failed:", message);
        if (!firstFailure) firstFailure = { engine: "Moonshine", message };
      }
    }
    if (firstFailure) {
      return {
        ok: false,
        error: `${firstFailure.engine} failed to transcribe the recording.`,
        hint: firstFailure.message
      };
    }
    return {
      ok: false,
      error: "No local speech-to-text engine is available.",
      hint: 'Open Settings \u2192 Voice models and click "Install & download" on a model. Tezbar will install the required runtime (whisper.cpp via Homebrew or Moonshine via pip) and the weights in one step.'
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    console.error("[stt][main] transcription pipeline error:", message);
    return { ok: false, error: message };
  } finally {
    await import_node_fs24.promises.rm(sourcePath, { force: true }).catch(() => void 0);
    if (wavPath !== sourcePath) {
      await import_node_fs24.promises.rm(wavPath, { force: true }).catch(() => void 0);
    }
  }
}

// src/main/permissions/manager.ts
init_electron_shim();
var DESCRIPTORS3 = {
  accessibility: {
    id: "accessibility",
    title: "Accessibility",
    summary: "Synthesize keystrokes, control windows, automate UI.",
    rationale: "Needed to automate the active app: move windows, click through menus, send keystrokes to focused controls.",
    settingsUrl: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
    remediation: "Open System Settings \u2192 Privacy & Security \u2192 Accessibility and enable Tezbar."
  },
  automation: {
    id: "automation",
    title: "Automation (Apple Events)",
    summary: "Talk to other apps via AppleScript / Apple Events.",
    rationale: "Required for AppleScript-based commands (toggle dark mode, empty trash, control Music/Finder).",
    settingsUrl: "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation",
    remediation: "Open System Settings \u2192 Privacy & Security \u2192 Automation and allow Tezbar to control the target app."
  },
  "input-monitoring": {
    id: "input-monitoring",
    title: "Input Monitoring",
    summary: "Observe keyboard and mouse events globally.",
    rationale: "Used for global hotkeys and key-capture flows (e.g. global Alt+Space, keystroke recording).",
    settingsUrl: "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
    remediation: "Open System Settings \u2192 Privacy & Security \u2192 Input Monitoring and enable Tezbar."
  },
  microphone: {
    id: "microphone",
    title: "Microphone",
    summary: "Capture audio for voice commands.",
    rationale: "Voice-activated commands and transcription features require microphone access.",
    settingsUrl: "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
    remediation: "Open System Settings \u2192 Privacy & Security \u2192 Microphone and enable Tezbar."
  },
  calendar: {
    id: "calendar",
    title: "Calendar",
    summary: "Read and create events for calendar-aware commands.",
    rationale: 'Calendar-related extensions and the built-in "next meeting" command need access to your Calendar database.',
    settingsUrl: "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars",
    remediation: "Open System Settings \u2192 Privacy & Security \u2192 Calendars and enable Tezbar."
  },
  "screen-recording": {
    id: "screen-recording",
    title: "Screen Recording",
    summary: "Capture screen content for screenshots and window vision.",
    rationale: "Needed for screenshot-based flows, window snapshots, and visual automation helpers.",
    settingsUrl: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
    remediation: "Open System Settings \u2192 Privacy & Security \u2192 Screen Recording and enable Tezbar."
  }
};
function mapMediaStatus(value) {
  switch (value) {
    case "granted":
      return "granted";
    case "denied":
      return "denied";
    case "restricted":
      return "restricted";
    case "not-determined":
    case "unknown":
      return "not-determined";
    default:
      return "not-determined";
  }
}
function probeAccessibility(promptIfNeeded = false) {
  if (process.platform !== "darwin") return "unsupported";
  try {
    return systemPreferences.isTrustedAccessibilityClient(promptIfNeeded) ? "granted" : "denied";
  } catch {
    return "not-determined";
  }
}
function getStatus(type) {
  if (process.platform !== "darwin") return "unsupported";
  try {
    return mapMediaStatus(
      systemPreferences.getMediaAccessStatus(type)
    );
  } catch {
    return "not-determined";
  }
}
function probePermission(id) {
  switch (id) {
    case "accessibility":
      return probeAccessibility(false);
    case "microphone":
      return getStatus("microphone");
    case "calendar":
      return getStatus("calendar");
    case "screen-recording":
      return getStatus("screen");
    case "automation":
    case "input-monitoring":
      return process.platform === "darwin" ? "not-determined" : "unsupported";
    default:
      return "unsupported";
  }
}
function snapshotPermissions() {
  const statuses = Object.keys(DESCRIPTORS3).map((id) => ({
    descriptor: DESCRIPTORS3[id],
    state: probePermission(id),
    checkedAt: Date.now()
  }));
  return {
    platform: process.platform,
    statuses
  };
}
async function requestPermission(id) {
  const descriptor = DESCRIPTORS3[id];
  if (!descriptor) {
    throw new Error(`Unknown permission: ${id}`);
  }
  if (process.platform !== "darwin") {
    return { descriptor, state: "unsupported", checkedAt: Date.now() };
  }
  try {
    if (id === "accessibility") {
      systemPreferences.isTrustedAccessibilityClient(true);
    } else if (id === "microphone") {
      await systemPreferences.askForMediaAccess("microphone");
    } else if (descriptor.settingsUrl) {
      await shell.openExternal(descriptor.settingsUrl);
    }
  } catch {
  }
  return { descriptor, state: probePermission(id), checkedAt: Date.now() };
}

// src/main/terminal/service.ts
var import_node_fs25 = require("node:fs");
var import_node_os12 = require("node:os");
var import_node_path25 = require("node:path");
var import_node_crypto9 = require("node:crypto");
var import_node_child_process14 = require("node:child_process");
var import_node_module3 = require("node:module");

// src/shared/terminal.ts
var TERMINAL_IPC = {
  CREATE: "terminal:create",
  WRITE: "terminal:write",
  RESIZE: "terminal:resize",
  KILL: "terminal:kill",
  DATA: "terminal:data",
  EXIT: "terminal:exit",
  GET_PROMPT_INFO: "terminal:get-prompt-info"
};

// src/main/terminal/service.ts
var requireNative = (0, import_node_module3.createRequire)(__filename);
function spawnBunPipeTerminal(shell2, args, cwd, env, cols, rows) {
  const bun = globalThis.Bun;
  if (!bun) return spawnPipeTerminal(shell2, args, cwd, env, cols, rows);
  const child = bun.spawn([shell2, ...args], {
    cwd,
    env,
    stdin: "pipe",
    stdout: "pipe",
    stderr: "pipe"
  });
  const dataListeners = /* @__PURE__ */ new Set();
  const pump = async (stream) => {
    const reader = stream.getReader();
    const decoder = new TextDecoder();
    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        const text = decoder.decode(value, { stream: true });
        if (text) dataListeners.forEach((listener) => listener(text));
      }
    } catch {
    } finally {
      reader.releaseLock();
    }
  };
  void pump(child.stdout);
  void pump(child.stderr);
  return {
    pid: child.pid,
    process: shell2,
    cols,
    rows,
    handleFlowControl: false,
    onData: (listener) => {
      dataListeners.add(listener);
      return { dispose: () => dataListeners.delete(listener) };
    },
    onExit: (listener) => {
      let active2 = true;
      void child.exited.then((exitCode) => {
        if (active2) listener({ exitCode, signal: 0 });
      });
      return { dispose: () => {
        active2 = false;
      } };
    },
    write: (data) => {
      child.stdin.write(data);
      void child.stdin.flush?.();
    },
    resize: () => void 0,
    clear: () => void 0,
    pause: () => void 0,
    resume: () => void 0,
    kill: () => child.kill()
  };
}
function spawnPipeTerminal(shell2, args, cwd, env, cols, rows) {
  const child = (0, import_node_child_process14.spawn)(shell2, args, { cwd, env, stdio: ["pipe", "pipe", "pipe"] });
  return {
    pid: child.pid ?? -1,
    process: shell2,
    cols,
    rows,
    handleFlowControl: false,
    onData: (listener) => {
      const onData = (chunk) => listener(chunk.toString("utf8"));
      child.stdout.on("data", onData);
      child.stderr.on("data", onData);
      return { dispose: () => {
        child.stdout.off("data", onData);
        child.stderr.off("data", onData);
      } };
    },
    onExit: (listener) => {
      const onExit = (exitCode, signal) => {
        listener({ exitCode: exitCode ?? 1, signal: signal ? 1 : 0 });
      };
      child.once("exit", onExit);
      return { dispose: () => child.off("exit", onExit) };
    },
    write: (data) => {
      child.stdin.write(data);
    },
    resize: () => void 0,
    clear: () => void 0,
    pause: () => child.stdout.pause(),
    resume: () => child.stdout.resume(),
    kill: () => {
      child.kill();
    }
  };
}
var sessions2 = /* @__PURE__ */ new Map();
var ownerCleanupRegistered = /* @__PURE__ */ new Set();
function clampDimension(value, min, max) {
  if (!Number.isFinite(value)) return min;
  return Math.min(Math.max(Math.floor(value), min), max);
}
function resolveWorkingDirectory(raw) {
  const requested = raw?.trim();
  const expanded = requested === "~" ? (0, import_node_os12.homedir)() : requested?.startsWith("~/") ? (0, import_node_path25.join)((0, import_node_os12.homedir)(), requested.slice(2)) : requested;
  const candidate = expanded ? (0, import_node_path25.resolve)(expanded) : (0, import_node_os12.homedir)();
  try {
    return (0, import_node_fs25.existsSync)(candidate) && (0, import_node_fs25.statSync)(candidate).isDirectory() ? candidate : (0, import_node_os12.homedir)();
  } catch {
    return (0, import_node_os12.homedir)();
  }
}
function resolveShell() {
  const configured = process.env.SHELL?.trim();
  if (configured && configured.startsWith("/") && (0, import_node_fs25.existsSync)(configured)) return configured;
  return process.platform === "win32" ? "powershell.exe" : "/bin/zsh";
}
function terminalEnvironment() {
  const env = {};
  for (const [key, value] of Object.entries(process.env)) {
    if (typeof value === "string") env[key] = value;
  }
  env.TERM = "xterm-256color";
  env.COLORTERM = "truecolor";
  env.TERM_PROGRAM = "Tezbar";
  return env;
}
function killOwnerSessions(ownerId) {
  for (const [sessionId, session2] of sessions2) {
    if (session2.ownerId !== ownerId) continue;
    sessions2.delete(sessionId);
    try {
      session2.process.kill();
    } catch {
    }
  }
}
function sessionForOwner(sessionId, ownerId) {
  const session2 = sessions2.get(sessionId);
  return session2?.ownerId === ownerId ? session2 : null;
}
function pipeInputEcho(data) {
  if (data === "\x7F") return "\b \b";
  if (data.startsWith("\x1B")) return "";
  return data.replace(/\r/g, "\r\n").replace(/[^\x20-\x7e\r\n\b\t]/g, "");
}
function createTerminalSession(sender, request) {
  const sessionId = (0, import_node_crypto9.randomUUID)();
  const cwd = resolveWorkingDirectory(request.cwd);
  const shell2 = resolveShell();
  const cols = clampDimension(request.cols, 2, 500);
  const rows = clampDimension(request.rows, 2, 300);
  const args = process.platform === "win32" ? [] : ["-l"];
  const env = terminalEnvironment();
  const ptyProcess = process.versions.bun ? spawnBunPipeTerminal(shell2, args, cwd, env, cols, rows) : requireNative("node-pty").spawn(shell2, args, {
    name: "xterm-256color",
    cols,
    rows,
    cwd,
    env
  });
  sessions2.set(sessionId, {
    ownerId: sender.id,
    sender,
    process: ptyProcess,
    pipeMode: Boolean(process.versions.bun)
  });
  if (!ownerCleanupRegistered.has(sender.id)) {
    ownerCleanupRegistered.add(sender.id);
    sender.once("destroyed", () => {
      ownerCleanupRegistered.delete(sender.id);
      killOwnerSessions(sender.id);
    });
  }
  ptyProcess.onData((data) => {
    if (!sender.isDestroyed()) {
      sender.send(TERMINAL_IPC.DATA, { sessionId, data });
    }
  });
  ptyProcess.onExit(({ exitCode, signal }) => {
    sessions2.delete(sessionId);
    if (!sender.isDestroyed()) {
      sender.send(TERMINAL_IPC.EXIT, { sessionId, exitCode, signal });
    }
  });
  if (request.initialCommand) {
    ptyProcess.write(`${request.initialCommand}${process.versions.bun ? "\n" : "\r"}`);
  }
  return { sessionId, shell: shell2, cwd };
}
function writeTerminalSession(ownerId, sessionId, data) {
  const session2 = sessionForOwner(sessionId, ownerId);
  if (!session2 || data.length === 0 || data.length > 64 * 1024) return false;
  if (session2.pipeMode) {
    const echo = pipeInputEcho(data);
    if (echo && !session2.sender.isDestroyed()) {
      session2.sender.send(TERMINAL_IPC.DATA, { sessionId, data: echo });
    }
  }
  session2.process.write(session2.pipeMode ? data.replace(/\r/g, "\n") : data);
  return true;
}
function resizeTerminalSession(ownerId, sessionId, cols, rows) {
  const session2 = sessionForOwner(sessionId, ownerId);
  if (!session2) return false;
  session2.process.resize(clampDimension(cols, 2, 500), clampDimension(rows, 2, 300));
  return true;
}
function killTerminalSession(ownerId, sessionId) {
  const session2 = sessionForOwner(sessionId, ownerId);
  if (!session2) return false;
  sessions2.delete(sessionId);
  try {
    session2.process.kill();
  } catch {
  }
  return true;
}
function getTerminalPromptInfo() {
  const user = (0, import_node_os12.userInfo)().username;
  const host = (0, import_node_os12.hostname)().split(".")[0];
  const dir = "~";
  return { user, host, dir };
}
function shutdownTerminalSessions() {
  for (const session2 of sessions2.values()) {
    try {
      session2.process.kill();
    } catch {
    }
  }
  sessions2.clear();
}

// src/main/storage/service.ts
init_electron_shim();
var import_promises3 = require("node:fs/promises");
var import_node_path26 = require("node:path");
async function dirSize(root) {
  let total = 0;
  const pending = [root];
  while (pending.length > 0) {
    const path7 = pending.pop();
    if (!path7) continue;
    try {
      const stats = await (0, import_promises3.lstat)(path7);
      if (stats.isSymbolicLink()) continue;
      if (stats.isFile()) {
        total += stats.size;
        continue;
      }
      if (stats.isDirectory()) {
        const entries = await (0, import_promises3.readdir)(path7, { withFileTypes: true });
        for (const entry of entries) {
          pending.push((0, import_node_path26.join)(path7, entry.name));
        }
      }
    } catch {
    }
  }
  return total;
}
async function fileSize(path7) {
  try {
    return (await (0, import_promises3.stat)(path7)).size;
  } catch {
    return 0;
  }
}
function userData(...segments) {
  return (0, import_node_path26.join)(app.getPath("userData"), ...segments);
}
async function getStorageBreakdown() {
  const searchDir = getClipboardStoreDir();
  const clipboardImagesDir = getClipboardImagesDir();
  const voiceModelsDir = userData("voice-models");
  const bunDir = userData("bun");
  const extensionsDir = userData("extensions");
  const cacheDir = userData("Cache");
  const codeCacheDir = userData("Code Cache");
  const [
    indexBytes,
    walBytes,
    shmBytes,
    clipboardJsonBytes,
    clipboardImagesBytes,
    voiceModelsBytes,
    bunBytes,
    extensionsBytes,
    cacheDirBytes,
    codeCacheBytes
  ] = await Promise.all([
    fileSize((0, import_node_path26.join)(searchDir, "index.sqlite3")),
    fileSize((0, import_node_path26.join)(searchDir, "index.sqlite3-wal")),
    fileSize((0, import_node_path26.join)(searchDir, "index.sqlite3-shm")),
    fileSize((0, import_node_path26.join)(searchDir, "clipboard.json")),
    dirSize(clipboardImagesDir),
    dirSize(voiceModelsDir),
    dirSize(bunDir),
    dirSize(extensionsDir),
    dirSize(cacheDir),
    dirSize(codeCacheDir)
  ]);
  const searchDbBytes = indexBytes + walBytes + shmBytes + clipboardJsonBytes;
  const cacheBytes = cacheDirBytes + codeCacheBytes;
  const items = [
    {
      id: "clipboard-images",
      label: "Clipboard images",
      bytes: clipboardImagesBytes,
      paths: [clipboardImagesDir]
    },
    {
      id: "search-db",
      label: "Search index & history",
      bytes: searchDbBytes,
      paths: [searchDir]
    },
    {
      id: "voice-models",
      label: "Voice models",
      bytes: voiceModelsBytes,
      paths: [voiceModelsDir]
    },
    { id: "bun", label: "Extension installer (Bun)", bytes: bunBytes, paths: [bunDir] },
    { id: "extensions", label: "Installed extensions", bytes: extensionsBytes, paths: [extensionsDir] },
    { id: "chromium-cache", label: "Chromium cache", bytes: cacheBytes, paths: [cacheDir, codeCacheDir] }
  ];
  return {
    totalBytes: items.reduce((sum, item) => sum + item.bytes, 0),
    items
  };
}
function getClipboardStorageConfig() {
  return getClipboardConfig();
}
function setClipboardStorageConfig(patch) {
  setClipboardConfig(patch);
  restartClipboardWatcher();
}
async function clearClipboardImages() {
  return clearClipboardImageHistory();
}
async function clearChromiumCache() {
  const defaultSession = session.defaultSession;
  if (!defaultSession) return;
  await defaultSession.clearCache();
  await defaultSession.clearStorageData({ storages: ["shadercache"] });
  await Promise.all(
    ["Code Cache", "GPUCache", "DawnCache", "GrShaderCache", "ShaderCache"].map(
      (name) => (0, import_promises3.rm)(userData(name), { recursive: true, force: true })
    )
  );
}
async function vacuumSearchDatabase() {
  const searchDir = getClipboardStoreDir();
  const walPath = (0, import_node_path26.join)(searchDir, "index.sqlite3-wal");
  const beforeBytes = await fileSize(walPath);
  try {
    const db = getInstance();
    await db.ensureInitialized();
    db.vacuum();
  } catch (err) {
    console.warn("[storage] Search DB vacuum failed:", err);
  }
  const afterBytes = await fileSize(walPath);
  return { beforeBytes, afterBytes };
}

// src/main/ipc.ts
var LLM_DEFAULTS = {
  uiStateRetentionMs: 6e4
};
var answerAbort = null;
var agentAbort = null;
var agentRunId = null;
var pendingAgentApprovals = /* @__PURE__ */ new Map();
var quitConfirmationOpen = false;
var quitConfirmed = false;
function quitRaymesNow() {
  quitConfirmed = true;
  globalShortcut.unregisterAll();
  BrowserWindow.getAllWindows().forEach((window2) => {
    if (!window2.isDestroyed()) window2.hide();
  });
  app.quit();
  setTimeout(() => {
    app.exit(0);
  }, 500);
}
async function confirmQuitRaymes(getWindow) {
  if (quitConfirmed) return true;
  if (quitConfirmationOpen) return false;
  quitConfirmationOpen = true;
  const win = getWindow();
  setSuppressBlurHide(true);
  try {
    if (win && !win.isDestroyed()) {
      win.show();
      win.focus();
    }
    app.focus({ steal: true });
    const result = win && !win.isDestroyed() ? await dialog.showMessageBox(win, {
      type: "question",
      buttons: ["Cancel", "Quit"],
      defaultId: 1,
      cancelId: 0,
      title: "Quit Tezbar",
      message: "Quit Tezbar?",
      detail: "Are you sure you want to quit Tezbar and terminate all background processes?",
      noLink: true
    }) : await dialog.showMessageBox({
      type: "question",
      buttons: ["Cancel", "Quit"],
      defaultId: 1,
      cancelId: 0,
      title: "Quit Tezbar",
      message: "Quit Tezbar?",
      detail: "Are you sure you want to quit Tezbar and terminate all background processes?",
      noLink: true
    });
    quitConfirmed = result.response === 1;
    return quitConfirmed;
  } finally {
    quitConfirmationOpen = false;
    if (!quitConfirmed) {
      setSuppressBlurHide(false);
    }
  }
}
var CHAT_SYSTEM_PROMPT = "You are Tezbar, a helpful assistant. Answer clearly and concisely unless the user asks for more detail.";
function sendAgentEvent(sender, event) {
  if (!sender.isDestroyed()) sender.send(AGENT_IPC.EVENT, event);
}
var PERSISTABLE_READ_COMMANDS = /* @__PURE__ */ new Set(["grep", "rg"]);
var SAFE_PIPELINE_COMMANDS = /* @__PURE__ */ new Set(["ps", "head", "tail", "wc"]);
function commandName(part) {
  const token = part.trim().split(/\s+/, 1)[0] ?? "";
  return token.slice(token.lastIndexOf("/") + 1).toLowerCase();
}
function suggestedApprovalRule(command) {
  if (/[;<>`\n]/.test(command) || command.includes("$(") || command.includes("||")) {
    return void 0;
  }
  const names = command.split(/\s*(?:&&|\|)\s*/).map(commandName).filter((name) => name && name !== "cd" && !SAFE_PIPELINE_COMMANDS.has(name));
  return names.find((name) => PERSISTABLE_READ_COMMANDS.has(name));
}
function cancelPendingAgentApprovals(runId) {
  for (const [approvalId, approval] of pendingAgentApprovals) {
    if (runId && approval.runId !== runId) continue;
    pendingAgentApprovals.delete(approvalId);
    approval.settle(false);
  }
}
function requestAgentApproval(sender, runId, signal, request) {
  const approvalId = `approval-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
  const suggestedRule = suggestedApprovalRule(request.command);
  return new Promise((resolve4) => {
    let settled = false;
    const timeout = setTimeout(() => settle(false), 5 * 6e4);
    const settle = (approved) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      pendingAgentApprovals.delete(approvalId);
      resolve4(approved);
    };
    pendingAgentApprovals.set(approvalId, { runId, suggestedRule, settle });
    signal.addEventListener("abort", () => settle(false), { once: true });
    sendAgentEvent(sender, {
      type: "approval",
      runId,
      approvalId,
      title: request.title,
      command: request.command,
      suggestedRule
    });
  });
}
function waitForRetry(ms, signal) {
  if (signal.aborted) return Promise.resolve();
  return new Promise((resolve4) => {
    const timer = setTimeout(resolve4, ms);
    signal.addEventListener(
      "abort",
      () => {
        clearTimeout(timer);
        resolve4();
      },
      { once: true }
    );
  });
}
function canRetryRun(error, signal) {
  if (signal.aborted) return false;
  const message = error instanceof Error ? error.message : String(error);
  return !/aborted|cancelled|task is empty|invalid base64|unsupported agent image/i.test(message);
}
function startAgentRun(sender, task, images = []) {
  cancelPendingAgentApprovals();
  agentAbort?.abort();
  agentAbort = new AbortController();
  const ac = agentAbort;
  const runId = agentRunId = `agent-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
  const bridge = getSharedBridge();
  const piProvider = getSelectedPiProviderBridge();
  let emittedOutput = false;
  sendAgentEvent(sender, { type: "start", runId, task });
  const onStage = (stage) => {
    emittedOutput = true;
    sendAgentEvent(sender, { type: "stage", runId, stage });
  };
  const onMessageDelta = (delta) => {
    emittedOutput = true;
    sendAgentEvent(sender, { type: "message", runId, delta });
  };
  const onAnswer = (text) => {
    emittedOutput = true;
    sendAgentEvent(sender, { type: "answer", runId, text });
  };
  const onStderrLine = (line) => {
    sendAgentEvent(sender, { type: "log", runId, source: "stderr", line });
  };
  console.log("[tezbar:agent] run", { runId, taskPreview: task.slice(0, 120) });
  void (async () => {
    let runTask = task;
    let runImages = images;
    if (images.length > 0 && piProvider && !piProvider.acceptsImages) {
      onStderrLine("The selected model is text-only. Extracting screen text locally...");
      const extractedText = await extractTextFromAgentImages(images);
      if (!extractedText) {
        throw new Error(
          "The selected model cannot read images, and no text could be extracted from the attached screen. Choose a vision model or remove the attachment."
        );
      }
      runTask = `${task}

Text extracted locally from the attached screen:

${extractedText}`;
      runImages = [];
    }
    const options = {
      runId,
      model: piProvider?.modelPattern ?? getSelectedPiModelPattern(),
      raymesProviderJson: piProvider?.providerJson,
      raymesAlwaysAllowJson: JSON.stringify(getAgentAlwaysAllowedCommands()),
      requestApproval: (request) => requestAgentApproval(sender, runId, ac.signal, request),
      signal: ac.signal,
      onStage,
      onMessageDelta,
      onAnswer,
      onStderrLine,
      images: runImages
    };
    try {
      await bridge.run(runTask, options);
    } catch (error) {
      if (emittedOutput || !canRetryRun(error, ac.signal)) throw error;
      onStderrLine("Pi did not start cleanly. Retrying once...");
      await waitForRetry(350, ac.signal);
      if (ac.signal.aborted) throw error;
      await bridge.run(runTask, options);
    }
  })().then(() => {
    sendAgentEvent(sender, { type: "done", runId });
  }).catch((err) => {
    const message = err instanceof Error ? err.message : String(err);
    sendAgentEvent(sender, { type: "error", runId, message });
    sendAgentEvent(sender, { type: "done", runId });
  }).finally(() => {
    cancelPendingAgentApprovals(runId);
    if (agentAbort === ac) agentAbort = null;
    if (agentRunId === runId) agentRunId = null;
  });
  return runId;
}
function normalizeChatTurns(raw) {
  if (!Array.isArray(raw)) return null;
  const turns = [];
  for (const item of raw) {
    if (!item || typeof item !== "object") return null;
    const turn = item;
    if (typeof turn.id !== "string" || turn.role !== "user" && turn.role !== "assistant" || typeof turn.text !== "string" || typeof turn.createdAt !== "number") {
      return null;
    }
    turns.push({
      id: turn.id,
      role: turn.role,
      text: turn.text,
      createdAt: turn.createdAt
    });
  }
  return turns;
}
function normalizeChatAttachments(raw) {
  if (!Array.isArray(raw)) return void 0;
  const attachments = [];
  for (const item of raw.slice(0, 4)) {
    if (!item || typeof item !== "object") continue;
    const attachment = item;
    if (attachment.kind !== "image" || typeof attachment.name !== "string" || attachment.mimeType !== "image/png" && attachment.mimeType !== "image/jpeg" && attachment.mimeType !== "image/webp") {
      continue;
    }
    attachments.push({
      kind: "image",
      name: attachment.name.slice(0, 120),
      mimeType: attachment.mimeType,
      width: typeof attachment.width === "number" ? attachment.width : void 0,
      height: typeof attachment.height === "number" ? attachment.height : void 0
    });
  }
  return attachments.length > 0 ? attachments : void 0;
}
function startChatRun(sender, turns) {
  agentAbort?.abort();
  agentAbort = new AbortController();
  const ac = agentAbort;
  const runId = agentRunId = `chat-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
  const contextTurns = turns.slice(-CHAT_CONTEXT_MAX_TURNS);
  const messages = [
    { role: "system", content: CHAT_SYSTEM_PROMPT },
    ...contextTurns.map((turn) => ({ role: turn.role, content: turn.text }))
  ];
  sendAgentEvent(sender, { type: "start", runId, task: turns.at(-1)?.text ?? "" });
  void (async () => {
    let fullText = "";
    try {
      let lastError;
      for (let attempt = 0; attempt < 2; attempt += 1) {
        try {
          const provider = getProviderForTask("chat");
          const stream = await provider.chat(messages, void 0, { signal: ac.signal });
          for await (const delta of stream) {
            if (ac.signal.aborted) return;
            if (delta.text) {
              fullText += delta.text;
              sendAgentEvent(sender, { type: "message", runId, delta: delta.text });
            }
          }
          lastError = void 0;
          break;
        } catch (error) {
          lastError = error;
          if (fullText || attempt > 0 || !canRetryRun(error, ac.signal)) throw error;
          await waitForRetry(350, ac.signal);
        }
      }
      if (lastError) throw lastError;
      if (!fullText.trim())
        throw new Error("The model returned an empty response. Please try again.");
      sendAgentEvent(sender, { type: "answer", runId, text: fullText });
    } catch (err) {
      if (!ac.signal.aborted) {
        sendAgentEvent(sender, {
          type: "error",
          runId,
          message: err instanceof Error ? err.message : String(err)
        });
      }
    } finally {
      if (!ac.signal.aborted) sendAgentEvent(sender, { type: "done", runId });
      if (agentAbort === ac) agentAbort = null;
      if (agentRunId === runId) agentRunId = null;
    }
  })();
  return runId;
}
function shutdownIpcHandlers() {
  answerAbort?.abort();
  cancelPendingAgentApprovals();
  agentAbort?.abort();
  clearAllExtensionSessions();
  disposeSharedBridge();
  shutdownTerminalSessions();
}
function registerIpcHandlers(getWindow, controls) {
  extensionRegistryEvents.on("progress", (payload) => {
    const win = getWindow();
    if (win && !win.isDestroyed()) {
      win.webContents.send("extension:install-progress", payload);
    }
  });
  ipcMain.handle("llm-config-get", async () => ({
    ...LLM_DEFAULTS,
    ...readLLMConfig(),
    ...readRawConfig(),
    uiStateRetentionMs: getUiStateRetentionMs()
  }));
  ipcMain.handle("llm-config-set", async (_event, patch) => {
    if (!patch || typeof patch !== "object") return;
    const configPatch = { ...patch };
    const requestedHotkey = configPatch.raymesHotkey;
    delete configPatch.raymesHotkey;
    if (typeof requestedHotkey === "string" && controls?.updateRaymesHotkey) {
      const result = controls.updateRaymesHotkey(requestedHotkey);
      if (!result.ok) return result;
      if (Object.keys(configPatch).length > 0) writeConfigPatch(configPatch);
      invalidateProviderCache();
      return result;
    }
    writeConfigPatch(configPatch);
    invalidateProviderCache();
  });
  ipcMain.handle("llm-provider-statuses", async () => {
    const cfg = readLLMConfig();
    const ids = [
      "openai",
      "openai-compatible",
      "anthropic",
      "ollama",
      "copilot",
      "gemini",
      "opencode",
      "deepseek"
    ];
    const entries = await Promise.all(
      ids.map(async (id) => {
        try {
          const ok = await buildProviderForId(id, cfg).isAvailable();
          return [id, ok];
        } catch {
          return [id, false];
        }
      })
    );
    return Object.fromEntries(entries);
  });
  ipcMain.handle("llm-list-models", async (_event, providerId) => {
    const id = providerId;
    const customProvider = typeof id === "string" && readLLMConfig().customProviders?.some((provider) => provider.id === id);
    if (id !== "openai" && id !== "openai-compatible" && id !== "anthropic" && id !== "ollama" && id !== "copilot" && id !== "gemini" && id !== "opencode" && id !== "deepseek" && !customProvider)
      return [];
    try {
      return await listModelsForProvider(id);
    } catch {
      return [];
    }
  });
  ipcMain.handle("window-set-content-height", async (_event, raw) => {
    const win = getWindow();
    if (!win || win.isDestroyed()) return;
    const payload = raw && typeof raw === "object" ? raw : { height: raw, zoomFactor: 1 };
    const height = typeof payload.height === "number" ? payload.height : Number(payload.height);
    const zoomFactor = typeof payload.zoomFactor === "number" ? payload.zoomFactor : Number(payload.zoomFactor);
    if (!Number.isFinite(height)) return;
    setLauncherContentHeight(win, height, zoomFactor);
  });
  ipcMain.handle("permissions:snapshot", async () => snapshotPermissions());
  ipcMain.handle("permissions:request", async (_event, raw) => {
    if (typeof raw !== "string") {
      throw new Error("Permission id must be a string");
    }
    return requestPermission(raw);
  });
  ipcMain.handle("safety:descriptors", async () => listSafetyDescriptors());
  ipcMain.handle("safety:log", async () => listSafetyLog());
  ipcMain.handle("safety:log-clear", async () => {
    clearSafetyLog();
  });
  ipcMain.handle("safety:dry-run:get", async () => getSafetyDryRun());
  ipcMain.handle("safety:dry-run:set", async (_event, raw) => {
    setSafetyDryRun(raw === true);
    return getSafetyDryRun();
  });
  ipcMain.handle("native-commands:list", async () => listNativeCommands());
  ipcMain.handle("clipboard:list", async () => listClipboardEntries());
  ipcMain.handle("clipboard:restore", async (_event, id) => {
    if (typeof id !== "string" || !id) return false;
    return restoreClipboardEntry(id);
  });
  ipcMain.handle("clipboard:delete", async (_event, id) => {
    if (typeof id !== "string" || !id) return false;
    return deleteClipboardEntry(id);
  });
  ipcMain.handle("clipboard:toggle-pin", async (_event, id) => {
    if (typeof id !== "string" || !id) return false;
    return togglePinClipboardEntry(id);
  });
  ipcMain.handle("clipboard:reveal", async (_event, id) => {
    if (typeof id !== "string" || !id) return false;
    return revealClipboardEntryInFinder(id);
  });
  ipcMain.handle("clipboard:image", async (_event, id) => {
    if (typeof id !== "string" || !id) return null;
    return readClipboardImagePayload(id);
  });
  ipcMain.handle("clipboard:clear", async () => {
    clearClipboardHistory();
  });
  ipcMain.handle("app-icon:data-url", async (_event, raw) => {
    const appPath = typeof raw === "string" ? raw.trim() : "";
    if (!appPath.endsWith(".app")) return null;
    return await appIconDataUrl(appPath) ?? null;
  });
  ipcMain.handle("asset-icon:data-url", async (_event, raw) => {
    if (!raw || typeof raw !== "object") return null;
    const payload = raw;
    const kind = payload.kind;
    const path7 = typeof payload.path === "string" ? payload.path.trim() : "";
    if (!path7) return null;
    if (kind === "application") {
      if (!path7.endsWith(".app")) return null;
      return await appIconDataUrl(path7) ?? null;
    }
    if (kind === "extension") {
      if (/^https?:\/\//i.test(path7)) return path7;
      return imageFileDataUrl(path7) ?? null;
    }
    if (kind === "file") {
      return await nativeFileIconDataUrl(path7) ?? null;
    }
    return null;
  });
  ipcMain.handle("snippets:list", async () => listSnippetsForUi());
  ipcMain.handle("snippets:copy", async (_event, id) => {
    if (typeof id !== "string" || !id) return { ok: false, message: "Invalid snippet" };
    return copySnippetById(id);
  });
  ipcMain.handle("snippets:add", async (_event, payload) => {
    if (!payload || typeof payload !== "object") return { ok: false, message: "Invalid payload" };
    const o = payload;
    const r = addUserSnippet({
      label: typeof o.label === "string" ? o.label : "",
      trigger: typeof o.trigger === "string" ? o.trigger : "",
      body: typeof o.body === "string" ? o.body : ""
    });
    if (r.ok) await reindexSnippets();
    return r;
  });
  ipcMain.handle("snippets:update", async (_event, id, payload) => {
    if (typeof id !== "string" || !id) return { ok: false, message: "Invalid snippet id" };
    if (!payload || typeof payload !== "object") return { ok: false, message: "Invalid payload" };
    const o = payload;
    const r = updateUserSnippet(id, {
      label: typeof o.label === "string" ? o.label : "",
      trigger: typeof o.trigger === "string" ? o.trigger : "",
      body: typeof o.body === "string" ? o.body : ""
    });
    if (r.ok) await reindexSnippets();
    return r;
  });
  ipcMain.handle("snippets:delete", async (_event, id) => {
    if (typeof id !== "string" || !id) return { ok: false, message: "Invalid snippet id" };
    const r = deleteUserSnippet(id);
    if (r.ok) await reindexSnippets();
    return r;
  });
  ipcMain.handle("notes:list", async () => listQuickNotes());
  ipcMain.handle("notes:append", async (_event, text) => {
    if (typeof text !== "string" || !text.trim()) return null;
    const entry = addQuickNote(text);
    await reindexQuickNotes();
    return entry;
  });
  ipcMain.handle("notes:update", async (_event, payload) => {
    if (!payload || typeof payload !== "object") return false;
    const o = payload;
    if (typeof o.createdAt !== "number" || typeof o.text !== "string") return false;
    const ok = updateQuickNote(o.createdAt, o.text);
    if (ok) await reindexQuickNotes();
    return ok;
  });
  ipcMain.handle("notes:delete", async (_event, createdAt) => {
    if (typeof createdAt !== "number") return false;
    const ok = deleteQuickNote(createdAt);
    if (ok) await reindexQuickNotes();
    return ok;
  });
  ipcMain.handle(TERMINAL_IPC.CREATE, async (event, raw) => {
    if (!raw || typeof raw !== "object") throw new Error("Invalid terminal request");
    const body = raw;
    if (typeof body.cols !== "number" || typeof body.rows !== "number") {
      throw new Error("Terminal dimensions are required");
    }
    if (body.cwd !== void 0 && typeof body.cwd !== "string") {
      throw new Error("Invalid terminal working directory");
    }
    if (body.initialCommand !== void 0 && typeof body.initialCommand !== "string") {
      throw new Error("Invalid initial terminal command");
    }
    if ((body.initialCommand?.length ?? 0) > 16 * 1024) {
      throw new Error("Initial terminal command is too long");
    }
    return createTerminalSession(event.sender, body);
  });
  ipcMain.handle(TERMINAL_IPC.WRITE, async (event, raw) => {
    if (!raw || typeof raw !== "object") return false;
    const body = raw;
    if (typeof body.sessionId !== "string" || typeof body.data !== "string") return false;
    return writeTerminalSession(event.sender.id, body.sessionId, body.data);
  });
  ipcMain.handle(TERMINAL_IPC.RESIZE, async (event, raw) => {
    if (!raw || typeof raw !== "object") return false;
    const body = raw;
    if (typeof body.sessionId !== "string" || typeof body.cols !== "number" || typeof body.rows !== "number") {
      return false;
    }
    return resizeTerminalSession(event.sender.id, body.sessionId, body.cols, body.rows);
  });
  ipcMain.handle(TERMINAL_IPC.KILL, async (event, raw) => {
    if (!raw || typeof raw !== "object") return false;
    const body = raw;
    if (typeof body.sessionId !== "string") return false;
    return killTerminalSession(event.sender.id, body.sessionId);
  });
  ipcMain.handle(TERMINAL_IPC.GET_PROMPT_INFO, async () => {
    return getTerminalPromptInfo();
  });
  ipcMain.handle("open-external-url", async (_event, url) => {
    if (typeof url !== "string") return;
    let parsed;
    try {
      parsed = new URL(url);
    } catch {
      return;
    }
    if (parsed.protocol !== "https:") return;
    if (parsed.hostname !== "github.com" && !parsed.hostname.endsWith(".github.com")) return;
    await shell.openExternal(url);
  });
  ipcMain.handle("github-device-start", async (_event, clientId) => {
    if (typeof clientId !== "string" || !clientId.trim()) {
      throw new Error("GitHub OAuth Client ID is required for device sign-in.");
    }
    return startGithubDeviceFlow(clientId.trim());
  });
  ipcMain.handle("github-device-poll", async () => {
    const r = await pollGithubDeviceFlow();
    if (r.status === "success") {
      persistCopilotTokens(r.access_token, r.refresh_token, r.expires_in);
      invalidateProviderCache();
    }
    return r;
  });
  ipcMain.handle("github-device-cancel", async () => {
    clearDeviceSession();
  });
  ipcMain.handle(IPC_CHANNELS.QUERY, async (event, input) => {
    const text = typeof input === "string" ? input : String(input ?? "");
    console.log("[IPC_CHANNELS.QUERY] received input:", text);
    const intent = await classifyIntent(text);
    console.log("[IPC_CHANNELS.QUERY] classified intent:", intent);
    if (intent.type === "answer" || intent.type === "ai") {
      console.log("[IPC_CHANNELS.QUERY] starting streamAnswerToRenderer");
      answerAbort?.abort();
      answerAbort = new AbortController();
      const ac = answerAbort;
      void streamAnswerToRenderer(event.sender, intent.input, ac.signal).finally(() => {
        if (answerAbort === ac) answerAbort = null;
      });
    }
    if (intent.type === "agent") {
      console.log("[IPC_CHANNELS.QUERY] starting startAgentRun");
      startAgentRun(event.sender, intent.input);
    }
    return intent;
  });
  ipcMain.handle("cancel", async () => {
    answerAbort?.abort();
    agentAbort?.abort();
  });
  ipcMain.handle(AGENT_IPC.RUN, async (event, raw) => {
    const request = typeof raw === "string" ? { task: raw } : raw && typeof raw === "object" ? raw : { task: "" };
    const task = typeof request.task === "string" ? request.task : "";
    if (!task.trim()) {
      return { ok: false, error: "Task is empty" };
    }
    const images = Array.isArray(request.images) ? request.images : [];
    const runId = startAgentRun(event.sender, task, images);
    return { ok: true, runId };
  });
  ipcMain.handle(AGENT_IPC.CAPTURE_ACTIVE_SCREEN, async (event) => {
    const display = screen.getDisplayNearestPoint(screen.getCursorScreenPoint());
    const maxWidth = 1600;
    const scale = Math.min(1, maxWidth / Math.max(1, display.size.width));
    const thumbnailSize = {
      width: Math.max(1, Math.round(display.size.width * scale)),
      height: Math.max(1, Math.round(display.size.height * scale))
    };
    const sourceWindow = BrowserWindow.fromWebContents(event.sender);
    const originalOpacity = sourceWindow?.getOpacity() ?? 1;
    try {
      sourceWindow?.setContentProtection(true);
      sourceWindow?.setOpacity(0);
      await new Promise((resolve4) => setTimeout(resolve4, 120));
      const sources = await desktopCapturer.getSources({ types: ["screen"], thumbnailSize });
      const source = sources.find((candidate) => candidate.display_id === String(display.id)) ?? sources[0];
      if (!source || source.thumbnail.isEmpty()) {
        throw new Error(
          "The active screen could not be captured. Check Screen Recording permission."
        );
      }
      const size = source.thumbnail.getSize();
      return {
        type: "image",
        data: source.thumbnail.toPNG().toString("base64"),
        mimeType: "image/png",
        width: size.width,
        height: size.height
      };
    } finally {
      if (sourceWindow && !sourceWindow.isDestroyed()) {
        sourceWindow.setOpacity(originalOpacity);
        sourceWindow.setContentProtection(false);
      }
    }
  });
  ipcMain.handle(AGENT_IPC.CANCEL, async () => {
    cancelPendingAgentApprovals(agentRunId ?? void 0);
    agentAbort?.abort();
    return { ok: true };
  });
  ipcMain.handle(AGENT_IPC.APPROVE, async (_event, raw) => {
    if (!raw || typeof raw !== "object") return { ok: false, error: "Invalid approval" };
    const response = raw;
    const decision = response.decision;
    if (typeof response.runId !== "string" || typeof response.approvalId !== "string" || decision !== "deny" && decision !== "once" && decision !== "always") {
      return { ok: false, error: "Invalid approval" };
    }
    const pending = pendingAgentApprovals.get(response.approvalId);
    if (!pending || pending.runId !== response.runId) {
      return { ok: false, error: "This approval is no longer active" };
    }
    if (decision === "always") {
      if (!pending.suggestedRule) {
        return { ok: false, error: "This command cannot be permanently allowed" };
      }
      addAgentAlwaysAllowedCommand(pending.suggestedRule);
    }
    pending.settle(decision !== "deny");
    return { ok: true };
  });
  ipcMain.handle(CHAT_IPC.RUN, async (event, rawTurns) => {
    const turns = normalizeChatTurns(rawTurns);
    if (!turns || turns.length === 0) {
      return { ok: false, error: "Invalid chat run payload" };
    }
    const runId = startChatRun(event.sender, turns);
    return { ok: true, runId };
  });
  ipcMain.handle(CHAT_IPC.LIST, async (_event, rawLimit) => {
    const limit = typeof rawLimit === "number" && Number.isFinite(rawLimit) && rawLimit > 0 ? Math.floor(rawLimit) : void 0;
    return listChatSessions(limit);
  });
  ipcMain.handle(CHAT_IPC.GET, async (_event, id) => {
    if (typeof id !== "string" || !id) return null;
    return getChatSession(id);
  });
  ipcMain.handle(CHAT_IPC.APPEND, async (_event, payload) => {
    if (!payload || typeof payload !== "object") {
      return { ok: false, error: "Invalid chat append payload" };
    }
    const body = payload;
    const s = body.session;
    const t = body.turn;
    if (!s || typeof s.id !== "string" || typeof s.title !== "string" || typeof s.createdAt !== "number" || typeof s.updatedAt !== "number") {
      return { ok: false, error: "Invalid session" };
    }
    if (!t || typeof t.id !== "string" || t.role !== "user" && t.role !== "assistant" || typeof t.text !== "string" || typeof t.createdAt !== "number") {
      return { ok: false, error: "Invalid turn" };
    }
    try {
      await upsertChatSession({
        id: s.id,
        title: s.title,
        createdAt: s.createdAt,
        updatedAt: s.updatedAt
      });
      await appendChatTurn(s.id, {
        id: t.id,
        role: t.role,
        text: t.text,
        stages: Array.isArray(t.stages) ? t.stages : void 0,
        error: typeof t.error === "string" ? t.error : void 0,
        attachments: normalizeChatAttachments(t.attachments),
        createdAt: t.createdAt
      });
      return { ok: true };
    } catch (err) {
      return { ok: false, error: err instanceof Error ? err.message : String(err) };
    }
  });
  ipcMain.handle(CHAT_IPC.UPDATE_TITLE, async (_event, payload) => {
    if (!payload || typeof payload !== "object") return { ok: false };
    const body = payload;
    if (typeof body.id !== "string" || typeof body.title !== "string") {
      return { ok: false };
    }
    try {
      await updateChatSessionTitle(body.id, body.title);
      return { ok: true };
    } catch {
      return { ok: false };
    }
  });
  ipcMain.handle(CHAT_IPC.DELETE, async (_event, id) => {
    if (typeof id !== "string" || !id) return { ok: false };
    return { ok: await deleteChatSession(id) };
  });
  ipcMain.handle(CHAT_IPC.CLEAR, async () => {
    try {
      await clearAllChatSessions();
      return { ok: true };
    } catch {
      return { ok: false };
    }
  });
  ipcMain.handle("app:confirm-quit", async () => confirmQuitRaymes(getWindow));
  ipcMain.on("app:quit-confirmed", () => {
    if (!quitConfirmed) return;
    quitRaymesNow();
  });
  ipcMain.on("app:request-quit", () => {
    void confirmQuitRaymes(getWindow).then((confirmed) => {
      if (confirmed) quitRaymesNow();
    }).catch(() => {
      setSuppressBlurHide(false);
    });
  });
  ipcMain.handle("get-extensions", async () => {
    return listInstalledExtensions();
  });
  ipcMain.handle("extensions:listInstalled", async () => {
    return listInstalledExtensions();
  });
  ipcMain.handle("extensions:searchStore", async (_event, query) => {
    const q = typeof query === "string" ? query : "";
    return searchStoreExtensions(q);
  });
  ipcMain.handle("extensions:install", async (_event, extensionId) => {
    if (typeof extensionId !== "string" || !extensionId.trim()) {
      throw new Error("A valid extension id is required");
    }
    const result = await installExtension(extensionId);
    await reindexExtensions();
    return result;
  });
  ipcMain.handle("extensions:uninstall", async (_event, extensionId) => {
    if (typeof extensionId !== "string" || !extensionId.trim()) {
      throw new Error("A valid extension id is required");
    }
    const result = await uninstallExtension(extensionId);
    await reindexExtensions();
    return result;
  });
  ipcMain.handle("extensions:integrity", async (_event, extensionId) => {
    if (typeof extensionId !== "string" || !extensionId.trim()) {
      throw new Error("A valid extension id is required");
    }
    return inspectExtensionIntegrity(extensionId);
  });
  ipcMain.handle("extensions:reinstall", async (_event, extensionId) => {
    if (typeof extensionId !== "string" || !extensionId.trim()) {
      throw new Error("A valid extension id is required");
    }
    return reinstallExtension(extensionId);
  });
  ipcMain.handle("extensions:install-error", async (_event, extensionId) => {
    if (typeof extensionId !== "string" || !extensionId.trim()) return null;
    return getExtensionInstallError(extensionId);
  });
  ipcMain.handle("extension:list", async () => {
    return listInstalledRegistryExtensions();
  });
  ipcMain.handle("extension:search-store", async (_event, query) => {
    const q = typeof query === "string" ? query : "";
    return searchExtensionCatalog(q);
  });
  ipcMain.handle("extension:install", async (_event, extensionId) => {
    if (typeof extensionId !== "string" || !extensionId.trim()) {
      throw new Error("A valid extension id is required");
    }
    const result = await installRegistryExtension(extensionId);
    await reindexExtensions();
    return result;
  });
  ipcMain.handle("extension:uninstall", async (_event, extensionId) => {
    if (typeof extensionId !== "string" || !extensionId.trim()) {
      throw new Error("A valid extension id is required");
    }
    const result = uninstallRegistryExtension(extensionId);
    await reindexExtensions();
    return result;
  });
  ipcMain.handle("extension:run-command", async (_event, payload) => {
    if (!payload || typeof payload !== "object") {
      throw new Error("Invalid extension run payload");
    }
    const body = payload;
    if (typeof body.extensionId !== "string" || typeof body.commandName !== "string") {
      throw new Error("extensionId and commandName are required");
    }
    const argumentValues = body.argumentValues && typeof body.argumentValues === "object" ? Object.fromEntries(
      Object.entries(body.argumentValues).map(([key, value]) => [
        key,
        typeof value === "string" ? value : String(value ?? "")
      ])
    ) : void 0;
    return runExtensionCommand({
      extensionId: body.extensionId,
      commandName: body.commandName,
      argumentValues
    });
  });
  ipcMain.handle("extension:invoke-action", async (_event, payload) => {
    if (!payload || typeof payload !== "object") {
      throw new Error("Invalid extension action payload");
    }
    const body = payload;
    if (typeof body.sessionId !== "string" || typeof body.actionId !== "string") {
      throw new Error("sessionId and actionId are required");
    }
    const formValues = body.formValues && typeof body.formValues === "object" ? Object.fromEntries(
      Object.entries(body.formValues).map(([key, value]) => [
        key,
        Array.isArray(value) ? value.filter((entry) => typeof entry === "string") : typeof value === "string" || typeof value === "boolean" || typeof value === "number" ? value : String(value ?? "")
      ])
    ) : void 0;
    return invokeExtensionAction({
      sessionId: body.sessionId,
      actionId: body.actionId,
      formValues
    });
  });
  ipcMain.handle("extension:search-text-changed", async (_event, payload) => {
    if (!payload || typeof payload !== "object") {
      throw new Error("Invalid extension search payload");
    }
    const body = payload;
    if (typeof body.sessionId !== "string" || typeof body.searchText !== "string") {
      throw new Error("sessionId and searchText are required");
    }
    const { updateSearchText: updateSearchText2 } = await Promise.resolve().then(() => (init_extension_runner(), extension_runner_exports));
    return updateSearchText2({
      sessionId: body.sessionId,
      searchText: body.searchText
    });
  });
  ipcMain.handle("extension:refresh-session", async (_event, payload) => {
    if (!payload || typeof payload !== "object") {
      throw new Error("Invalid extension refresh payload");
    }
    const body = payload;
    if (typeof body.sessionId !== "string") {
      throw new Error("sessionId is required");
    }
    return refreshExtensionSession({ sessionId: body.sessionId });
  });
  ipcMain.handle("extension:dispose-session", async (_event, payload) => {
    if (!payload || typeof payload !== "object") return false;
    const body = payload;
    if (typeof body.sessionId !== "string") return false;
    return disposeExtensionSession(body.sessionId);
  });
  ipcMain.handle("extension:load-more", async (_event, payload) => {
    if (!payload || typeof payload !== "object") {
      throw new Error("Invalid extension pagination payload");
    }
    const body = payload;
    if (typeof body.sessionId !== "string") {
      throw new Error("sessionId is required");
    }
    return loadMoreExtensionSession({ sessionId: body.sessionId });
  });
  ipcMain.handle("clipboard:read", async () => {
    return clipboard.readText();
  });
  ipcMain.handle("clipboard:write", async (_event, raw) => {
    const text = typeof raw === "string" ? raw : String(raw ?? "");
    clipboard.writeText(text);
    return { ok: true };
  });
  ipcMain.handle("shell:open", async (_event, raw) => {
    const target = typeof raw === "string" ? raw.trim() : "";
    if (!target) return { ok: false };
    await shell.openExternal(target);
    return { ok: true };
  });
  ipcMain.handle("preferences:get", async (_event, payload) => {
    if (!payload || typeof payload !== "object") return {};
    const body = payload;
    if (typeof body.extensionId !== "string" || !body.extensionId.trim()) return {};
    const commandName2 = typeof body.commandName === "string" ? body.commandName : void 0;
    return getExtensionPreferences(body.extensionId, commandName2);
  });
  ipcMain.handle("preferences:setup", async (_event, payload) => {
    if (!payload || typeof payload !== "object") return null;
    const body = payload;
    if (typeof body.extensionId !== "string" || !body.extensionId.trim()) return null;
    const commandName2 = typeof body.commandName === "string" ? body.commandName : void 0;
    return getExtensionPreferenceSetup(body.extensionId, commandName2);
  });
  ipcMain.handle("preferences:set", async (_event, payload) => {
    if (!payload || typeof payload !== "object") return {};
    const body = payload;
    if (typeof body.extensionId !== "string" || !body.extensionId.trim()) return {};
    const values = body.values && typeof body.values === "object" ? body.values : {};
    const commandName2 = typeof body.commandName === "string" ? body.commandName : void 0;
    return saveExtensionPreferences(body.extensionId, values, commandName2);
  });
  ipcMain.handle(IPC_CHANNELS.SEARCH_ALL, async (_event, query) => {
    const q = typeof query === "string" ? query : "";
    return searchEverything(q);
  });
  ipcMain.handle(IPC_CHANNELS.PATH_COMPLETE, async (_event, query) => {
    const q = typeof query === "string" ? query : "";
    return completePath(q);
  });
  ipcMain.handle(IPC_CHANNELS.DIRECTORY_VISIT_RECORD, async (_event, path7) => {
    if (typeof path7 === "string") recordDirectoryVisit(path7);
  });
  ipcMain.handle(IPC_CHANNELS.SEARCH_BENCHMARK_RUN, async () => {
    return runSearchBenchmarks();
  });
  ipcMain.handle(IPC_CHANNELS.SEARCH_BENCHMARK_HISTORY, async () => {
    return getSearchBenchmarkHistory();
  });
  ipcMain.handle("currency:frankfurter-latest", async (_event, from) => {
    if (typeof from !== "string" || !from.trim()) {
      throw new Error("Frankfurter: currency code required");
    }
    return fetchFrankfurterLatest(from.trim());
  });
  ipcMain.handle("open-ports:list", async () => {
    return listOpenPorts();
  });
  ipcMain.handle("port-manager:named:list", async () => listNamedPorts());
  ipcMain.handle("port-manager:named:add", async (_event, raw) => {
    if (!raw || typeof raw !== "object") return null;
    const o = raw;
    const name = typeof o.name === "string" ? o.name : "";
    const port = typeof o.port === "number" ? o.port : Number(o.port);
    return addNamedPort(name, port);
  });
  ipcMain.handle("port-manager:named:remove", async (_event, id) => {
    if (typeof id !== "string" || !id.trim()) return false;
    return removeNamedPort(id.trim());
  });
  ipcMain.handle(IPC_CHANNELS.SEARCH_EXECUTE, async (_event, payload) => {
    try {
      const request = parseSearchExecuteRequest(payload);
      return executeSearchAction(request.action, request.context);
    } catch {
      if (!payload || typeof payload !== "object") {
        throw new Error("Invalid search action payload");
      }
      return executeSearchAction(payload);
    }
  });
  ipcMain.handle(IPC_CHANNELS.AI_ACTION, async (_event, payload) => {
    const req = parseAiActionRequest(payload);
    const cfg = readLLMConfig();
    if (cfg.aiActionRequirePermission !== false && req.allowAutomation !== true) {
      return {
        ok: false,
        output: "Action mode requires explicit permission. Retry with allowAutomation=true."
      };
    }
    return runAiActionMode({
      ...req,
      redactSensitive: req.redactSensitive ?? cfg.aiActionRedactionEnabled !== false
    });
  });
  ipcMain.handle(IPC_CHANNELS.VOICE_TTS_SPEAK, async (_event, payload) => {
    const req = parseVoiceSpeakRequest(payload);
    await speakText(req.text);
    return { ok: true };
  });
  ipcMain.handle(IPC_CHANNELS.VOICE_TTS_STOP, async () => {
    stopSpeaking();
    return { ok: true };
  });
  ipcMain.handle(IPC_CHANNELS.VOICE_STT_MODES, async () => {
    return listSttModes();
  });
  ipcMain.handle(IPC_CHANNELS.VOICE_STT_TRANSCRIBE, async (_event, payload) => {
    const req = parseVoiceTranscribeRequest(payload);
    return transcribeAudio(req);
  });
  ipcMain.handle("window:suppress-blur-hide", async (_event, payload) => {
    setSuppressBlurHide(payload === true);
    return { ok: true };
  });
  ipcMain.handle(IPC_CHANNELS.VOICE_MODELS_LIST, async () => {
    return listVoiceModels();
  });
  ipcMain.handle(IPC_CHANNELS.VOICE_MODEL_DOWNLOAD, async (_event, payload) => {
    const req = parseVoiceModelRequest(payload);
    return downloadVoiceModel(req.modelId);
  });
  ipcMain.handle(IPC_CHANNELS.VOICE_MODEL_GET_SELECTED, async () => {
    return { modelId: getSelectedVoiceModelId() };
  });
  ipcMain.handle(IPC_CHANNELS.VOICE_MODEL_SET_SELECTED, async (_event, payload) => {
    const req = parseVoiceModelRequest(payload);
    return { modelId: setSelectedVoiceModelId(req.modelId) };
  });
  ipcMain.handle("window:show", async () => {
    const win = getWindow();
    if (win) {
      win.show();
      win.focus();
    }
  });
  ipcMain.handle("window:hide", async () => {
    const win = getWindow();
    if (win) win.hide();
  });
  ipcMain.handle("window:close-current", async (event) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    if (win && !win.isDestroyed()) win.close();
  });
  ipcMain.handle("window:snap-drag-start", async () => {
    const win = getWindow();
    if (!win || win.isDestroyed()) return;
    controls?.startWindowDragMonitoring(win);
  });
  ipcMain.handle("window:snap-drag-end", async () => {
    const win = getWindow();
    if (!win || win.isDestroyed()) return;
    controls?.stopWindowDragMonitoring(win);
  });
  ipcMain.handle("storage:breakdown", async () => {
    return getStorageBreakdown();
  });
  ipcMain.handle("storage:clipboard-config:get", async () => {
    return getClipboardStorageConfig();
  });
  ipcMain.handle("storage:clipboard-config:set", async (_event, payload) => {
    const patch = typeof payload === "object" && payload !== null ? payload : {};
    setClipboardStorageConfig(patch);
    return getClipboardStorageConfig();
  });
  ipcMain.handle("storage:clear-clipboard-images", async () => {
    return clearClipboardImages();
  });
  ipcMain.handle("storage:vacuum-search-db", async () => {
    return vacuumSearchDatabase();
  });
  ipcMain.handle("storage:clear-chromium-cache", async () => {
    await clearChromiumCache();
    return { ok: true };
  });
}

// src/main/server.ts
init_configStore();
init_electron_shim();
var import_node_fs26 = require("node:fs");
var import_node_path27 = require("node:path");
var import_node_child_process15 = require("node:child_process");
var import_node_net = require("node:net");
function materializePiPolicy() {
  const root = process.env.APPDATA_DIR;
  if (!root || false) return;
  try {
    const runtimeDir = (0, import_node_path27.join)(root, "runtime");
    const extensionPath = (0, import_node_path27.join)(runtimeDir, "raymes-pi-policy.ts");
    (0, import_node_fs26.mkdirSync)(runtimeDir, { recursive: true });
    (0, import_node_fs26.writeFileSync)(extensionPath, "type ToolCallEvent = {\n  toolName: string\n  input?: {\n    command?: unknown\n  }\n}\n\ntype ToolCallResult = {\n  block?: boolean\n  reason?: string\n}\n\ntype ExtensionContext = {\n  ui: {\n    confirm(title: string, message: string, opts?: { timeoutMs?: number }): Promise<boolean>\n  }\n}\n\ntype ExtensionAPI = {\n  on(\n    event: 'tool_call',\n    handler: (event: ToolCallEvent, ctx: ExtensionContext) => ToolCallResult | undefined | Promise<ToolCallResult | undefined>,\n  ): void\n  registerProvider(name: string, config: RaymesPiProviderConfig): void\n}\n\ntype RaymesPiProviderConfig = {\n  baseUrl: string\n  apiKey: string\n  api: 'openai-completions' | 'anthropic-messages'\n  authHeader?: boolean\n  models: Array<{\n    id: string\n    name: string\n    reasoning: boolean\n    input: Array<'text' | 'image'>\n    cost: {\n      input: number\n      output: number\n      cacheRead: number\n      cacheWrite: number\n    }\n    contextWindow: number\n    maxTokens: number\n    compat?: Record<string, unknown>\n  }>\n}\n\nfunction registerRaymesProvider(pi: ExtensionAPI): void {\n  const raw = process.env['RAYMES_PI_PROVIDER_JSON']\n  if (!raw) return\n  try {\n    const parsed = JSON.parse(raw) as RaymesPiProviderConfig\n    if (!parsed.baseUrl || !parsed.apiKey || !parsed.api || !Array.isArray(parsed.models)) return\n    pi.registerProvider('tezbar', parsed)\n  } catch {\n    /* Ignore malformed bridge env so pi can still start with its own config. */\n  }\n}\n\nfunction hasUnsafeShellSyntax(command: string): boolean {\n  return /[;|<>`\\n]/.test(command) || command.includes('$(') || command.includes('||')\n}\n\nfunction persistedAllowedCommands(): Set<string> {\n  const raw = process.env['RAYMES_PI_ALWAYS_ALLOW_JSON']\n  if (!raw) return new Set()\n  try {\n    const parsed = JSON.parse(raw) as unknown\n    if (!Array.isArray(parsed)) return new Set()\n    return new Set(\n      parsed\n        .filter(\n          (entry): entry is string =>\n            typeof entry === 'string' && /^[a-z0-9][a-z0-9._+-]{0,63}$/i.test(entry)\n        )\n        .map((entry) => entry.toLowerCase())\n    )\n  } catch {\n    return new Set()\n  }\n}\n\nfunction executableName(command: string): string {\n  const token = command.trim().split(/\\s+/, 1)[0] ?? ''\n  return token.slice(token.lastIndexOf('/') + 1).toLowerCase()\n}\n\nconst SAFE_PIPELINE_COMMANDS = new Set(['ps', 'head', 'tail', 'wc'])\n\nexport function isPersistentlyAllowedBash(\n  command: string,\n  allowedCommands: ReadonlySet<string>\n): boolean {\n  const trimmed = command.trim()\n  if (\n    !trimmed ||\n    /[;<>`\\n]/.test(trimmed) ||\n    trimmed.includes('$(') ||\n    trimmed.includes('||')\n  ) {\n    return false\n  }\n\n  const commands = trimmed\n    .split(/\\s*(?:&&|\\|)\\s*/)\n    .map((part) => part.trim())\n    .filter(Boolean)\n  if (commands.length === 0) return false\n\n  return commands.every((part) => {\n    if (isSimpleCd(part)) return true\n    const executable = executableName(part)\n    return SAFE_PIPELINE_COMMANDS.has(executable) || allowedCommands.has(executable)\n  })\n}\n\nfunction isSimpleCd(command: string): boolean {\n  return /^cd\\s+(?:\"[^\"]+\"|'[^']+'|[~./A-Za-z0-9_ -]+)$/.test(command.trim())\n}\n\nfunction isSafeGitStatus(command: string): boolean {\n  return /^git\\s+status(?:\\s+[^;&|<>`$()\\n]+)*$/.test(command.trim())\n}\n\nfunction isSafeGitClone(command: string): boolean {\n  return /^git\\s+clone(?:\\s+[^;&|<>`$()\\n]+)+$/.test(command.trim())\n}\n\nfunction isSafeDirectoryRead(command: string): boolean {\n  const trimmed = command.trim()\n  return (\n    trimmed === 'pwd' ||\n    /^ls(?:\\s+-[A-Za-z0-9@]+)*(?:\\s+(?:\"[^\"]+\"|'[^']+'|[~./A-Za-z0-9_ -]+))*$/.test(trimmed) ||\n    /^which\\s+[-A-Za-z0-9_ .+/]+$/.test(trimmed) ||\n    /^command\\s+-v\\s+[-A-Za-z0-9_ .+/]+$/.test(trimmed) ||\n    /^find\\s+(?:\\/Applications|~\\/Applications)(?:\\s+[^;&|<>`$()\\n]+)*$/.test(trimmed) ||\n    /^mdfind\\s+[^;&|<>`$()\\n]+$/.test(trimmed)\n  )\n}\n\nexport function isAutoAllowedBash(\n  command: string,\n  allowedCommands: ReadonlySet<string> = persistedAllowedCommands()\n): boolean {\n  const trimmed = command.trim()\n  if (!trimmed) return false\n  if (isPersistentlyAllowedBash(trimmed, allowedCommands)) return true\n  if (hasUnsafeShellSyntax(trimmed)) return false\n\n  const parts = trimmed.split(/\\s+&&\\s+/).map((part) => part.trim()).filter(Boolean)\n  if (parts.length === 0) return false\n\n  const commandToRun = parts[parts.length - 1]\n  if (\n    !commandToRun ||\n    !(isSafeGitStatus(commandToRun) || isSafeGitClone(commandToRun) || isSafeDirectoryRead(commandToRun))\n  ) {\n    return false\n  }\n\n  return parts.slice(0, -1).every(isSimpleCd)\n}\n\nexport default function raymesPiPolicy(pi: ExtensionAPI): void {\n  registerRaymesProvider(pi)\n\n  pi.on('tool_call', async (event, ctx) => {\n    if (event.toolName !== 'bash') return undefined\n\n    const command = event.input?.command\n    if (typeof command !== 'string') {\n      return { block: true, reason: 'Missing bash command.' }\n    }\n\n    if (isAutoAllowedBash(command)) return undefined\n\n    const confirmed = await ctx.ui.confirm('Run bash command?', command)\n    if (confirmed) return undefined\n\n    return { block: true, reason: 'Bash command was not approved.' }\n  })\n}\n", "utf8");
    process.env.RAYMES_PI_EXTENSION = extensionPath;
  } catch (error) {
    console.error("[server] failed to materialize Pi policy:", error);
  }
}
function fixPathSync() {
  if (process.platform === "win32") return;
  try {
    const stdout = (0, import_node_child_process15.execFileSync)("bash", ["-lc", "echo -n $PATH"], {
      encoding: "utf8",
      timeout: 2e3
    });
    const fromShell = stdout.trim();
    if (fromShell) {
      process.env.PATH = fromShell;
    }
  } catch (err) {
    console.warn("[server] failed to get PATH from login shell:", err);
  }
  const extras = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"];
  const existing = new Set((process.env.PATH || "").split(":").filter(Boolean));
  const currentPaths = (process.env.PATH || "").split(":");
  for (const e of extras) {
    if (!existing.has(e)) {
      currentPaths.push(e);
    }
  }
  process.env.PATH = currentPaths.filter(Boolean).join(":");
}
fixPathSync();
materializePiPolicy();
var mockWin = new BrowserWindow();
var tauriIpcMain = ipcMain;
registerIpcHandlers(() => mockWin, {
  startWindowDragMonitoring: () => {
  },
  stopWindowDragMonitoring: () => {
  },
  updateRaymesHotkey: (h) => {
    writeConfigPatch({ raymesHotkey: h });
    return { ok: true, accelerator: h };
  }
});
startClipboardWatcher();
function writeReply(payload) {
  process.stdout.write(`${JSON.stringify(payload)}
`);
}
async function handleLine(line) {
  if (!line.trim()) return;
  try {
    const message = JSON.parse(line);
    if (message.type === "invoke") {
      const { id, channel, payload } = message;
      if (typeof id !== "string" && typeof id !== "number" || typeof channel !== "string") return;
      const startedAt = Date.now();
      try {
        const args = Array.isArray(payload) ? payload : [payload];
        const result = await tauriIpcMain._invoke(channel, ...args);
        const elapsedMs2 = Date.now() - startedAt;
        if (elapsedMs2 >= 1e3) {
          console.warn(`[server] slow IPC: ${channel} completed in ${elapsedMs2}ms`);
        }
        writeReply({ type: "reply", id, result });
      } catch (error) {
        console.error(`[server] IPC failed: ${channel}`, error);
        writeReply({
          type: "reply",
          id,
          error: error instanceof Error ? error.message : String(error)
        });
      }
    }
  } catch (error) {
    console.error("[server] error parsing/handling stdin line:", error);
  }
}
var stdinBuffer = "";
function processInputChunk(chunk) {
  stdinBuffer += chunk;
  let newlineIndex = stdinBuffer.indexOf("\n");
  while (newlineIndex >= 0) {
    const line = stdinBuffer.slice(0, newlineIndex);
    stdinBuffer = stdinBuffer.slice(newlineIndex + 1);
    void handleLine(line);
    newlineIndex = stdinBuffer.indexOf("\n");
  }
}
var bunRuntime = globalThis.Bun;
var backendIpcPort = Number(process.env.BACKEND_IPC_PORT);
if (Number.isInteger(backendIpcPort) && backendIpcPort > 0 && backendIpcPort <= 65535) {
  const socket = (0, import_node_net.createConnection)({ host: "127.0.0.1", port: backendIpcPort }, () => {
    console.error(`[server] Connected to Tauri IPC on localhost:${backendIpcPort}`);
  });
  socket.setEncoding("utf8");
  socket.on("data", processInputChunk);
  socket.on("end", cleanup);
  socket.on("error", (error) => {
    console.error("[server] Tauri IPC socket failed:", error);
    cleanup();
  });
} else if (bunRuntime) {
  void (async () => {
    const reader = bunRuntime.stdin.stream().getReader();
    const decoder = new TextDecoder();
    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        processInputChunk(decoder.decode(value, { stream: true }));
      }
      processInputChunk(decoder.decode());
    } finally {
      reader.releaseLock();
    }
    cleanup();
  })().catch((error) => {
    console.error("[server] native Bun stdin reader failed:", error);
    cleanup();
  });
} else {
  process.stdin.setEncoding("utf8");
  process.stdin.on("data", processInputChunk);
  process.stdin.on("end", cleanup);
  process.stdin.resume();
}
function cleanup() {
  try {
    stopClipboardWatcher();
    shutdownIpcHandlers();
    flushConfig();
  } catch (err) {
    console.error("[server] error during cleanup:", err);
  }
  process.exit(0);
}
process.on("SIGINT", cleanup);
process.on("SIGTERM", cleanup);
console.error("[server] Raymes TS background runner started successfully via stdin/stdout IPC.");
//# sourceMappingURL=main.js.map
