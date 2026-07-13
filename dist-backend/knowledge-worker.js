"use strict";

// src/main/knowledge/worker.ts
var import_node_os3 = require("node:os");

// src/main/knowledge/service.ts
var import_node_child_process4 = require("node:child_process");
var import_node_crypto4 = require("node:crypto");
var import_node_fs6 = require("node:fs");
var import_node_os2 = require("node:os");
var import_node_path7 = require("node:path");
var import_node_util3 = require("node:util");

// src/main/knowledge/backends/local/backend.ts
var import_node_fs2 = require("node:fs");

// src/main/knowledge/core/chunker.ts
var import_node_crypto = require("node:crypto");
var TARGET_CHARS = 1200;
var OVERLAP_CHARS = 180;
var MAX_KNOWLEDGE_CHUNKS_PER_SOURCE = 4e3;
function normalizeText(value) {
  return value.replace(/\u0000/g, "").replace(/[ \t]+\n/g, "\n").replace(/\n{3,}/g, "\n\n").trim();
}
async function chunkText(sourceId, text, pageNumber, limit, yieldToInteractiveWork) {
  const normalized = normalizeText(text);
  if (!normalized) return [];
  const chunks = [];
  let start = 0;
  while (start < normalized.length && chunks.length < limit) {
    let end = Math.min(normalized.length, start + TARGET_CHARS);
    if (end < normalized.length) {
      const paragraph = normalized.lastIndexOf("\n\n", end);
      const sentence = normalized.lastIndexOf(". ", end);
      const boundary = Math.max(paragraph, sentence);
      if (boundary > start + TARGET_CHARS * 0.55) end = boundary + (boundary === sentence ? 1 : 0);
    }
    const value = normalized.slice(start, end).trim();
    if (value) {
      const id = (0, import_node_crypto.createHash)("sha256").update(`${sourceId}:${pageNumber ?? 0}:${start}:${value}`).digest("hex");
      chunks.push({ id, pageNumber, text: value, startOffset: start, endOffset: end });
    }
    if (end >= normalized.length) break;
    start = Math.max(start + 1, end - OVERLAP_CHARS);
    if (chunks.length % 32 === 0) {
      await new Promise((resolve2) => setImmediate(resolve2));
      await yieldToInteractiveWork?.();
    }
  }
  return chunks;
}
async function chunksFromPages(sourceId, pages, limit = MAX_KNOWLEDGE_CHUNKS_PER_SOURCE, yieldToInteractiveWork) {
  const chunks = [];
  for (const page of pages) {
    if (chunks.length >= limit) break;
    const extracted = page.extractedText?.trim() ?? "";
    const ocr = page.ocr?.text.trim() ?? "";
    const combined = extracted && ocr && !extracted.includes(ocr) ? `${extracted}

${ocr}` : extracted || ocr;
    chunks.push(...await chunkText(
      sourceId,
      combined,
      page.pageNumber,
      limit - chunks.length,
      yieldToInteractiveWork
    ));
  }
  return chunks;
}

// src/main/knowledge/embeddings/featureEmbedding.ts
var DIMENSIONS = 384;
var FEATURE_EMBEDDING_MODEL = {
  id: "tezbar-multilingual-feature-v1",
  version: "1.0.0",
  dimensions: DIMENSIONS
};
function hash(value, seed) {
  let current = seed >>> 0;
  for (let index = 0; index < value.length; index += 1) {
    current ^= value.charCodeAt(index);
    current = Math.imul(current, 16777619);
  }
  return current >>> 0;
}
function embedText(value) {
  const normalized = ` ${value.toLocaleLowerCase().normalize("NFKC").replace(/\s+/g, " ").trim()} `;
  const vector = new Float32Array(DIMENSIONS);
  const addFeature = (feature) => {
    const bucket = hash(feature, 2166136261) % DIMENSIONS;
    const sign = (hash(feature, 2654435769) & 1) === 0 ? 1 : -1;
    vector[bucket] += sign;
  };
  const words = normalized.trim().split(" ").filter(Boolean);
  for (const word of words) {
    addFeature(`w:${word}`);
    for (let size = 2; size <= 4; size += 1) {
      for (let index = 0; index + size <= word.length; index += 1) {
        addFeature(`c:${word.slice(index, index + size)}`);
      }
    }
  }
  let magnitude = 0;
  for (const component of vector) magnitude += component * component;
  magnitude = Math.sqrt(magnitude) || 1;
  return Array.from(vector, (component) => component / magnitude);
}
function cosineSimilarity(left, right) {
  const length = Math.min(left.length, right.length);
  if (length === 0) return 0;
  let dot = 0;
  let leftMagnitude = 0;
  let rightMagnitude = 0;
  for (let index = 0; index < length; index += 1) {
    const a = left[index] ?? 0;
    const b = right[index] ?? 0;
    dot += a * b;
    leftMagnitude += a * a;
    rightMagnitude += b * b;
  }
  if (leftMagnitude === 0 || rightMagnitude === 0) return 0;
  return dot / Math.sqrt(leftMagnitude * rightMagnitude);
}

// src/main/knowledge/extractors/localExtractor.ts
var import_node_child_process = require("node:child_process");
var import_node_fs = require("node:fs");
var import_promises = require("node:fs/promises");
var import_node_path = require("node:path");
var import_node_util = require("node:util");
var execFileAsync = (0, import_node_util.promisify)(import_node_child_process.execFile);
var MAX_EXTRACTED_BYTES = 64 * 1024 * 1024;
var MAX_PLAIN_TEXT_EXTRACTED_BYTES = 8 * 1024 * 1024;
var MAX_PLAIN_TEXT_SOURCE_BYTES = 64 * 1024 * 1024;
var MAX_IMAGE_SOURCE_BYTES = 75 * 1024 * 1024;
var MAX_DOCUMENT_SOURCE_BYTES = 150 * 1024 * 1024;
var PLAIN_TEXT_EXTENSIONS = /* @__PURE__ */ new Set([
  ".txt",
  ".text",
  ".md",
  ".mdx",
  ".rst",
  ".org",
  ".csv",
  ".tsv",
  ".log",
  ".json",
  ".jsonl",
  ".yaml",
  ".yml",
  ".toml",
  ".xml",
  ".html",
  ".htm",
  ".css",
  ".scss",
  ".less",
  ".js",
  ".jsx",
  ".mjs",
  ".cjs",
  ".ts",
  ".tsx",
  ".py",
  ".rb",
  ".php",
  ".go",
  ".rs",
  ".swift",
  ".java",
  ".kt",
  ".kts",
  ".c",
  ".h",
  ".cc",
  ".cpp",
  ".hpp",
  ".cs",
  ".sh",
  ".zsh",
  ".bash",
  ".fish",
  ".sql",
  ".graphql",
  ".gql",
  ".env.example",
  ".ini",
  ".cfg",
  ".conf",
  ".properties",
  ".tex",
  ".bib",
  ".srt",
  ".vtt",
  ".ics",
  ".vcf",
  ".diff",
  ".patch",
  ".dockerfile",
  ".gitignore"
]);
var IMAGE_EXTENSIONS = /* @__PURE__ */ new Set([
  ".png",
  ".jpg",
  ".jpeg",
  ".heic",
  ".heif",
  ".tif",
  ".tiff",
  ".bmp",
  ".gif",
  ".webp"
]);
var RICH_DOCUMENT_EXTENSIONS = /* @__PURE__ */ new Set([
  ".rtf",
  ".rtfd",
  ".doc",
  ".docx",
  ".odt",
  ".pages",
  ".ppt",
  ".pptx",
  ".odp",
  ".key",
  ".xls",
  ".xlsx",
  ".ods",
  ".numbers",
  ".epub"
]);
var INDEXABLE_EXTENSIONS = /* @__PURE__ */ new Set([
  ...PLAIN_TEXT_EXTENSIONS,
  ...IMAGE_EXTENSIONS,
  ...RICH_DOCUMENT_EXTENSIONS,
  ".pdf"
]);
function screenOcrHelperPath() {
  const candidates = [
    process.env.SCREENOCR_HELPER_PATH,
    (0, import_node_path.join)(process.cwd(), "native", "screenocr", "screenocr-helper")
  ];
  return candidates.find((candidate) => Boolean(candidate && (0, import_node_fs.existsSync)(candidate))) ?? "";
}
async function runScreenOcr(command, input, signal) {
  const helper = screenOcrHelperPath();
  if (!helper) throw new Error("The macOS text extraction helper is not available");
  const { stdout } = await execFileAsync(helper, [command, JSON.stringify(input)], {
    encoding: "utf8",
    maxBuffer: MAX_EXTRACTED_BYTES,
    signal
  });
  const parsed = JSON.parse(stdout.trim());
  if (!parsed.ok) throw new Error(parsed.error || "Native text extraction failed");
  return parsed.value;
}
function decodeText(buffer) {
  if (buffer.length === 0) return "";
  const sample = buffer.subarray(0, Math.min(buffer.length, 8192));
  let nulCount = 0;
  for (const value of sample) if (value === 0) nulCount += 1;
  if (nulCount / sample.length > 0.02) throw new Error("File appears to contain binary data");
  return buffer.toString("utf8").replace(/^\uFEFF/, "");
}
function stripMarkup(value) {
  return value.replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, " ").replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, " ").replace(/<[^>]+>/g, " ").replace(/&nbsp;/gi, " ").replace(/&amp;/gi, "&").replace(/&lt;/gi, "<").replace(/&gt;/gi, ">").replace(/&quot;/gi, '"').replace(/&#39;/gi, "'").replace(/[ \t]{2,}/g, " ").replace(/\n{3,}/g, "\n\n").trim();
}
async function extractPlainText(path) {
  const handle = await (0, import_promises.open)(path, "r");
  try {
    const stat = await handle.stat();
    const bytesToRead = Math.min(stat.size, MAX_PLAIN_TEXT_EXTRACTED_BYTES);
    const buffer = Buffer.allocUnsafe(bytesToRead);
    const { bytesRead } = await handle.read(buffer, 0, bytesToRead, 0);
    const text = decodeText(buffer.subarray(0, bytesRead));
    return {
      text: /\.(?:html?|xml)$/i.test(path) ? stripMarkup(text) : text.trim(),
      truncated: stat.size > bytesRead
    };
  } finally {
    await handle.close();
  }
}
async function extractRichDocument(path, signal) {
  if (process.platform === "darwin" && (0, import_node_fs.existsSync)("/usr/bin/textutil")) {
    try {
      const { stdout } = await execFileAsync("/usr/bin/textutil", ["-convert", "txt", "-stdout", path], {
        encoding: "utf8",
        maxBuffer: MAX_EXTRACTED_BYTES,
        signal
      });
      if (stdout.trim()) return stdout.trim();
    } catch {
    }
  }
  if ((0, import_node_fs.existsSync)("/usr/bin/unzip")) {
    const extension = (0, import_node_path.extname)(path).toLowerCase();
    const members = extension === ".epub" ? ["*.xhtml", "*.html"] : extension === ".xlsx" || extension === ".ods" || extension === ".numbers" ? ["*.xml"] : ["*.xml", "*.xhtml"];
    const parts = [];
    for (const member of members) {
      try {
        const { stdout } = await execFileAsync("/usr/bin/unzip", ["-p", path, member], {
          encoding: "utf8",
          maxBuffer: MAX_EXTRACTED_BYTES,
          signal
        });
        if (stdout.trim()) parts.push(stripMarkup(stdout));
      } catch {
      }
    }
    if (parts.length > 0) return parts.join("\n\n").trim();
  }
  throw new Error("No local extractor is available for this document format");
}
async function extractMetadataText(path, signal) {
  if (process.platform !== "darwin") return "";
  try {
    const { stdout } = await execFileAsync("/usr/bin/mdls", ["-raw", "-name", "kMDItemTextContent", path], {
      encoding: "utf8",
      maxBuffer: MAX_EXTRACTED_BYTES,
      signal
    });
    const trimmed = stdout.trim();
    return trimmed === "(null)" ? "" : trimmed;
  } catch {
    return "";
  }
}
function isIndexablePath(path) {
  const extension = (0, import_node_path.extname)(path).toLowerCase();
  if (INDEXABLE_EXTENSIONS.has(extension)) return true;
  const filename = path.split("/").pop()?.toLowerCase() ?? "";
  return ["dockerfile", "makefile", "license", "readme", "changelog"].includes(filename);
}
function maximumIndexableSourceBytes(path) {
  const extension = (0, import_node_path.extname)(path).toLowerCase();
  if (extension === ".pdf" || RICH_DOCUMENT_EXTENSIONS.has(extension)) {
    return MAX_DOCUMENT_SOURCE_BYTES;
  }
  if (IMAGE_EXTENSIONS.has(extension)) return MAX_IMAGE_SOURCE_BYTES;
  return MAX_PLAIN_TEXT_SOURCE_BYTES;
}
async function extractLocally(path, signal, options) {
  const extension = (0, import_node_path.extname)(path).toLowerCase();
  const warnings = [];
  if (extension === ".pdf") {
    if (process.platform !== "darwin") {
      const text2 = await extractMetadataText(path, signal);
      return {
        pages: [{ pageNumber: 1, extractedText: text2 }],
        images: [],
        completedCapabilities: ["extracted-text"],
        warnings: text2 ? [] : ["PDF extraction is not available on this platform yet."],
        extractor: { id: "metadata-pdf", version: "1.0.0" }
      };
    }
    const pdf = await runScreenOcr("extract-pdf", {
      documentPath: path,
      maxPages: options.maxPagesPerDocument ?? 2e3,
      ocrScannedPages: options.enableOcr,
      maxOcrPages: options.maxOcrPagesPerDocument ?? 2e3,
      ocrEveryPage: options.ocrEveryPage,
      languageCorrection: true,
      languages: ["en-US", "ru-RU"]
    }, signal);
    const pages = pdf.pages;
    const usedOcr = pages.some((page) => Boolean(page.ocr?.text));
    return {
      pages,
      images: [],
      completedCapabilities: usedOcr ? ["extracted-text", "ocr"] : ["extracted-text"],
      warnings,
      extractor: { id: "macos-pdfkit-vision", version: "1.0.0" },
      totalPageCount: pdf.totalPages
    };
  }
  if (IMAGE_EXTENSIONS.has(extension)) {
    if (!options.enableOcr) {
      return {
        pages: [],
        images: [],
        completedCapabilities: [],
        warnings: ["Image OCR is disabled at the selected Knowledge Depth."],
        extractor: { id: "metadata-only-image", version: "1.0.0" }
      };
    }
    if (process.platform !== "darwin") {
      return {
        pages: [],
        images: [],
        completedCapabilities: [],
        warnings: ["Image OCR is not available on this platform yet."],
        extractor: { id: "unsupported-image", version: "1.0.0" }
      };
    }
    const ocr = await runScreenOcr("recognize-file", {
      imagePath: path,
      languageCorrection: true,
      languages: ["en-US", "ru-RU"]
    }, signal);
    const image = {
      id: `${path}:image:1`,
      pageNumber: 1,
      ocrText: ocr.text,
      width: ocr.width,
      height: ocr.height
    };
    return {
      pages: [{
        pageNumber: 1,
        ocr: { text: ocr.text, blocks: ocr.blocks ?? [], averageConfidence: ocr.averageConfidence }
      }],
      images: [image],
      completedCapabilities: ["ocr"],
      warnings,
      extractor: { id: "macos-vision-image", version: "1.0.0" },
      totalPageCount: 1
    };
  }
  let text = "";
  if (PLAIN_TEXT_EXTENSIONS.has(extension) || !extension) {
    const extraction = await extractPlainText(path);
    text = extraction.text;
    if (extraction.truncated) {
      warnings.push("Only the first 8 MB of this large text file was indexed.");
    }
  } else if (RICH_DOCUMENT_EXTENSIONS.has(extension)) {
    text = await extractRichDocument(path, signal);
  }
  if (!text) text = await extractMetadataText(path, signal);
  if (!text) warnings.push("No textual content could be extracted from this file.");
  return {
    pages: [{ pageNumber: 1, extractedText: text }],
    images: [],
    completedCapabilities: ["extracted-text"],
    warnings,
    extractor: {
      id: RICH_DOCUMENT_EXTENSIONS.has(extension) ? "system-document-text" : "plain-text",
      version: "1.0.0"
    },
    totalPageCount: 1
  };
}

