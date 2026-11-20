# Lorem Ipsum structured document commands

Added `markdown` and `html` to the existing standalone Rust port, bringing its command inventory from four to six. Native development version is 0.1.1; the previous 0.1.0 artifacts are retained. Existing commands, preferences, grants, IDs and SDK revision remain intact.

Source: Raycast extensions commit `5bcd41ff46cf8ad355e98f11bb7e716c6fdafc4b`, `extensions/lorem-ipsum/src/{utils,html,markdown}.tsx`. Exact fetched source and package metadata are retained beside this report. The original MIT author/contributor attribution remains in the extension NOTICE.

Both new commands generate 3–8 document blocks, begin with a level-2/3 heading, and choose paragraphs, ordered/unordered lists and quotations with the source weights. Paragraphs optionally emphasize one middle word. Markdown and HTML formatting follows the source, including blank-line separators, list indentation and blockquote structure. Output goes through the existing clipboard/paste/paste-and-copy handler and its 1 MiB limit. No AI/network service is used for these commands.

The port retains its existing lorem-ipsum 2.0.10-derived word engine; the newer upstream package depends on 3.0.0. Random output is not asserted byte-for-byte equal to upstream. Existing output ordering deliberately completes clipboard writes before hiding the app; this behavior remains unchanged.

Tests cover exact rendering of every block/emphasis form, randomized heading and block bounds, and both new commands over actual executable JSONL pipes for all three output preferences. Recording host effects prevent real clipboard, focus, AI or browser interactions.

Build, package and importer evidence will be appended after the corresponding checks complete. macOS build and live desktop acceptance remain pending; no release publication is claimed.
