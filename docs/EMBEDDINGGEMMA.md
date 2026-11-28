# Optional local semantic search

In Settings → Knowledge → Semantic search, download EmbeddingGemma 2, then
select it as the search engine. Built-in search remains the default. Nothing
is downloaded until the user requests it.

This first integration supports text and code, including text extracted from
PDFs and OCR. It does not embed raw images, audio, or video. Smart and Deep
folders receive learned embeddings; Basic folders remain keyword-only.

The text-only Q8 model is 309,855,456 bytes. The platform-specific native runtime
adds roughly 12–20 MB. Actual runtime memory includes model mappings, context
buffers, and inference allocations; the download size is not a RAM estimate.
The runtime loads on demand and shuts down after two idle minutes. A pipe-owned
native supervisor also stops it when the Rust host exits, including abrupt
termination. This private mode of the Tezbar executable does not initialize
Tauri or start a JS runtime. Unix process groups and Windows Job Objects own
the inference process tree.

## Installation and inference

- Model: `ggml-org/embeddinggemma-2-GGUF`, pinned revision
  `bfcd298762cc34d0357ece5ebdd31791a3a374d8`, `embeddinggemma-2-Q8_0.gguf`.
- Runtime: llama.cpp `b11474`, a build after Gemma 2 support was merged.
- Every artifact has a pinned size and SHA-256 checksum. Incomplete files are
  never treated as installed. Cancel removes incomplete installation files.
- Supported downloads: Windows x64/ARM64, macOS x64/ARM64, Linux x64.
- Runtime and weights live under the app data directory in
  `knowledge/models/embeddinggemma-2`. Inference uses an authenticated loopback
  endpoint. Document contents are never sent to a remote inference provider.
- No Python installation, cloud credentials, or separate Ollama installation
  is required. Downloading requires internet; inference then works offline.

## Retrieval and lifecycle

Gemma inference runs outside the backend process. Background embedding uses
small batches and yields while the launcher is busy. Query vectors are cached.
Queries use `task: search result | query: …`; chunks use
`title: <filename> | text: …`.

The model's projected 768-dimensional output is truncated to 256 dimensions
and L2-normalized. SQLite stores float32 vectors in a separate index; it never
compares them to the existing 384-dimensional hashed vectors. The index key
includes model revision, quantization, dimensions, and preprocessing version.
Keyword and semantic rankings are combined using reciprocal rank fusion.

Enabling the model embeds existing extracted chunks without repeating OCR.
Progress survives restart through persisted per-chunk embeddings. Updated or
removed chunks invalidate their learned vectors. Pausing Knowledge indexing
also pauses semantic indexing. Removing the model selects built-in search and
clears learned vectors, while preserving extracted text and keyword search.
If inference fails, keyword search remains available and Settings reports the
error. “Retry semantic indexing” restarts unfinished work.

## Validation

Run `cargo test --manifest-path src-tauri/Cargo.toml --lib native_core::knowledge`
for the current native extraction, database migration, inference-protocol and
process-lifetime fixtures. They use temporary profiles and fake loopback model
responses; they do not download a model.

The retained TypeScript tests (`pnpm exec vitest run src/main/knowledge`) describe
the pre-port implementation. Their real-model integration tests are explicitly
opt-in and download about
330 MB into the specified isolated directory:

```powershell
$env:TEZBAR_GEMMA_INTEGRATION = '1'
$env:APPDATA_DIR = 'C:/path/to/isolated/gemma-test-data'
pnpm exec vitest run src/main/knowledge/embeddings/gemmaRuntime.integration.test.ts
```

These legacy tests check English paraphrases, Russian-to-English retrieval,
local restart, and the old extraction → background embedding → search path.
The pre-port runtime was exercised on Windows x64. That is not validation of
the new Rust integration: a real-model smoke on each release platform remains
necessary. Cross-platform downloads remain pinned.

## Upstream references

- [Google model card and prompt/truncation guidance](https://huggingface.co/google/embeddinggemma-2)
- [GGUF model files](https://huggingface.co/ggml-org/embeddinggemma-2-GGUF)
- [llama.cpp support](https://github.com/ggml-org/llama.cpp/pull/30054)

EmbeddingGemma 2 is Apache-2.0; llama.cpp is MIT. Runtime distributions retain
their upstream license files.