// src/main/safety/redaction.ts
function redactSensitiveText(input) {
  return input.replace(/-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----/g, "[REDACTED_PRIVATE_KEY]").replace(/\b(sk-[A-Za-z0-9_-]{12,})\b/g, "[REDACTED_API_KEY]").replace(/\b(gh[pousr]_[A-Za-z0-9_]{12,})\b/g, "[REDACTED_TOKEN]").replace(/\b(AKIA[0-9A-Z]{16})\b/g, "[REDACTED_AWS_ACCESS_KEY]").replace(/\b(bearer\s+)[A-Za-z0-9._~+\/-]{16,}/gi, "$1[REDACTED_TOKEN]").replace(/\b(password|passwd|api[_-]?key|secret|token)\s*[=:]\s*([^\s,;]+)/gi, "$1=[REDACTED]");
}

// src/main/knowledge/backends/local/backend.ts
var LocalIndexingBackend = class {
  id = "local";
  jobs = /* @__PURE__ */ new Map();
  async estimate(request) {
    const stat = (0, import_node_fs2.statSync)(request.path);
    const estimatedPages = request.path.toLowerCase().endsWith(".pdf") ? Math.max(1, Math.ceil(stat.size / 75e3)) : 1;
    return {
      sourceCount: 1,
      byteSize: stat.size,
      estimatedPages,
      estimatedOcrPages: request.requestedCapabilities.includes("ocr") && request.path.match(/\.(?:png|jpe?g|heic|heif|tiff?|bmp|gif|webp)$/i) ? 1 : 0,
      estimatedStorageBytes: Math.min(stat.size * 2, 128 * 1024 * 1024)
    };
  }
  async index(request, context) {
    const controller = new AbortController();
    const abort = () => controller.abort();
    context.signal.addEventListener("abort", abort, { once: true });
    this.jobs.set(request.jobId, controller);
    try {
      context.onProgress(0.08, "Extracting text");
      const extraction = await extractLocally(request.path, controller.signal, {
        maxPagesPerDocument: request.maxPagesPerDocument,
        enableOcr: request.requestedCapabilities.includes("ocr"),
        maxOcrPagesPerDocument: request.maxOcrPagesPerDocument,
        ocrEveryPage: request.ocrEveryPage
      });
      for (const page of extraction.pages) {
        if (page.extractedText) page.extractedText = redactSensitiveText(page.extractedText);
        if (page.ocr) {
          page.ocr.text = redactSensitiveText(page.ocr.text);
          page.ocr.blocks = page.ocr.blocks.map((block) => ({
            ...block,
            text: redactSensitiveText(block.text)
          }));
        }
      }
      for (const image of extraction.images) {
        if (image.ocrText) image.ocrText = redactSensitiveText(image.ocrText);
        if (image.description) image.description = redactSensitiveText(image.description);
      }
      if (controller.signal.aborted) throw new Error("Indexing cancelled");
      context.onProgress(0.62, "Creating search chunks");
      await context.yieldToInteractiveWork?.();
      const chunks = await chunksFromPages(
        request.sourceId,
        extraction.pages,
        MAX_KNOWLEDGE_CHUNKS_PER_SOURCE,
        context.yieldToInteractiveWork
      );
      const shouldEmbedText = request.requestedCapabilities.includes("text-embeddings");
      if (shouldEmbedText) {
        context.onProgress(0.76, "Creating local search vectors");
        for (let index = 0; index < chunks.length; index += 1) {
          if (controller.signal.aborted) throw new Error("Indexing cancelled");
          const chunk = chunks[index];
          if (chunk) chunk.embedding = embedText(chunk.text);
          await new Promise((resolve2) => setImmediate(resolve2));
          await context.yieldToInteractiveWork?.();
        }
      }
      const completedCapabilities = new Set(extraction.completedCapabilities);
      completedCapabilities.add("chunks");
      if (chunks.length > 0 && shouldEmbedText) completedCapabilities.add("text-embeddings");
      context.onProgress(1, "Ready");
      return {
        sourceId: request.sourceId,
        sourceHash: request.fingerprint.contentHash,
        extractor: extraction.extractor,
        textEmbeddingModel: chunks.length > 0 && shouldEmbedText ? FEATURE_EMBEDDING_MODEL : void 0,
        pages: extraction.pages,
        chunks,
        images: extraction.images,
        completedCapabilities: Array.from(completedCapabilities),
        warnings: chunks.length >= MAX_KNOWLEDGE_CHUNKS_PER_SOURCE ? [...extraction.warnings, `Limited this source to ${MAX_KNOWLEDGE_CHUNKS_PER_SOURCE} chunks.`] : extraction.warnings,
        totalPageCount: extraction.totalPageCount
      };
    } finally {
      context.signal.removeEventListener("abort", abort);
      this.jobs.delete(request.jobId);
    }
  }
  async cancel(jobId) {
    this.jobs.get(jobId)?.abort();
  }
};

// src/main/knowledge/backends/cloud/backend.ts
var TezbarCloudIndexingBackend = class {
  id = "tezbar-cloud";
  async estimate(request) {
    void request;
    throw new Error("Tezbar Cloud indexing is not available yet.");
  }
  async index(request, context) {
    void request;
    void context;
    throw new Error("Tezbar Cloud indexing is not available yet.");
  }
  async cancel(jobId) {
    void jobId;
  }
};

// src/main/knowledge/core/fingerprint.ts
var import_node_crypto2 = require("node:crypto");
var import_node_fs3 = require("node:fs");
async function fingerprintSource(path, signal) {
  const stat = (0, import_node_fs3.statSync)(path);
  const hash2 = (0, import_node_crypto2.createHash)("sha256");
  const stream = (0, import_node_fs3.createReadStream)(path);
  await new Promise((resolve2, reject) => {
    const abort = () => {
      stream.destroy(new Error("Indexing cancelled"));
    };
    signal?.addEventListener("abort", abort, { once: true });
    stream.on("data", (chunk) => hash2.update(chunk));
    stream.once("end", resolve2);
    stream.once("error", reject);
    stream.once("close", () => signal?.removeEventListener("abort", abort));
  });
  return {
    contentHash: hash2.digest("hex"),
    byteSize: stat.size,
    modifiedAt: stat.mtimeMs
  };
}
function sourceIdForPath(path) {
  return (0, import_node_crypto2.createHash)("sha256").update(path).digest("hex");
}
function artifactSettingsHash(value) {
  return (0, import_node_crypto2.createHash)("sha256").update(JSON.stringify(value)).digest("hex");
}

// src/main/desktop-runtime.ts
var import_node_path2 = require("node:path");
var import_node_os = require("node:os");
var import_node_child_process2 = require("node:child_process");
var import_node_util2 = require("node:util");
var execFileAsync2 = (0, import_node_util2.promisify)(import_node_child_process2.execFile);
var backendWebContents = {
  id: 1,
  send(channel, ...args) {
    process.stdout.write(`${JSON.stringify({ type: "event", channel, payload: args[0] })}
`);
  },
  isDestroyed() {
    return false;
  },
  once(_event, _listener) {
  }
};
var IpcMain = class {
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
var ipcMain = new IpcMain();
var app = {
  isPackaged: process.env.IS_TAURI === "true",
  name: "Tezbar",
  getPath(name) {
    if (name === "userData") {
      return process.env.APPDATA_DIR || (0, import_node_path2.join)((0, import_node_os.homedir)(), ".tezbar");
    }
    if (name === "temp") {
      return process.env.TEMP_DIR || (0, import_node_os.tmpdir)();
    }
    if (name === "home") {
      return (0, import_node_os.homedir)();
    }
    return (0, import_node_path2.join)((0, import_node_os.homedir)(), `.${name}`);
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
  focus(_options) {
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
  once(_event, _listener) {
  },
  quit() {
    process.stdout.write(`${JSON.stringify({ type: "app_quit" })}
`);
  },
  exit(_code) {
    process.stdout.write(`${JSON.stringify({ type: "app_quit" })}
`);
  }
};

// src/main/better-sqlite3-shim.ts
var import_bun_sqlite = require("bun:sqlite");
var import_node_fs4 = require("node:fs");
var import_node_path3 = require("node:path");
if (process.platform === "darwin") {
  const sqliteCandidates = [
    process.env.TEZBAR_SQLITE_LIBRARY_PATH,
    (0, import_node_path3.join)(__dirname, "libsqlite3.dylib"),
    "/opt/homebrew/opt/sqlite/lib/libsqlite3.dylib",
    "/usr/local/opt/sqlite/lib/libsqlite3.dylib",
    "/usr/local/opt/sqlite3/lib/libsqlite3.dylib"
  ].filter((path) => Boolean(path && (0, import_node_fs4.existsSync)(path)));
  const sqliteLibrary = sqliteCandidates[0];
  if (sqliteLibrary) {
    try {
      import_bun_sqlite.Database.setCustomSQLite(sqliteLibrary);
    } catch (error) {
      console.warn("[SQLite] Could not enable loadable extensions:", error);
    }
  }
}
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
  loadExtension(path) {
    this._db.loadExtension(path);
  }
  transaction(fn) {
    return this._db.transaction(fn);
  }
};
var better_sqlite3_shim_default = DatabaseShim;

// src/main/knowledge/database/store.ts
var import_node_fs5 = require("node:fs");
var import_node_path5 = require("node:path");
var import_node_crypto3 = require("node:crypto");

// src/main/sqlite-vec-bundled.ts
var import_node_path4 = require("node:path");
function load(database) {
  database.loadExtension((0, import_node_path4.join)(__dirname, "vec0"));
}

// src/main/search/textMatch.ts
function wordTokens(value) {
  return value.toLocaleLowerCase().normalize("NFKC").match(/[\p{L}\p{N}]+/gu) ?? [];
}
function buildContentFtsQuery(query) {
  const tokens = wordTokens(query);
  return tokens.filter((token) => token.length >= 3).map((token) => token.length >= 4 ? `${token}*` : token).join(" AND ");
}

// src/main/knowledge/depth.ts
var DEFAULT_KNOWLEDGE_SETTINGS = {
  depth: "smart",
  maxConcurrentExtractors: 1,
  maxConcurrentOcrJobs: 1,
  runOnBattery: false,
  onlyRunHeavyJobsWhenIdle: true
};
var PROFILES = {
  basic: {
    depth: "basic",
    requestedCapabilities: ["extracted-text", "chunks"],
    maxPagesPerDocument: 20,
    maxOcrPagesPerDocument: 0,
    ocrEveryPage: false
  },
  smart: {
    depth: "smart",
    requestedCapabilities: ["extracted-text", "ocr", "chunks", "text-embeddings"],
    maxPagesPerDocument: null,
    maxOcrPagesPerDocument: 20,
    ocrEveryPage: false
  },
  deep: {
    depth: "deep",
    requestedCapabilities: ["extracted-text", "ocr", "chunks", "text-embeddings"],
    maxPagesPerDocument: null,
    maxOcrPagesPerDocument: null,
    ocrEveryPage: true
  }
};
function effectiveKnowledgeDepth(root, settings) {
  return root.depth === "inherit" ? settings.depth : root.depth;
}
function profileForDepth(depth) {
  return depth === "off" ? null : PROFILES[depth];
}
function indexingProfileKey(profile) {
  return JSON.stringify({
    depth: profile.depth,
    capabilities: profile.requestedCapabilities,
    maxPagesPerDocument: profile.maxPagesPerDocument,
    maxOcrPagesPerDocument: profile.maxOcrPagesPerDocument,
    ocrEveryPage: profile.ocrEveryPage
  });
}

// src/main/knowledge/database/store.ts
var DEFAULT_STATUS = {
  state: "idle",
  backend: "local",
  progress: 0,
  queuedSources: 0,
  processedSources: 0,
  failedSources: 0,
  sourceCount: 0,
  chunkCount: 0,
  indexedPageCount: 0,
  totalPageCount: 0,
  partialSourceCount: 0,
  sourceBytes: 0
};
function databasePath() {
  const directory = (0, import_node_path5.join)(app.getPath("userData"), "knowledge");
  (0, import_node_fs5.mkdirSync)(directory, { recursive: true });
  return (0, import_node_path5.join)(directory, "knowledge.sqlite3");
}
function parseJson(value, fallback) {
  if (!value) return fallback;
  try {
    return JSON.parse(value);
  } catch {
    return fallback;
  }
}
function encodeEmbedding(value) {
  const buffer = Buffer.allocUnsafe(value.length * Float32Array.BYTES_PER_ELEMENT);
  for (let index = 0; index < value.length; index += 1) {
    buffer.writeFloatLE(value[index] ?? 0, index * Float32Array.BYTES_PER_ELEMENT);
  }
  return buffer;
}
function encodeBinaryEmbedding(value) {
  const buffer = Buffer.alloc(Math.ceil(value.length / 8));
  for (let index = 0; index < value.length; index += 1) {
    if ((value[index] ?? 0) >= 0) {
      const byteIndex = Math.floor(index / 8);
      buffer[byteIndex] = (buffer[byteIndex] ?? 0) | 1 << index % 8;
    }
  }
  return buffer;
}
function decodeEmbedding(value) {
  if (!value) return void 0;
  if (typeof value === "string") return parseJson(value, void 0);
  const buffer = Buffer.isBuffer(value) ? value : Buffer.from(value.buffer, value.byteOffset, value.byteLength);
  if (buffer.byteLength % Float32Array.BYTES_PER_ELEMENT !== 0) return void 0;
  const result = new Array(buffer.byteLength / Float32Array.BYTES_PER_ELEMENT);
  for (let index = 0; index < result.length; index += 1) {
    result[index] = buffer.readFloatLE(index * Float32Array.BYTES_PER_ELEMENT);
  }
  return result;
}
var singleton = null;
function getKnowledgeStore() {
  singleton ??= new KnowledgeStore();
  return singleton;
}
var KnowledgeStore = class {
  database = null;
  vectorIndexAvailable = false;
  get db() {
    if (!this.database) throw new Error("Knowledge database is not initialized");
    return this.database;
  }
  ensureInitialized() {
    if (this.database) return;
    this.database = new better_sqlite3_shim_default(databasePath());
    this.db.pragma("journal_mode = WAL");
    this.db.pragma("synchronous = NORMAL");
    this.db.pragma("busy_timeout = 5000");
    try {
      load(
        this.db
      );
      this.vectorIndexAvailable = true;
    } catch (error) {
      console.warn("[Knowledge] sqlite-vec unavailable; using compatibility search:", error);
    }
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS knowledge_roots (
        id TEXT PRIMARY KEY,
        path TEXT NOT NULL UNIQUE,
        depth TEXT NOT NULL,
        processing_backend TEXT NOT NULL,
        enabled INTEGER NOT NULL DEFAULT 1,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS knowledge_sources (
        id TEXT PRIMARY KEY,
        root_id TEXT NOT NULL,
        path TEXT NOT NULL UNIQUE,
        content_hash TEXT NOT NULL,
        byte_size INTEGER NOT NULL,
        modified_at INTEGER NOT NULL,
        media_type TEXT,
        status TEXT NOT NULL,
        error TEXT,
        indexing_profile TEXT NOT NULL DEFAULT '',
        total_pages INTEGER,
        indexed_pages INTEGER NOT NULL DEFAULT 0,
        indexed_at INTEGER,
        FOREIGN KEY(root_id) REFERENCES knowledge_roots(id) ON DELETE CASCADE
      );

      CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_sources_fts USING fts5(
        id UNINDEXED,
        path,
        tokenize = 'unicode61'
      );

      CREATE TRIGGER IF NOT EXISTS knowledge_sources_fts_insert
      AFTER INSERT ON knowledge_sources BEGIN
        INSERT INTO knowledge_sources_fts(rowid, id, path) VALUES (new.rowid, new.id, new.path);
      END;

      CREATE TRIGGER IF NOT EXISTS knowledge_sources_fts_delete
      AFTER DELETE ON knowledge_sources BEGIN
        DELETE FROM knowledge_sources_fts WHERE rowid = old.rowid;
      END;

      CREATE TRIGGER IF NOT EXISTS knowledge_sources_fts_update
      AFTER UPDATE OF path ON knowledge_sources BEGIN
        UPDATE knowledge_sources_fts SET id = new.id, path = new.path WHERE rowid = old.rowid;
      END;

      CREATE TABLE IF NOT EXISTS knowledge_pages (
        source_id TEXT NOT NULL,
        page_number INTEGER NOT NULL,
        extracted_text TEXT,
        ocr_text TEXT,
        ocr_blocks_json TEXT,
        ocr_confidence REAL,
        extraction_status TEXT NOT NULL DEFAULT 'not-indexed',
        ocr_status TEXT NOT NULL DEFAULT 'not-indexed',
        embedding_status TEXT NOT NULL DEFAULT 'not-indexed',
        image_embedding_status TEXT NOT NULL DEFAULT 'not-indexed',
        content_hash TEXT,
        processing_cost_ms INTEGER,
        last_accessed_at INTEGER,
        PRIMARY KEY(source_id, page_number),
        FOREIGN KEY(source_id) REFERENCES knowledge_sources(id) ON DELETE CASCADE
      );

      CREATE TABLE IF NOT EXISTS knowledge_chunks (
        id TEXT PRIMARY KEY,
        source_id TEXT NOT NULL,
        page_number INTEGER,
        text TEXT NOT NULL,
        embedding_json TEXT,
        start_offset INTEGER,
        end_offset INTEGER,
        FOREIGN KEY(source_id) REFERENCES knowledge_sources(id) ON DELETE CASCADE
      );

      CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_chunks_fts USING fts5(
        id UNINDEXED,
        source_id UNINDEXED,
        text,
        tokenize = 'unicode61'
      );

      CREATE TABLE IF NOT EXISTS knowledge_images (
        id TEXT PRIMARY KEY,
        source_id TEXT NOT NULL,
        page_number INTEGER,
        ocr_text TEXT,
        description TEXT,
        embedding_json TEXT,
        width INTEGER NOT NULL,
        height INTEGER NOT NULL,
        FOREIGN KEY(source_id) REFERENCES knowledge_sources(id) ON DELETE CASCADE
      );

      CREATE TABLE IF NOT EXISTS knowledge_artifacts (
        id TEXT PRIMARY KEY,
        source_hash TEXT NOT NULL,
        type TEXT NOT NULL,
        processor_id TEXT NOT NULL,
        processor_version TEXT NOT NULL,
        settings_hash TEXT NOT NULL,
        created_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS knowledge_metadata (
        key TEXT PRIMARY KEY,
        value_json TEXT NOT NULL
      );

      CREATE TABLE IF NOT EXISTS knowledge_stats (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        source_count INTEGER NOT NULL DEFAULT 0,
        chunk_count INTEGER NOT NULL DEFAULT 0,
        indexed_page_count INTEGER NOT NULL DEFAULT 0,
        total_page_count INTEGER NOT NULL DEFAULT 0,
        partial_source_count INTEGER NOT NULL DEFAULT 0,
        source_bytes INTEGER NOT NULL DEFAULT 0
      );

      CREATE TABLE IF NOT EXISTS knowledge_indexing_jobs (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        job_id TEXT NOT NULL,
        total_sources INTEGER NOT NULL,
        processed_sources INTEGER NOT NULL DEFAULT 0,
        failed_sources INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS knowledge_indexing_queue (
        job_id TEXT NOT NULL,
        position INTEGER NOT NULL,
        root_id TEXT NOT NULL,
        path TEXT NOT NULL,
        PRIMARY KEY(job_id, position)
      );

      CREATE INDEX IF NOT EXISTS knowledge_sources_root_idx ON knowledge_sources(root_id);
      CREATE INDEX IF NOT EXISTS knowledge_chunks_source_idx ON knowledge_chunks(source_id);
      CREATE INDEX IF NOT EXISTS knowledge_indexing_queue_job_idx
        ON knowledge_indexing_queue(job_id, position);
    `);
    this.ensureColumn("knowledge_sources", "indexing_profile", "TEXT NOT NULL DEFAULT ''");
    this.ensureColumn("knowledge_sources", "total_pages", "INTEGER");
    this.ensureColumn("knowledge_sources", "indexed_pages", "INTEGER NOT NULL DEFAULT 0");
    const sourceCount = this.db.prepare("SELECT COUNT(*) AS count FROM knowledge_sources").get().count;
    const metadataIndexCount = this.db.prepare("SELECT COUNT(*) AS count FROM knowledge_sources_fts").get().count;
    if (sourceCount !== metadataIndexCount) {
      this.db.exec(`
        DELETE FROM knowledge_sources_fts;
        INSERT INTO knowledge_sources_fts(rowid, id, path)
        SELECT rowid, id, path FROM knowledge_sources;
      `);
    }
    this.db.exec(`
      CREATE INDEX IF NOT EXISTS knowledge_sources_reuse_idx
        ON knowledge_sources(content_hash, indexing_profile, status, indexed_at DESC)
    `);
    this.ensureColumn("knowledge_pages", "extraction_status", "TEXT NOT NULL DEFAULT 'not-indexed'");
    this.ensureColumn("knowledge_pages", "ocr_status", "TEXT NOT NULL DEFAULT 'not-indexed'");
    this.ensureColumn("knowledge_pages", "embedding_status", "TEXT NOT NULL DEFAULT 'not-indexed'");
    this.ensureColumn(
      "knowledge_pages",
      "image_embedding_status",
      "TEXT NOT NULL DEFAULT 'not-indexed'"
    );
    this.ensureColumn("knowledge_pages", "content_hash", "TEXT");
    this.ensureColumn("knowledge_pages", "processing_cost_ms", "INTEGER");
    this.ensureColumn("knowledge_pages", "last_accessed_at", "INTEGER");
    this.db.prepare(
      `
      UPDATE knowledge_roots SET depth = 'inherit'
      WHERE depth IN ('text', 'text-and-images')
    `
    ).run();
    this.ensureMaterializedStats();
    this.ensureVectorIndex();
  }
  ensureVectorIndex() {
    if (!this.vectorIndexAvailable) return;
    this.db.exec(`
      CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_chunk_vectors_binary USING vec0(
        embedding bit[${FEATURE_EMBEDDING_MODEL.dimensions}],
        root_id text
      );
    `);
    const cutoff = this.db.prepare(
      "SELECT value_json AS valueJson FROM knowledge_metadata WHERE key = 'vector-binary-backfill-max-rowid-v1'"
    ).get();
    if (!cutoff) {
      const maximumRow = this.db.prepare(
        "SELECT COALESCE(MAX(rowid), 0) AS maximumRowId FROM knowledge_chunks WHERE embedding_json IS NOT NULL"
      ).get();
      this.db.prepare(
        `INSERT INTO knowledge_metadata (key, value_json)
           VALUES ('vector-binary-backfill-max-rowid-v1', ?)`
      ).run(JSON.stringify(maximumRow.maximumRowId));
    }
  }
  /**
   * Incrementally migrates embeddings created before the sqlite-vec index was
   * introduced. Keeping this bounded is critical: large existing libraries
   * must not delay the backend IPC connection during app startup.
   */
  backfillVectorIndexBatch(limit = 128) {
    this.ensureInitialized();
    if (!this.vectorIndexAvailable) return { processed: 0, hasMore: false };
    const batchSize = Math.max(1, Math.min(1e3, Math.round(limit)));
    const cursorRow = this.db.prepare(
      "SELECT value_json AS valueJson FROM knowledge_metadata WHERE key = 'vector-binary-backfill-cursor-v1'"
    ).get();
    const cursor = Math.max(0, parseJson(cursorRow?.valueJson, 0));
    const cutoffRow = this.db.prepare(
      "SELECT value_json AS valueJson FROM knowledge_metadata WHERE key = 'vector-binary-backfill-max-rowid-v1'"
    ).get();
    const cutoff = Math.max(cursor, parseJson(cutoffRow?.valueJson, cursor));
    const rows = this.db.prepare(
      `
      SELECT c.rowid AS rowId, c.embedding_json AS embeddingJson, s.root_id AS rootId
      FROM knowledge_chunks c
      JOIN knowledge_sources s ON s.id = c.source_id
      WHERE c.rowid > ? AND c.rowid <= ? AND c.embedding_json IS NOT NULL
      ORDER BY c.rowid ASC
      LIMIT ?
    `
    ).all(cursor, cutoff, batchSize);
    if (rows.length === 0) return { processed: 0, hasMore: false };
    let processed = 0;
    const migrate = this.db.transaction(() => {
      const insert = this.db.prepare(
        `INSERT INTO knowledge_chunk_vectors_binary(rowid, embedding, root_id)
         VALUES (?, vec_bit(?), ?)`
      );
      for (const row of rows) {
        const embedding = decodeEmbedding(row.embeddingJson);
        if (embedding?.length !== FEATURE_EMBEDDING_MODEL.dimensions) continue;
        const rowId = BigInt(row.rowId);
        insert.run(rowId, encodeBinaryEmbedding(embedding), row.rootId);
        processed += 1;
      }
      this.db.prepare(
        `
        INSERT INTO knowledge_metadata (key, value_json)
        VALUES ('vector-binary-backfill-cursor-v1', ?)
        ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json
      `
      ).run(JSON.stringify(rows.at(-1)?.rowId ?? cursor));
    });
    migrate();
    return { processed, hasMore: true };
  }
  deleteVectorsForSource(sourceId) {
    if (!this.vectorIndexAvailable) return;
    const rows = this.db.prepare("SELECT rowid AS rowId FROM knowledge_chunks WHERE source_id = ?").all(sourceId);
    const remove = this.db.prepare("DELETE FROM knowledge_chunk_vectors_binary WHERE rowid = ?");
    for (const row of rows) remove.run(BigInt(row.rowId));
  }
  /**
   * Keep dashboard counters in one tiny row instead of repeatedly scanning the
   * multi-gigabyte chunks table. The previous COUNT(*) calls ran on every
   * indexing progress update and could monopolize the backend for minutes.
   *
   * Existing installs are seeded from the already-persisted status snapshot,
   * so this migration itself never performs a full-table scan. Triggers keep
   * the counters exact for all subsequent source/chunk mutations.
   */
  ensureMaterializedStats() {
    const existing = this.db.prepare("SELECT id FROM knowledge_stats WHERE id = 1").get();
    if (!existing) {
      const row = this.db.prepare("SELECT value_json AS valueJson FROM knowledge_metadata WHERE key = 'status'").get();
      const status = parseJson(row?.valueJson, {});
      const count = (value) => {
        const numeric = typeof value === "number" && Number.isFinite(value) ? value : 0;
        return Math.max(0, Math.round(numeric));
      };
      this.db.prepare(
        `
        INSERT INTO knowledge_stats
          (id, source_count, chunk_count, indexed_page_count, total_page_count,
           partial_source_count, source_bytes)
        VALUES (1, ?, ?, ?, ?, ?, ?)
      `
      ).run(
        count(status.sourceCount),
        count(status.chunkCount),
        count(status.indexedPageCount),
        count(status.totalPageCount),
        count(status.partialSourceCount),
        count(status.sourceBytes)
      );
    }
    this.db.exec(`
      CREATE TRIGGER IF NOT EXISTS knowledge_stats_source_insert
      AFTER INSERT ON knowledge_sources
      BEGIN
        UPDATE knowledge_stats SET
          source_count = source_count + CASE WHEN NEW.status = 'indexed' THEN 1 ELSE 0 END,
          indexed_page_count = indexed_page_count + CASE WHEN NEW.status = 'indexed' THEN COALESCE(NEW.indexed_pages, 0) ELSE 0 END,
          total_page_count = total_page_count + CASE WHEN NEW.status = 'indexed' THEN COALESCE(NEW.total_pages, 0) ELSE 0 END,
          partial_source_count = partial_source_count + CASE WHEN NEW.status = 'indexed' AND COALESCE(NEW.total_pages, 0) > COALESCE(NEW.indexed_pages, 0) THEN 1 ELSE 0 END,
          source_bytes = source_bytes + CASE WHEN NEW.status = 'indexed' THEN COALESCE(NEW.byte_size, 0) ELSE 0 END
        WHERE id = 1;
      END;

      CREATE TRIGGER IF NOT EXISTS knowledge_stats_source_delete
      AFTER DELETE ON knowledge_sources
      BEGIN
        UPDATE knowledge_stats SET
          source_count = MAX(0, source_count - CASE WHEN OLD.status = 'indexed' THEN 1 ELSE 0 END),
          indexed_page_count = MAX(0, indexed_page_count - CASE WHEN OLD.status = 'indexed' THEN COALESCE(OLD.indexed_pages, 0) ELSE 0 END),
          total_page_count = MAX(0, total_page_count - CASE WHEN OLD.status = 'indexed' THEN COALESCE(OLD.total_pages, 0) ELSE 0 END),
          partial_source_count = MAX(0, partial_source_count - CASE WHEN OLD.status = 'indexed' AND COALESCE(OLD.total_pages, 0) > COALESCE(OLD.indexed_pages, 0) THEN 1 ELSE 0 END),
          source_bytes = MAX(0, source_bytes - CASE WHEN OLD.status = 'indexed' THEN COALESCE(OLD.byte_size, 0) ELSE 0 END)
        WHERE id = 1;
      END;

      CREATE TRIGGER IF NOT EXISTS knowledge_stats_source_update
      AFTER UPDATE OF status, indexed_pages, total_pages, byte_size ON knowledge_sources
      BEGIN
        UPDATE knowledge_stats SET
          source_count = MAX(0, source_count
            - CASE WHEN OLD.status = 'indexed' THEN 1 ELSE 0 END
            + CASE WHEN NEW.status = 'indexed' THEN 1 ELSE 0 END),
          indexed_page_count = MAX(0, indexed_page_count
            - CASE WHEN OLD.status = 'indexed' THEN COALESCE(OLD.indexed_pages, 0) ELSE 0 END
            + CASE WHEN NEW.status = 'indexed' THEN COALESCE(NEW.indexed_pages, 0) ELSE 0 END),
          total_page_count = MAX(0, total_page_count
            - CASE WHEN OLD.status = 'indexed' THEN COALESCE(OLD.total_pages, 0) ELSE 0 END
            + CASE WHEN NEW.status = 'indexed' THEN COALESCE(NEW.total_pages, 0) ELSE 0 END),
          partial_source_count = MAX(0, partial_source_count
            - CASE WHEN OLD.status = 'indexed' AND COALESCE(OLD.total_pages, 0) > COALESCE(OLD.indexed_pages, 0) THEN 1 ELSE 0 END
            + CASE WHEN NEW.status = 'indexed' AND COALESCE(NEW.total_pages, 0) > COALESCE(NEW.indexed_pages, 0) THEN 1 ELSE 0 END),
          source_bytes = MAX(0, source_bytes
            - CASE WHEN OLD.status = 'indexed' THEN COALESCE(OLD.byte_size, 0) ELSE 0 END
            + CASE WHEN NEW.status = 'indexed' THEN COALESCE(NEW.byte_size, 0) ELSE 0 END)
        WHERE id = 1;
      END;

      CREATE TRIGGER IF NOT EXISTS knowledge_stats_chunk_insert
      AFTER INSERT ON knowledge_chunks
      BEGIN
        UPDATE knowledge_stats SET chunk_count = chunk_count + 1 WHERE id = 1;
      END;

      CREATE TRIGGER IF NOT EXISTS knowledge_stats_chunk_delete
      AFTER DELETE ON knowledge_chunks
      BEGIN
        UPDATE knowledge_stats SET chunk_count = MAX(0, chunk_count - 1) WHERE id = 1;
      END;
    `);
  }
  ensureColumn(table, column, definition) {
    const columns = this.db.prepare(`PRAGMA table_info(${table})`).all();
    if (columns.some((candidate) => candidate.name === column)) return;
    this.db.exec(`ALTER TABLE ${table} ADD COLUMN ${column} ${definition}`);
  }
  listRoots() {
    this.ensureInitialized();
    const rows = this.db.prepare(
      `
      SELECT id, path, depth, processing_backend AS processingBackend,
             enabled, created_at AS createdAt, updated_at AS updatedAt
      FROM knowledge_roots ORDER BY created_at ASC
    `
    ).all();
    return rows.map((row) => ({ ...row, enabled: row.enabled === 1 }));
  }
  upsertRoot(root) {
    this.ensureInitialized();
    this.db.prepare(
      `
      INSERT INTO knowledge_roots
        (id, path, depth, processing_backend, enabled, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, ?, ?)
      ON CONFLICT(id) DO UPDATE SET
        path = excluded.path,
        depth = excluded.depth,
        processing_backend = excluded.processing_backend,
        enabled = excluded.enabled,
        updated_at = excluded.updated_at
    `
    ).run(
      root.id,
      root.path,
      root.depth,
      root.processingBackend,
      root.enabled ? 1 : 0,
      root.createdAt,
      root.updatedAt
    );
  }
  removeRoot(rootId) {
    this.ensureInitialized();
    const sourceIds = this.db.prepare("SELECT id FROM knowledge_sources WHERE root_id = ?").all(rootId);
    const transaction = this.db.transaction(() => {
      for (const source of sourceIds) this.removeSource(source.id);
      this.db.prepare("DELETE FROM knowledge_roots WHERE id = ?").run(rootId);
    });
    transaction();
  }
  getSourceByPath(path) {
    this.ensureInitialized();
    return this.db.prepare(
      `
      SELECT id, root_id AS rootId, path, content_hash AS contentHash,
             byte_size AS byteSize, modified_at AS modifiedAt, status, error,
             indexing_profile AS indexingProfile
      FROM knowledge_sources WHERE path = ?
    `
    ).get(path) ?? null;
  }
  markSourcePending(input) {
    this.ensureInitialized();
    this.db.prepare(
      `
      INSERT INTO knowledge_sources
        (id, root_id, path, content_hash, byte_size, modified_at, media_type, status, error,
         indexing_profile)
      VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', NULL, ?)
      ON CONFLICT(id) DO UPDATE SET
        root_id = excluded.root_id,
        path = excluded.path,
        content_hash = excluded.content_hash,
        byte_size = excluded.byte_size,
        modified_at = excluded.modified_at,
        media_type = excluded.media_type,
        indexing_profile = excluded.indexing_profile,
        status = 'pending',
        error = NULL
    `
    ).run(
      input.id,
      input.rootId,
      input.path,
      input.fingerprint.contentHash,
      input.fingerprint.byteSize,
      input.fingerprint.modifiedAt,
      input.mediaType ?? null,
      input.indexingProfile
    );
  }
  markSourceFailed(sourceId, error) {
    this.ensureInitialized();
    this.db.prepare(
      `
      UPDATE knowledge_sources SET status = 'failed', error = ?, indexed_at = ? WHERE id = ?
    `
    ).run(error.slice(0, 2e3), Date.now(), sourceId);
  }
  saveResult(rootId, path, fingerprint, indexingProfile, result) {
    this.ensureInitialized();
    const deleteFts = this.db.prepare(`
      DELETE FROM knowledge_chunks_fts
      WHERE rowid IN (SELECT rowid FROM knowledge_chunks WHERE source_id = ?)
        AND source_id = ?
    `);
    const embeddedPages = new Set(
      result.chunks.filter((chunk) => chunk.embedding).map((chunk) => chunk.pageNumber ?? 1)
    );
    const imageEmbeddedPages = new Set(
      result.images.filter((image) => image.embedding).map((image) => image.pageNumber ?? 1)
    );
    const transaction = this.db.transaction(() => {
      this.db.prepare(
        `
        INSERT INTO knowledge_sources
          (id, root_id, path, content_hash, byte_size, modified_at, status, error,
           indexing_profile, total_pages, indexed_pages, indexed_at)
        VALUES (?, ?, ?, ?, ?, ?, 'indexed', NULL, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
          root_id = excluded.root_id,
          path = excluded.path,
          content_hash = excluded.content_hash,
          byte_size = excluded.byte_size,
          modified_at = excluded.modified_at,
          indexing_profile = excluded.indexing_profile,
          total_pages = excluded.total_pages,
          indexed_pages = excluded.indexed_pages,
          status = 'indexed',
          error = NULL,
          indexed_at = excluded.indexed_at
      `
      ).run(
        result.sourceId,
        rootId,
        path,
        fingerprint.contentHash,
        fingerprint.byteSize,
        fingerprint.modifiedAt,
        indexingProfile,
        result.totalPageCount ?? result.pages.length,
        result.pages.length,
        Date.now()
      );
      deleteFts.run(result.sourceId, result.sourceId);
      this.deleteVectorsForSource(result.sourceId);
      this.db.prepare("DELETE FROM knowledge_pages WHERE source_id = ?").run(result.sourceId);
      this.db.prepare("DELETE FROM knowledge_chunks WHERE source_id = ?").run(result.sourceId);
      this.db.prepare("DELETE FROM knowledge_images WHERE source_id = ?").run(result.sourceId);
      const insertPage = this.db.prepare(`
        INSERT INTO knowledge_pages
          (source_id, page_number, extracted_text, ocr_text, ocr_blocks_json, ocr_confidence,
           extraction_status, ocr_status, embedding_status, image_embedding_status,
           content_hash, processing_cost_ms, last_accessed_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL)
      `);
      for (const page of result.pages) {
        const pageText = `${page.extractedText ?? ""}
${page.ocr?.text ?? ""}`.trim();
        insertPage.run(
          result.sourceId,
          page.pageNumber,
          page.extractedText ?? null,
          page.ocr?.text ?? null,
          page.ocr ? JSON.stringify(page.ocr.blocks) : null,
          page.ocr?.averageConfidence ?? null,
          page.extractedText ? "text-indexed" : "metadata-only",
          page.ocr ? "ocr-indexed" : "not-indexed",
          embeddedPages.has(page.pageNumber) ? "embedded" : "not-indexed",
          imageEmbeddedPages.has(page.pageNumber) ? "embedded" : "not-indexed",
          pageText ? (0, import_node_crypto3.createHash)("sha256").update(pageText).digest("hex") : null
        );
      }
      const insertChunk = this.db.prepare(`
        INSERT INTO knowledge_chunks
          (id, source_id, page_number, text, embedding_json, start_offset, end_offset)
        VALUES (?, ?, ?, ?, ?, ?, ?)
      `);
      const insertChunkFts = this.db.prepare(`
        INSERT INTO knowledge_chunks_fts (rowid, id, source_id, text) VALUES (?, ?, ?, ?)
      `);
      const insertChunkVector = this.vectorIndexAvailable ? this.db.prepare(
        `INSERT INTO knowledge_chunk_vectors_binary(rowid, embedding, root_id)
             VALUES (?, vec_bit(?), ?)`
      ) : null;
      for (const chunk of result.chunks) {
        const inserted = insertChunk.run(
          chunk.id,
          result.sourceId,
          chunk.pageNumber ?? null,
          chunk.text,
          chunk.embedding ? encodeEmbedding(chunk.embedding) : null,
          chunk.startOffset ?? null,
          chunk.endOffset ?? null
        );
        insertChunkFts.run(inserted.lastInsertRowid, chunk.id, result.sourceId, chunk.text);
        if (chunk.embedding?.length === FEATURE_EMBEDDING_MODEL.dimensions) {
          insertChunkVector?.run(
            BigInt(inserted.lastInsertRowid),
            encodeBinaryEmbedding(chunk.embedding),
            rootId
          );
        }
      }
      const insertImage = this.db.prepare(`
        INSERT INTO knowledge_images
          (id, source_id, page_number, ocr_text, description, embedding_json, width, height)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      `);
      for (const image of result.images) {
        insertImage.run(
          image.id,
          result.sourceId,
          image.pageNumber ?? null,
          image.ocrText ?? null,
          image.description ?? null,
          image.embedding ? encodeEmbedding(image.embedding) : null,
          image.width,
          image.height
        );
      }
      const insertArtifact = this.db.prepare(`
        INSERT OR IGNORE INTO knowledge_artifacts
          (id, source_hash, type, processor_id, processor_version, settings_hash, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
      `);
      for (const capability of result.completedCapabilities) {
        const processor = capability === "text-embeddings" && result.textEmbeddingModel ? result.textEmbeddingModel : result.extractor;
        const settingsHash = artifactSettingsHash({ capability, processor });
        const artifactId = (0, import_node_crypto3.createHash)("sha256").update(
          `${result.sourceHash}:${capability}:${processor.id}:${processor.version}:${settingsHash}`
        ).digest("hex");
        insertArtifact.run(
          artifactId,
          result.sourceHash,
          capability,
          processor.id,
          processor.version,
          settingsHash,
          Date.now()
        );
      }
    });
    transaction();
  }
  findReusableResult(sourceHash, targetSourceId, indexingProfile) {
    this.ensureInitialized();
    const source = this.db.prepare(
      `
      SELECT id, total_pages AS totalPages FROM knowledge_sources
      WHERE content_hash = ? AND indexing_profile = ? AND status = 'indexed' AND id <> ?
      ORDER BY indexed_at DESC LIMIT 1
    `
    ).get(sourceHash, indexingProfile, targetSourceId);
    if (!source) return null;
    const pages = this.db.prepare(
      `
      SELECT page_number AS pageNumber, extracted_text AS extractedText,
             ocr_text AS ocrText, ocr_blocks_json AS ocrBlocksJson,
             ocr_confidence AS ocrConfidence
      FROM knowledge_pages WHERE source_id = ? ORDER BY page_number ASC
    `
    ).all(source.id);
    const chunks = this.db.prepare(
      `
      SELECT id, page_number AS pageNumber, text, embedding_json AS embeddingJson,
             start_offset AS startOffset, end_offset AS endOffset
      FROM knowledge_chunks WHERE source_id = ? ORDER BY rowid ASC
    `
    ).all(source.id);
    const images = this.db.prepare(
      `
      SELECT id, page_number AS pageNumber, ocr_text AS ocrText, description,
             embedding_json AS embeddingJson, width, height
      FROM knowledge_images WHERE source_id = ? ORDER BY rowid ASC
    `
    ).all(source.id);
    const hasOcr = pages.some((page) => Boolean(page.ocrText)) || images.some((image) => Boolean(image.ocrText));
    const hasEmbeddings = chunks.some((chunk) => Boolean(chunk.embeddingJson));
    return {
      sourceId: targetSourceId,
      sourceHash,
      extractor: { id: "content-addressed-cache", version: "1.0.0" },
      textEmbeddingModel: hasEmbeddings ? FEATURE_EMBEDDING_MODEL : void 0,
      pages: pages.map((page) => ({
        pageNumber: page.pageNumber,
        extractedText: page.extractedText ?? void 0,
        ocr: page.ocrText ? {
          text: page.ocrText,
          blocks: parseJson(page.ocrBlocksJson, []),
          averageConfidence: page.ocrConfidence ?? void 0
        } : void 0
      })),
      chunks: chunks.map((chunk) => ({
        id: (0, import_node_crypto3.createHash)("sha256").update(`${targetSourceId}:${chunk.id}`).digest("hex"),
        pageNumber: chunk.pageNumber ?? void 0,
        text: chunk.text,
        embedding: decodeEmbedding(chunk.embeddingJson),
        startOffset: chunk.startOffset ?? void 0,
        endOffset: chunk.endOffset ?? void 0
      })),
      images: images.map((image) => ({
        id: (0, import_node_crypto3.createHash)("sha256").update(`${targetSourceId}:${image.id}`).digest("hex"),
        pageNumber: image.pageNumber ?? void 0,
        ocrText: image.ocrText ?? void 0,
        description: image.description ?? void 0,
        embedding: decodeEmbedding(image.embeddingJson),
        width: image.width,
        height: image.height
      })),
      completedCapabilities: [
        "extracted-text",
        ...hasOcr ? ["ocr"] : [],
        "chunks",
        ...hasEmbeddings ? ["text-embeddings"] : []
      ],
      warnings: ["Reused locally cached artifacts for identical content."],
      totalPageCount: source.totalPages ?? pages.length
    };
  }
  removeSource(sourceId) {
    this.ensureInitialized();
    this.db.prepare(
      `
        DELETE FROM knowledge_chunks_fts
        WHERE rowid IN (SELECT rowid FROM knowledge_chunks WHERE source_id = ?)
          AND source_id = ?
      `
    ).run(sourceId, sourceId);
    this.deleteVectorsForSource(sourceId);
    this.db.prepare("DELETE FROM knowledge_pages WHERE source_id = ?").run(sourceId);
    this.db.prepare("DELETE FROM knowledge_chunks WHERE source_id = ?").run(sourceId);
    this.db.prepare("DELETE FROM knowledge_images WHERE source_id = ?").run(sourceId);
    this.db.prepare("DELETE FROM knowledge_sources WHERE id = ?").run(sourceId);
  }
  removeMissingSources(rootId, presentPaths) {
    this.ensureInitialized();
    const rows = this.db.prepare("SELECT id, path FROM knowledge_sources WHERE root_id = ?").all(rootId);
    let removed = 0;
    for (const row of rows) {
      if (presentPaths.has(row.path)) continue;
      this.removeSource(row.id);
      removed += 1;
    }
    return removed;
  }
  counts() {
    this.ensureInitialized();
    const stats = this.db.prepare(
      `
      SELECT source_count AS sourceCount,
             chunk_count AS chunkCount,
             indexed_page_count AS indexedPageCount,
             total_page_count AS totalPageCount,
             partial_source_count AS partialSourceCount,
             source_bytes AS sourceBytes
      FROM knowledge_stats WHERE id = 1
    `
    ).get();
    return stats;
  }
  storageBytes() {
    this.ensureInitialized();
    const path = databasePath();
    return [path, `${path}-wal`, `${path}-shm`].reduce((total, candidate) => {
      try {
        return total + (0, import_node_fs5.statSync)(candidate).size;
      } catch {
        return total;
      }
    }, 0);
  }
  listSources(input) {
    this.ensureInitialized();
    const query = input?.query?.trim() ?? "";
    const offset = Math.max(0, Math.round(input?.offset ?? 0));
    const limit = Math.max(1, Math.min(500, Math.round(input?.limit ?? 200)));
    const where = query ? "WHERE path LIKE ?" : "";
    const parameters = query ? [`%${query}%`] : [];
    const total = this.db.prepare(
      `
      SELECT COUNT(*) AS count FROM knowledge_sources ${where}
    `
    ).get(...parameters);
    const rows = this.db.prepare(
      `
      SELECT id, path, byte_size AS byteSize, modified_at AS modifiedAt,
             indexed_at AS indexedAt, status, error,
             COALESCE(total_pages, 0) AS totalPageCount,
             COALESCE(indexed_pages, 0) AS indexedPageCount
      FROM knowledge_sources ${where}
      ORDER BY CASE status WHEN 'pending' THEN 0 WHEN 'failed' THEN 1 ELSE 2 END,
               indexed_at DESC, path ASC
      LIMIT ? OFFSET ?
    `
    ).all(...parameters, limit, offset);
    return {
      sources: rows.map((row) => ({
        ...row,
        title: (0, import_node_path5.basename)(row.path),
        indexedAt: row.indexedAt ?? void 0,
        error: row.error ?? void 0
      })),
      total: total.count,
      offset,
      hasMore: offset + rows.length < total.count
    };
  }
  searchMetadata(query, limit = 20, rootIds) {
    this.ensureInitialized();
    const trimmed = query.trim();
    const ftsQuery = buildContentFtsQuery(trimmed);
    if (!ftsQuery || limit <= 0 || rootIds?.length === 0) return [];
    const rootFilter = rootIds ? `AND r.id IN (${rootIds.map(() => "?").join(", ")})` : "";
    const rows = this.db.prepare(
      `
      SELECT s.id AS sourceId, s.path, bm25(knowledge_sources_fts) AS rank
      FROM knowledge_sources_fts f
      JOIN knowledge_sources s ON s.id = f.id
      JOIN knowledge_roots r ON r.id = s.root_id
      WHERE knowledge_sources_fts MATCH ? AND r.enabled = 1 ${rootFilter}
      ORDER BY rank ASC
      LIMIT ?
    `
    ).all(ftsQuery, ...rootIds ?? [], limit);
    return rows.map((row, index) => ({
      sourceId: row.sourceId,
      path: row.path,
      title: (0, import_node_path5.basename)(row.path),
      score: Math.max(0.2, 1 - index / Math.max(rows.length, 1))
    }));
  }
  getPersistedStatus() {
    this.ensureInitialized();
    const row = this.db.prepare("SELECT value_json AS valueJson FROM knowledge_metadata WHERE key = 'status'").get();
    const counts = this.counts();
    return { ...DEFAULT_STATUS, ...parseJson(row?.valueJson, {}), ...counts };
  }
  saveStatus(status) {
    this.ensureInitialized();
    this.db.prepare(
      `
      INSERT INTO knowledge_metadata (key, value_json) VALUES ('status', ?)
      ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json
    `
    ).run(JSON.stringify(status));
  }
  saveIndexingCheckpoint(jobId, candidates) {
    this.ensureInitialized();
    const insertCandidate = this.db.prepare(
      `
      INSERT INTO knowledge_indexing_queue (job_id, position, root_id, path)
      VALUES (?, ?, ?, ?)
    `
    );
    const transaction = this.db.transaction(() => {
      this.db.prepare("DELETE FROM knowledge_indexing_queue").run();
      this.db.prepare("DELETE FROM knowledge_indexing_jobs").run();
      this.db.prepare(
        `
        INSERT INTO knowledge_indexing_jobs
          (id, job_id, total_sources, processed_sources, failed_sources, created_at)
        VALUES (1, ?, ?, 0, 0, ?)
      `
      ).run(jobId, candidates.length, Date.now());
      candidates.forEach((candidate, position) => {
        insertCandidate.run(jobId, position, candidate.rootId, candidate.path);
      });
    });
    transaction();
    return this.getIndexingCheckpoint();
  }
  getIndexingCheckpoint() {
    this.ensureInitialized();
    const job = this.db.prepare(
      `
      SELECT job_id AS jobId, total_sources AS totalSources,
             processed_sources AS processedSources, failed_sources AS failedSources
      FROM knowledge_indexing_jobs WHERE id = 1
    `
    ).get();
    if (!job) return null;
    const candidates = this.db.prepare(
      `
      SELECT position, root_id AS rootId, path
      FROM knowledge_indexing_queue
      WHERE job_id = ?
      ORDER BY position ASC
    `
    ).all(job.jobId);
    return { ...job, candidates };
  }
  completeIndexingCandidate(jobId, position, failed) {
    this.ensureInitialized();
    let completed = false;
    const transaction = this.db.transaction(() => {
      const removed = this.db.prepare("DELETE FROM knowledge_indexing_queue WHERE job_id = ? AND position = ?").run(jobId, position);
      if (removed.changes === 0) return;
      this.db.prepare(
        `
        UPDATE knowledge_indexing_jobs
        SET processed_sources = processed_sources + 1,
            failed_sources = failed_sources + ?
        WHERE id = 1 AND job_id = ?
      `
      ).run(failed ? 1 : 0, jobId);
      completed = true;
    });
    transaction();
    return completed;
  }
  clearIndexingCheckpoint(jobId) {
    this.ensureInitialized();
    const transaction = this.db.transaction(() => {
      if (jobId) {
        this.db.prepare("DELETE FROM knowledge_indexing_queue WHERE job_id = ?").run(jobId);
        this.db.prepare("DELETE FROM knowledge_indexing_jobs WHERE id = 1 AND job_id = ?").run(jobId);
        return;
      }
      this.db.prepare("DELETE FROM knowledge_indexing_queue").run();
      this.db.prepare("DELETE FROM knowledge_indexing_jobs").run();
    });
    transaction();
  }
  getSettings() {
    this.ensureInitialized();
    const row = this.db.prepare("SELECT value_json AS valueJson FROM knowledge_metadata WHERE key = 'settings'").get();
    const persisted = parseJson(row?.valueJson, {});
    const depth = ["off", "basic", "smart", "deep"].includes(persisted.depth ?? "") ? persisted.depth : DEFAULT_KNOWLEDGE_SETTINGS.depth;
    return { ...DEFAULT_KNOWLEDGE_SETTINGS, ...persisted, depth };
  }
  saveSettings(settings) {
    this.ensureInitialized();
    this.db.prepare(
      `
      INSERT INTO knowledge_metadata (key, value_json) VALUES ('settings', ?)
      ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json
    `
    ).run(JSON.stringify(settings));
  }
  search(query, limit = 12, rootIds) {
    this.ensureInitialized();
    const trimmed = query.trim();
    if (!trimmed || limit <= 0 || rootIds?.length === 0) return [];
    const rootFilter = rootIds ? `AND r.id IN (${rootIds.map(() => "?").join(", ")})` : "";
    const ftsQuery = buildContentFtsQuery(trimmed);
    const lexicalRows = ftsQuery ? this.db.prepare(
      `
          SELECT c.id, c.source_id AS sourceId, s.path, c.page_number AS pageNumber,
                 c.text, c.embedding_json AS embeddingJson,
                 bm25(knowledge_chunks_fts, 1.0) AS rank
          FROM knowledge_chunks_fts f
          JOIN knowledge_chunks c ON c.rowid = f.rowid
          JOIN knowledge_sources s ON s.id = c.source_id
          JOIN knowledge_roots r ON r.id = s.root_id
          WHERE knowledge_chunks_fts MATCH ? AND r.enabled = 1 ${rootFilter}
          LIMIT ?
        `
    ).all(ftsQuery, ...rootIds ?? [], Math.max(40, limit * 5)) : [];
    lexicalRows.sort((left, right) => (left.rank ?? 0) - (right.rank ?? 0));
    const queryEmbedding = embedText(trimmed);
    const semanticRows = this.vectorIndexAvailable ? this.db.prepare(
      `
          WITH nearest AS (
            SELECT rowid, distance
            FROM knowledge_chunk_vectors_binary
            WHERE embedding MATCH vec_bit(?) AND k = ?
            ${rootIds ? `AND root_id IN (${rootIds.map(() => "?").join(", ")})` : ""}
          )
          SELECT c.id, c.source_id AS sourceId, s.path, c.page_number AS pageNumber,
                 c.text, c.embedding_json AS embeddingJson, nearest.distance
          FROM nearest
          JOIN knowledge_chunks c ON c.rowid = nearest.rowid
          JOIN knowledge_sources s ON s.id = c.source_id
          JOIN knowledge_roots r ON r.id = s.root_id
          WHERE s.status = 'indexed' AND r.enabled = 1
          ORDER BY nearest.distance ASC
        `
    ).all(
      encodeBinaryEmbedding(queryEmbedding),
      Math.max(320, limit * 32),
      ...rootIds ?? []
    ) : this.db.prepare(
      `
          SELECT c.id, c.source_id AS sourceId, s.path, c.page_number AS pageNumber,
                 c.text, c.embedding_json AS embeddingJson
          FROM knowledge_chunks c
          JOIN knowledge_sources s ON s.id = c.source_id
          JOIN knowledge_roots r ON r.id = s.root_id
          WHERE c.embedding_json IS NOT NULL AND s.status = 'indexed' AND r.enabled = 1
          ${rootFilter}
          ORDER BY c.rowid DESC LIMIT 1200
        `
    ).all(...rootIds ?? []);
    const byId = /* @__PURE__ */ new Map();
    const lexicalIds = new Set(lexicalRows.map((row) => row.id));
    const candidates = /* @__PURE__ */ new Map();
    for (const row of [...lexicalRows, ...semanticRows]) candidates.set(row.id, row);
    for (const row of candidates.values()) {
      const semanticScore = Math.max(
        0,
        cosineSimilarity(queryEmbedding, decodeEmbedding(row.embeddingJson) ?? [])
      );
      const lexicalIndex = lexicalRows.findIndex((candidate) => candidate.id === row.id);
      const lexicalScore = lexicalIds.has(row.id) ? Math.max(0.15, 1 - lexicalIndex / Math.max(lexicalRows.length, 1)) : 0;
      const score = lexicalScore * 0.72 + semanticScore * 0.28;
      if (score <= 0.08) continue;
      byId.set(row.id, {
        chunkId: row.id,
        sourceId: row.sourceId,
        path: row.path,
        title: (0, import_node_path5.basename)(row.path),
        pageNumber: row.pageNumber ?? void 0,
        text: row.text,
        score,
        lexicalScore,
        semanticScore
      });
    }
    return Array.from(byId.values()).sort((left, right) => right.score - left.score).slice(0, limit);
  }
  readChunk(chunkId, maxChars = 12e3, rootIds) {
    this.ensureInitialized();
    if (!chunkId || rootIds?.length === 0) return null;
    const rootFilter = rootIds ? `AND r.id IN (${rootIds.map(() => "?").join(", ")})` : "";
    const selected = this.db.prepare(
      `
      SELECT c.rowid AS rowId, c.source_id AS sourceId, c.page_number AS pageNumber,
             s.path
      FROM knowledge_chunks c
      JOIN knowledge_sources s ON s.id = c.source_id
      JOIN knowledge_roots r ON r.id = s.root_id
      WHERE c.id = ? AND r.enabled = 1 ${rootFilter}
      LIMIT 1
    `
    ).get(chunkId, ...rootIds ?? []);
    if (!selected) return null;
    const nearby = this.db.prepare(
      `
      SELECT text FROM knowledge_chunks
      WHERE source_id = ? AND rowid BETWEEN ? AND ?
      ORDER BY rowid ASC
    `
    ).all(selected.sourceId, selected.rowId - 2, selected.rowId + 2);
    const text = nearby.map((row) => row.text).join("\n\n").slice(0, Math.max(500, maxChars));
    this.db.prepare(
      `
      UPDATE knowledge_pages SET last_accessed_at = ?
      WHERE source_id = ? AND page_number = ?
    `
    ).run(Date.now(), selected.sourceId, selected.pageNumber ?? 1);
    return {
      resultId: chunkId,
      sourceId: selected.sourceId,
      path: selected.path,
      title: (0, import_node_path5.basename)(selected.path),
      pageNumber: selected.pageNumber ?? void 0,
      text
    };
  }
};

// src/main/knowledge/workerHost.ts
var import_node_child_process3 = require("node:child_process");
var import_node_path6 = require("node:path");
var KnowledgeWorkerHost = class {
  constructor(onStatus, onExit) {
    this.onStatus = onStatus;
    this.onExit = onExit;
  }
  onStatus;
  onExit;
  child = null;
  expectedStops = /* @__PURE__ */ new WeakSet();
  isRunning() {
    return this.child !== null;
  }
  start() {
    if (this.child) return;
    const workerPath = (0, import_node_path6.join)(__dirname, "knowledge-worker.js");
    const child = (0, import_node_child_process3.spawn)(process.execPath, [workerPath], {
      env: { ...process.env, TEZBAR_KNOWLEDGE_WORKER: "1" },
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true
    });
    this.child = child;
    let stdoutBuffer = "";
    const consumeLine = (line) => {
      if (!line.trim()) return;
      try {
        const message = JSON.parse(line);
        if (message.type === "event" && message.channel === "knowledge:status" && message.payload) {
          this.onStatus(message.payload);
        }
      } catch {
        console.warn("[knowledge-worker] ignored malformed worker output");
      }
    };
    child.stdout?.setEncoding("utf8");
    child.stdout?.on("data", (chunk) => {
      stdoutBuffer += chunk;
      let newline = stdoutBuffer.indexOf("\n");
      while (newline >= 0) {
        consumeLine(stdoutBuffer.slice(0, newline));
        stdoutBuffer = stdoutBuffer.slice(newline + 1);
        newline = stdoutBuffer.indexOf("\n");
      }
    });
    let stderrBuffer = "";
    child.stderr?.setEncoding("utf8");
    child.stderr?.on("data", (chunk) => {
      stderrBuffer += chunk;
      let newline = stderrBuffer.indexOf("\n");
      while (newline >= 0) {
        const line = stderrBuffer.slice(0, newline).trim();
        stderrBuffer = stderrBuffer.slice(newline + 1);
        if (line) console.error(`[knowledge-worker] ${line}`);
        newline = stderrBuffer.indexOf("\n");
      }
    });
    child.once("error", (error) => {
      console.error("[knowledge-worker] failed to launch:", error);
    });
    child.once("close", (code, signal) => {
      if (stdoutBuffer.trim()) consumeLine(stdoutBuffer);
      if (stderrBuffer.trim()) console.error(`[knowledge-worker] ${stderrBuffer.trim()}`);
      const expected = this.expectedStops.has(child);
      if (this.child === child) this.child = null;
      this.onExit({ code, signal, expected });
    });
  }
  async stop() {
    const child = this.child;
    if (!child) return;
    this.expectedStops.add(child);
    await new Promise((resolve2) => {
      let settled = false;
      const finish = () => {
        if (settled) return;
        settled = true;
        clearTimeout(forceTimer);
        resolve2();
      };
      const forceTimer = setTimeout(() => {
        if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
      }, 3e3);
      forceTimer.unref();
      child.once("close", finish);
      if (!child.kill("SIGTERM")) {
        if (this.child === child) this.child = null;
        finish();
      }
    });
  }
  shutdown() {
    const child = this.child;
    if (!child) return;
    this.expectedStops.add(child);
    child.kill("SIGTERM");
  }
};

// src/main/knowledge/service.ts
var execFileAsync3 = (0, import_node_util3.promisify)(import_node_child_process4.execFile);
var MAX_SCANNED_FILES = 75e3;
var STATUS_EVENT_INTERVAL_MS = 100;
var STATUS_PERSIST_INTERVAL_MS = 1e3;
var BACKGROUND_FILE_DELAY_MS = 12;
var VECTOR_BACKFILL_BATCH_SIZE = 512;
var VECTOR_BACKFILL_DELAY_MS = 25;
var SKIP_NAMES = /* @__PURE__ */ new Set([
  ".git",
  ".svn",
  ".hg",
  "Library",
  "Applications",
  "System",
  "AppData",
  "Windows",
  "Program Files",
  "Program Files (x86)",
  "ProgramData",
  "$RECYCLE.BIN",
  "node_modules",
  "bower_components",
  "Pods",
  "DerivedData",
  ".next",
  ".nuxt",
  ".cache",
  ".idea",
  ".vscode",
  ".gradle",
  ".terraform",
  ".serverless",
  "build",
  "dist",
  "coverage",
  "out",
  "target",
  "__pycache__",
  ".venv",
  "venv"
]);
var SKIP_FILE_NAMES = /* @__PURE__ */ new Set([
  ".DS_Store",
  "Thumbs.db",
  "desktop.ini",
  "package-lock.json",
  "pnpm-lock.yaml",
  "yarn.lock",
  "bun.lock",
  "bun.lockb",
  "Cargo.lock",
  "composer.lock",
  "Gemfile.lock",
  "Pipfile.lock",
  "poetry.lock"
]);
var SENSITIVE_FILE_PATTERNS = [
  /^(?:id_(?:rsa|dsa|ecdsa|ed25519)|authorized_keys|known_hosts)$/i,
  /(?:^|[._-])(?:credential|credentials|secret|secrets|token|tokens)(?:[._-]|$)/i,
  /\.(?:pem|key|p12|pfx|keystore)$/i
];
var SKIP_DIRECTORY_SUFFIXES = [
  ".app",
  ".bundle",
  ".framework",
  ".photoslibrary",
  ".photolibrary",
  ".plugin",
  ".xcarchive"
];
var MAJOR_KNOWLEDGE_FOLDER_NAMES = ["Desktop", "Documents", "Downloads", "Pictures"];
function shouldSkipKnowledgeEntry(name, isDirectory) {
  if (name.startsWith(".") || SKIP_NAMES.has(name)) return true;
  const lower = name.toLowerCase();
  if (isDirectory) return SKIP_DIRECTORY_SUFFIXES.some((suffix) => lower.endsWith(suffix));
  if (SKIP_FILE_NAMES.has(name)) return true;
  if (SENSITIVE_FILE_PATTERNS.some((pattern) => pattern.test(name))) return true;
  return lower.endsWith(".min.js") || lower.endsWith(".min.css");
}
function discoverMajorKnowledgeFolders(home = (0, import_node_os2.homedir)()) {
  return MAJOR_KNOWLEDGE_FOLDER_NAMES.map((name) => (0, import_node_path7.join)(home, name)).filter((path) => {
    try {
      return (0, import_node_fs6.statSync)(path).isDirectory();
    } catch {
      return false;
    }
  });
}
function isKnowledgeCandidatePath(rootPath, path) {
  const relativePath = (0, import_node_path7.relative)(rootPath, path);
  if (!relativePath || relativePath === ".." || relativePath.startsWith(`..${import_node_path7.sep}`)) return false;
  const segments = relativePath.split(import_node_path7.sep).filter(Boolean);
  const fileName = segments.pop();
  if (!fileName || shouldSkipKnowledgeEntry(fileName, false) || !isIndexablePath(path)) return false;
  return segments.every((name) => !shouldSkipKnowledgeEntry(name, true));
}
function initialStatus() {
  return {
    state: "idle",
    backend: "local",
    progress: 0,
    queuedSources: 0,
    processedSources: 0,
    failedSources: 0,
    sourceCount: 0,
    chunkCount: 0,
    indexedPageCount: 0,
    totalPageCount: 0,
    partialSourceCount: 0,
    sourceBytes: 0
  };
}
function emitStatus(status) {
  process.stdout.write(
    `${JSON.stringify({ type: "event", channel: "knowledge:status", payload: status })}
`
  );
}
var KnowledgeService = class {
  store = getKnowledgeStore();
  backends = /* @__PURE__ */ new Map([
    ["local", new LocalIndexingBackend()],
    ["tezbar-cloud", new TezbarCloudIndexingBackend()]
  ]);
  status = initialStatus();
  settings = { ...DEFAULT_KNOWLEDGE_SETTINGS };
  controller = null;
  activePromise = null;
  initialized = false;
  watchers = /* @__PURE__ */ new Map();
  rescanTimer = null;
  startupTimer = null;
  vectorBackfillTimer = null;
  rescanRequested = false;
  manuallyPaused = false;
  interactiveUntil = 0;
  lastStatusEventAt = 0;
  lastStatusPersistAt = 0;
  mode;
  statusSink;
  workerHost;
  constructor(options = {}) {
    this.mode = options.mode ?? "inline";
    this.statusSink = options.statusSink ?? emitStatus;
    this.workerHost = this.mode === "coordinator" ? new KnowledgeWorkerHost(
      (status) => this.acceptWorkerStatus(status),
      (exit) => this.handleWorkerExit(exit)
    ) : null;
  }
  initialize() {
    if (this.initialized) return;
    this.store.ensureInitialized();
    this.settings = this.store.getSettings();
    this.store.saveSettings(this.settings);
    this.status = this.store.getPersistedStatus();
    this.manuallyPaused = this.status.state === "paused";
    const checkpoint = this.store.getIndexingCheckpoint();
    if (checkpoint && checkpoint.candidates.length > 0) {
      const processed = Math.min(checkpoint.processedSources, checkpoint.totalSources);
      this.status = {
        ...this.status,
        state: this.manuallyPaused ? "paused" : "indexing",
        jobId: checkpoint.jobId,
        progress: checkpoint.totalSources > 0 ? processed / checkpoint.totalSources : 1,
        queuedSources: checkpoint.candidates.length,
        processedSources: processed,
        failedSources: checkpoint.failedSources,
        detail: this.manuallyPaused ? `Indexing paused \xB7 ${checkpoint.candidates.length} files remaining` : `Ready to resume ${checkpoint.candidates.length} files`
      };
      this.store.saveStatus(this.status);
    } else if (checkpoint) {
      this.status = {
        ...this.status,
        state: "completed",
        progress: 1,
        queuedSources: 0,
        processedSources: checkpoint.processedSources,
        failedSources: checkpoint.failedSources,
        detail: `Indexed ${checkpoint.processedSources} files`,
        lastCompletedAt: this.status.lastCompletedAt ?? Date.now()
      };
      this.store.clearIndexingCheckpoint(checkpoint.jobId);
      this.store.saveStatus(this.status);
    } else if (this.status.state === "scanning" || this.status.state === "indexing") {
      this.status = { ...this.status, state: "idle", progress: 0, detail: void 0 };
      this.store.saveStatus(this.status);
    }
    this.refreshCounts();
    this.initialized = true;
    if (this.mode !== "worker") this.syncWatchers();
    if (this.mode !== "worker") this.scheduleVectorBackfill(1e3);
    if (this.mode !== "worker" && !this.manuallyPaused && this.activeRoots().length > 0) {
      this.startupTimer = setTimeout(() => {
        this.startupTimer = null;
        void this.startIndexing();
      }, 2500);
      this.startupTimer.unref();
    }
  }
  snapshot() {
    this.initialize();
    this.refreshCounts();
    return {
      roots: this.store.listRoots(),
      status: { ...this.status },
      localBackendAvailable: true,
      cloudBackendAvailable: false,
      settings: { ...this.settings },
      storageBytes: this.store.storageBytes()
    };
  }
  addRoot(path) {
    this.initialize();
    const normalized = (0, import_node_path7.resolve)(path.trim());
    if (!normalized || !(0, import_node_fs6.existsSync)(normalized) || !(0, import_node_fs6.statSync)(normalized).isDirectory()) {
      throw new Error("Choose an existing folder");
    }
    const current = this.store.listRoots();
    const duplicate = current.find((root) => root.path === normalized);
    if (duplicate) return this.snapshot();
    const now = Date.now();
    this.store.upsertRoot({
      id: (0, import_node_crypto4.randomUUID)(),
      path: normalized,
      depth: "inherit",
      processingBackend: "local",
      enabled: true,
      createdAt: now,
      updatedAt: now
    });
    this.syncWatchers();
    if (this.isIndexingActive()) this.rescanRequested = true;
    void this.startIndexing();
    return this.snapshot();
  }
  addMajorRoots() {
    this.initialize();
    const existingRoots = new Map(this.store.listRoots().map((root) => [root.path, root]));
    const now = Date.now();
    for (const path of discoverMajorKnowledgeFolders()) {
      const existing = existingRoots.get(path);
      if (existing) {
        if (!existing.enabled) this.store.upsertRoot({ ...existing, enabled: true, updatedAt: now });
        continue;
      }
      this.store.upsertRoot({
        id: (0, import_node_crypto4.randomUUID)(),
        path,
        depth: "inherit",
        processingBackend: "local",
        enabled: true,
        createdAt: now,
        updatedAt: now
      });
    }
    this.syncWatchers();
    if (this.isIndexingActive()) this.rescanRequested = true;
    void this.startIndexing();
    return this.snapshot();
  }
  async removeRoot(rootId) {
    this.initialize();
    await this.stopCoordinatorWorker();
    this.store.removeRoot(rootId);
    this.store.clearIndexingCheckpoint();
    this.syncWatchers();
    this.refreshCounts();
    this.persistStatus();
    if (!this.manuallyPaused && this.activeRoots().length > 0) void this.startIndexing();
    return this.snapshot();
  }
  async setRootEnabled(rootId, enabled) {
    this.initialize();
    const root = this.store.listRoots().find((candidate) => candidate.id === rootId);
    if (!root) throw new Error("Knowledge folder was not found");
    await this.stopCoordinatorWorker();
    this.store.upsertRoot({ ...root, enabled, updatedAt: Date.now() });
    this.store.clearIndexingCheckpoint();
    this.syncWatchers();
    if (!this.manuallyPaused && this.activeRoots().length > 0) void this.startIndexing();
    return this.snapshot();
  }
  async setDepth(depth) {
    this.initialize();
    if (!["off", "basic", "smart", "deep"].includes(depth)) {
      throw new Error("Invalid Knowledge Depth");
    }
    if (this.settings.depth === depth) return this.snapshot();
    this.settings = { ...this.settings, depth };
    this.store.saveSettings(this.settings);
    return this.restartForConfigurationChange(`Knowledge Depth changed to ${depth}`);
  }
  async setRootDepth(rootId, depth) {
    this.initialize();
    if (!["inherit", "off", "basic", "smart", "deep"].includes(depth)) {
      throw new Error("Invalid folder Knowledge Depth");
    }
    const root = this.store.listRoots().find((candidate) => candidate.id === rootId);
    if (!root) throw new Error("Knowledge folder was not found");
    if (root.depth === depth) return this.snapshot();
    this.store.upsertRoot({ ...root, depth, updatedAt: Date.now() });
    this.syncWatchers();
    return this.restartForConfigurationChange("Folder Knowledge Depth changed");
  }
  search(query, limit = 12) {
    this.initialize();
    return this.store.search(
      query,
      limit,
      this.activeRoots().map((root) => root.id)
    );
  }
  searchMetadata(query, limit = 20) {
    this.initialize();
    return this.store.searchMetadata(
      query,
      limit,
      this.activeRoots().map((root) => root.id)
    );
  }
  read(resultId, maxChars = 12e3) {
    this.initialize();
    return this.store.readChunk(
      resultId,
      maxChars,
      this.activeRoots().map((root) => root.id)
    );
  }
  listSources(input) {
    this.initialize();
    return this.store.listSources(input);
  }
  notifyInteractiveActivity(durationMs = 1e3) {
    this.interactiveUntil = Math.max(this.interactiveUntil, Date.now() + durationMs);
  }
  async startIndexing() {
    this.initialize();
    if (this.startupTimer) clearTimeout(this.startupTimer);
    this.startupTimer = null;
    if (this.manuallyPaused) return this.snapshot();
    if (this.isIndexingActive()) return this.snapshot();
    const roots = this.activeRoots();
    if (roots.length === 0) return this.snapshot();
    const checkpoint = this.store.getIndexingCheckpoint();
    const jobId = checkpoint?.jobId ?? (0, import_node_crypto4.randomUUID)();
    if (checkpoint) {
      const processed = Math.min(checkpoint.processedSources, checkpoint.totalSources);
      this.updateStatus({
        ...this.status,
        state: "indexing",
        jobId,
        progress: checkpoint.totalSources > 0 ? processed / checkpoint.totalSources : 1,
        queuedSources: checkpoint.candidates.length,
        processedSources: processed,
        failedSources: checkpoint.failedSources,
        error: void 0,
        detail: `Resuming ${checkpoint.candidates.length} files`
      });
    } else {
      this.updateStatus({
        ...initialStatus(),
        state: "scanning",
        backend: "local",
        jobId,
        detail: "Scanning knowledge folders"
      });
    }
    if (this.mode === "coordinator") {
      if (checkpoint) this.rescanRequested = true;
      this.workerHost?.start();
      return this.snapshot();
    }
    this.controller = new AbortController();
    const run = checkpoint ? this.resumeIndexing(checkpoint, roots, this.controller.signal) : this.runIndexing(jobId, roots, this.controller.signal);
    this.activePromise = run.finally(() => {
      this.activePromise = null;
      this.controller = null;
      if (this.rescanRequested && !this.manuallyPaused) {
        this.rescanRequested = false;
        this.scheduleRescan();
      }
    });
    void this.activePromise;
    return this.snapshot();
  }
  async pause() {
    this.manuallyPaused = true;
    this.rescanRequested = false;
    if (this.rescanTimer) clearTimeout(this.rescanTimer);
    this.rescanTimer = null;
    if (this.mode === "coordinator") {
      await this.workerHost?.stop();
      this.updateStatus({ ...this.status, state: "paused", detail: "Indexing paused" });
      return this.snapshot();
    }
    this.controller?.abort();
    const jobId = this.status.jobId;
    if (jobId) await this.backends.get("local")?.cancel(jobId);
    this.updateStatus({ ...this.status, state: "paused", detail: "Indexing paused" });
    return this.snapshot();
  }
  async resume() {
    this.manuallyPaused = false;
    this.rescanRequested = false;
    if (this.mode === "coordinator") await this.workerHost?.stop();
    else if (this.activePromise) await this.activePromise.catch(() => {
    });
    return this.startIndexing();
  }
  async waitForCurrentRun() {
    const active = this.activePromise;
    if (active) await active.catch(() => {
    });
  }
  async runIndexing(jobId, roots, signal) {
    try {
      const candidates = [];
      for (const root of roots) {
        const scan = await this.scanRoot(root, signal);
        if (scan.complete) this.store.removeMissingSources(root.id, new Set(scan.paths));
        candidates.push(
          ...scan.paths.filter((path) => this.needsIndexing(root, path)).map((path) => ({ root, path }))
        );
      }
      if (signal.aborted) return;
      this.updateStatus({
        ...this.status,
        state: "indexing",
        queuedSources: candidates.length,
        progress: candidates.length === 0 ? 1 : 0,
        detail: candidates.length === 0 ? "No supported files found" : `Preparing ${candidates.length} files`
      });
      const checkpoint = this.store.saveIndexingCheckpoint(
        jobId,
        candidates.map((candidate) => ({ rootId: candidate.root.id, path: candidate.path }))
      );
      await this.processCandidateQueue(
        checkpoint,
        candidates.map((candidate, position) => ({ ...candidate, position })),
        signal
      );
    } catch (error) {
      if (signal.aborted) return;
      this.updateStatus({
        ...this.status,
        state: "failed",
        error: error instanceof Error ? error.message : String(error),
        detail: "Indexing stopped"
      });
    }
  }
  async resumeIndexing(checkpoint, roots, signal) {
    try {
      const rootsById = new Map(roots.map((root) => [root.id, root]));
      const candidates = [];
      let processed = checkpoint.processedSources;
      for (const persisted of checkpoint.candidates) {
        const root = rootsById.get(persisted.rootId);
        if (!root || !isKnowledgeCandidatePath(root.path, persisted.path) || !this.needsIndexing(root, persisted.path)) {
          if (this.store.completeIndexingCandidate(checkpoint.jobId, persisted.position, false)) {
            processed += 1;
          }
          continue;
        }
        candidates.push({ root, path: persisted.path, position: persisted.position });
      }
      await this.processCandidateQueue(
        { ...checkpoint, processedSources: processed },
        candidates,
        signal
      );
    } catch (error) {
      if (signal.aborted) return;
      this.updateStatus({
        ...this.status,
        state: "failed",
        error: error instanceof Error ? error.message : String(error),
        detail: "Indexing stopped"
      });
    }
  }
  async processCandidateQueue(checkpoint, candidates, signal) {
    let cursor = 0;
    let completed = checkpoint.processedSources;
    let failed = checkpoint.failedSources;
    const total = checkpoint.totalSources;
    this.updateStatus({
      ...this.status,
      state: "indexing",
      jobId: checkpoint.jobId,
      progress: total > 0 ? Math.min(1, completed / total) : 1,
      queuedSources: Math.max(0, total - completed),
      processedSources: completed,
      failedSources: failed,
      detail: candidates.length === 0 ? "Finishing index" : `Indexing ${candidates.length} files`
    });
    const workers = Array.from(
      { length: Math.min(this.settings.maxConcurrentExtractors, candidates.length) },
      async () => {
        while (!signal.aborted) {
          const index = cursor;
          cursor += 1;
          const candidate = candidates[index];
          if (!candidate) return;
          const succeeded = await this.indexCandidate(
            checkpoint.jobId,
            candidate,
            total,
            () => completed,
            signal
          );
          if (signal.aborted) return;
          if (!this.store.completeIndexingCandidate(checkpoint.jobId, candidate.position, !succeeded)) {
            continue;
          }
          completed += 1;
          if (!succeeded) failed += 1;
          this.updateStatus({ ...this.status, processedSources: completed, failedSources: failed });
          this.updateProgress(completed, total, `Processed ${(0, import_node_path7.basename)(candidate.path)}`);
          if (this.mode === "worker") {
            await new Promise((resolve2) => setTimeout(resolve2, BACKGROUND_FILE_DELAY_MS));
          }
        }
      }
    );
    await Promise.all(workers);
    if (signal.aborted) return;
    this.updateStatus({
      ...this.status,
      state: "completed",
      progress: 1,
      queuedSources: 0,
      processedSources: completed,
      failedSources: failed,
      detail: `Indexed ${completed} files`,
      lastCompletedAt: Date.now()
    });
    this.store.clearIndexingCheckpoint(checkpoint.jobId);
  }
  async scanRoot(root, signal) {
    const queue = [root.path];
    const paths = [];
    let visited = 0;
    let complete = true;
    while (queue.length > 0 && visited < MAX_SCANNED_FILES && !signal.aborted) {
      const directory = queue.shift();
      if (!directory) break;
      let entries;
      try {
        entries = (0, import_node_fs6.readdirSync)(directory, { withFileTypes: true });
      } catch {
        complete = false;
        continue;
      }
      for (const entry of entries) {
        if (signal.aborted) break;
        visited += 1;
        if (shouldSkipKnowledgeEntry(entry.name, entry.isDirectory())) continue;
        const path = (0, import_node_path7.join)(directory, entry.name);
        if (entry.isSymbolicLink()) continue;
        if (entry.isDirectory()) {
          queue.push(path);
        } else if (entry.isFile() && isIndexablePath(path)) {
          try {
            const fileStat = (0, import_node_fs6.statSync)(path);
            const isExtensionlessExecutable = !(0, import_node_path7.extname)(path) && (fileStat.mode & 73) !== 0;
            if (!isExtensionlessExecutable && fileStat.size <= maximumIndexableSourceBytes(path)) {
              paths.push(path);
            }
          } catch {
          }
        }
        if (visited % 300 === 0) await new Promise((resolve2) => setImmediate(resolve2));
      }
    }
    if (visited >= MAX_SCANNED_FILES || signal.aborted) complete = false;
    return { paths, complete };
  }
  async indexCandidate(jobId, candidate, total, completed, signal) {
    const depth = effectiveKnowledgeDepth(candidate.root, this.settings);
    const profile = profileForDepth(depth);
    if (!profile) return true;
    const profileKey = indexingProfileKey(profile);
    const sourceId = sourceIdForPath(candidate.path);
    try {
      const existing = this.store.getSourceByPath(candidate.path);
      const stat = (0, import_node_fs6.statSync)(candidate.path);
      if (existing?.status === "indexed" && existing.byteSize === stat.size && Math.round(existing.modifiedAt) === Math.round(stat.mtimeMs) && existing.indexingProfile === profileKey) {
        return true;
      }
      const fingerprint = await fingerprintSource(candidate.path, signal);
      if (signal.aborted) return false;
      this.store.markSourcePending({
        id: sourceId,
        rootId: candidate.root.id,
        path: candidate.path,
        fingerprint,
        mediaType: (0, import_node_path7.extname)(candidate.path).slice(1).toLowerCase(),
        indexingProfile: profileKey
      });
      const reusable = this.store.findReusableResult(fingerprint.contentHash, sourceId, profileKey);
      if (reusable) {
        this.store.saveResult(candidate.root.id, candidate.path, fingerprint, profileKey, reusable);
        return true;
      }
      const request = {
        jobId,
        rootId: candidate.root.id,
        sourceId,
        path: candidate.path,
        fingerprint,
        depth: profile.depth,
        requestedCapabilities: profile.requestedCapabilities,
        maxPagesPerDocument: profile.maxPagesPerDocument,
        maxOcrPagesPerDocument: profile.maxOcrPagesPerDocument,
        ocrEveryPage: profile.ocrEveryPage
      };
      const backend = this.backends.get(
        candidate.root.processingBackend === "cloud" ? "tezbar-cloud" : "local"
      );
      if (!backend) throw new Error("Selected indexing backend is unavailable");
      const result = await backend.index(request, {
        signal,
        yieldToInteractiveWork: () => this.waitForInteractiveIdle(signal),
        onProgress: (fileProgress, detail) => {
          const overall = total > 0 ? (completed() + fileProgress) / total : 1;
          this.updateStatus({
            ...this.status,
            progress: Math.max(this.status.progress, overall),
            detail: detail ? `${detail} \xB7 ${(0, import_node_path7.basename)(candidate.path)}` : (0, import_node_path7.basename)(candidate.path)
          });
        }
      });
      await this.waitForInteractiveIdle(signal);
      if (signal.aborted) return false;
      this.store.saveResult(candidate.root.id, candidate.path, fingerprint, profileKey, result);
      return true;
    } catch (error) {
      if (signal.aborted) return false;
      this.store.markSourceFailed(sourceId, error instanceof Error ? error.message : String(error));
      return false;
    }
  }
  needsIndexing(root, path) {
    const profile = profileForDepth(effectiveKnowledgeDepth(root, this.settings));
    if (!profile) return false;
    try {
      const existing = this.store.getSourceByPath(path);
      const fileStat = (0, import_node_fs6.statSync)(path);
      return !(existing?.status === "indexed" && existing.byteSize === fileStat.size && Math.round(existing.modifiedAt) === Math.round(fileStat.mtimeMs) && existing.indexingProfile === indexingProfileKey(profile));
    } catch {
      return false;
    }
  }
  activeRoots() {
    return this.store.listRoots().filter((root) => root.enabled && effectiveKnowledgeDepth(root, this.settings) !== "off");
  }
  async restartForConfigurationChange(detail) {
    this.rescanRequested = false;
    if (this.rescanTimer) clearTimeout(this.rescanTimer);
    this.rescanTimer = null;
    if (this.mode === "coordinator") {
      await this.workerHost?.stop();
    } else {
      const active = this.activePromise;
      this.controller?.abort();
      const jobId = this.status.jobId;
      if (jobId) await this.backends.get("local")?.cancel(jobId);
      if (active) await active.catch(() => {
      });
    }
    this.store.clearIndexingCheckpoint();
    if (this.manuallyPaused) {
      this.updateStatus({ ...this.status, state: "paused", detail: "Indexing paused" });
      return this.snapshot();
    }
    if (this.activeRoots().length === 0) {
      this.updateStatus({
        ...initialStatus(),
        state: "idle",
        backend: "local",
        progress: 1,
        detail: `${detail} \xB7 No active content-indexing folders`
      });
      return this.snapshot();
    }
    return this.startIndexing();
  }
  updateProgress(done, total, detail) {
    this.updateStatus({
      ...this.status,
      progress: total > 0 ? Math.max(this.status.progress, Math.min(1, done / total)) : 1,
      queuedSources: Math.max(0, total - done),
      detail
    });
  }
  refreshCounts() {
    const counts = this.store.counts();
    this.status = { ...this.status, ...counts };
  }
  updateStatus(status) {
    const stateChanged = status.state !== this.status.state;
    this.status = status;
    this.refreshCounts();
    const now = Date.now();
    if (stateChanged || now - this.lastStatusPersistAt >= STATUS_PERSIST_INTERVAL_MS) {
      this.persistStatus();
      this.lastStatusPersistAt = now;
    }
    if (stateChanged || now - this.lastStatusEventAt >= STATUS_EVENT_INTERVAL_MS) {
      this.statusSink(this.status);
      this.lastStatusEventAt = now;
    }
  }
  persistStatus() {
    this.store.saveStatus(this.status);
  }
  async waitForInteractiveIdle(signal) {
    while (!signal.aborted && Date.now() < this.interactiveUntil) {
      const remaining = this.interactiveUntil - Date.now();
      await new Promise((resolve2) => setTimeout(resolve2, Math.min(100, remaining)));
    }
  }
  shutdown() {
    this.workerHost?.shutdown();
    this.controller?.abort();
    if (this.startupTimer) clearTimeout(this.startupTimer);
    this.startupTimer = null;
    if (this.rescanTimer) clearTimeout(this.rescanTimer);
    this.rescanTimer = null;
    if (this.vectorBackfillTimer) clearTimeout(this.vectorBackfillTimer);
    this.vectorBackfillTimer = null;
    for (const watcher of this.watchers.values()) watcher.close();
    this.watchers.clear();
    if (this.initialized) this.persistStatus();
  }
  scheduleVectorBackfill(delayMs = VECTOR_BACKFILL_DELAY_MS) {
    if (this.mode === "worker" || this.vectorBackfillTimer) return;
    this.vectorBackfillTimer = setTimeout(() => {
      this.vectorBackfillTimer = null;
      if (Date.now() < this.interactiveUntil) {
        this.scheduleVectorBackfill(250);
        return;
      }
      try {
        const batch = this.store.backfillVectorIndexBatch(VECTOR_BACKFILL_BATCH_SIZE);
        if (batch.hasMore) this.scheduleVectorBackfill();
      } catch (error) {
        console.warn("[Knowledge] Vector backfill paused after an error:", error);
        this.scheduleVectorBackfill(2e3);
      }
    }, delayMs);
    this.vectorBackfillTimer.unref();
  }
  scheduleRescan() {
    if (this.manuallyPaused) {
      this.rescanRequested = true;
      return;
    }
    if (this.isIndexingActive()) {
      this.rescanRequested = true;
      return;
    }
    if (this.rescanTimer) clearTimeout(this.rescanTimer);
    this.rescanTimer = setTimeout(() => {
      this.rescanTimer = null;
      void this.startIndexing();
    }, 1500);
    this.rescanTimer.unref();
  }
  syncWatchers() {
    if (this.mode === "worker") return;
    const enabledRoots = new Map(this.activeRoots().map((root) => [root.id, root]));
    for (const [rootId, watcher] of this.watchers) {
      if (enabledRoots.has(rootId)) continue;
      watcher.close();
      this.watchers.delete(rootId);
    }
    for (const root of enabledRoots.values()) {
      if (this.watchers.has(root.id)) continue;
      try {
        const watcher = (0, import_node_fs6.watch)(root.path, { recursive: true }, () => this.scheduleRescan());
        watcher.on("error", () => {
          watcher.close();
          this.watchers.delete(root.id);
        });
        this.watchers.set(root.id, watcher);
      } catch {
      }
    }
  }
  isIndexingActive() {
    return this.activePromise !== null || Boolean(this.workerHost?.isRunning());
  }
  async stopCoordinatorWorker() {
    if (this.mode === "coordinator") await this.workerHost?.stop();
  }
  acceptWorkerStatus(status) {
    this.status = { ...status };
    this.statusSink(this.status);
  }
  handleWorkerExit(exit) {
    if (!this.initialized || this.mode !== "coordinator") return;
    if (exit.expected) return;
    const persisted = this.store.getPersistedStatus();
    this.status = persisted;
    if (this.status.state === "scanning" || this.status.state === "indexing") {
      this.updateStatus({
        ...this.status,
        state: "failed",
        detail: "Indexing worker stopped",
        error: `Indexing worker exited with ${exit.signal ?? exit.code ?? "an unknown error"}`
      });
    } else {
      this.statusSink(this.status);
    }
    if (this.rescanRequested && !this.manuallyPaused) {
      this.rescanRequested = false;
      this.scheduleRescan();
    }
  }
};

// src/main/knowledge/worker.ts
try {
  (0, import_node_os3.setPriority)(0, 10);
} catch {
}
function emitStatus2(status) {
  process.stdout.write(
    `${JSON.stringify({ type: "event", channel: "knowledge:status", payload: status })}
`
  );
}
var service = new KnowledgeService({ mode: "worker", statusSink: emitStatus2 });
var stopping = false;
function shutdown() {
  if (stopping) return;
  stopping = true;
  service.shutdown();
  process.exit(0);
}
process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
void (async () => {
  service.initialize();
  await service.startIndexing();
  await service.waitForCurrentRun();
  service.shutdown();
})().catch((error) => {
  console.error(error instanceof Error ? error.stack ?? error.message : String(error));
  process.exitCode = 1;
});
//# sourceMappingURL=knowledge-worker.js.map
