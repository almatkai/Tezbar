# v0.2.0-beta.2 — Windows Preview and Performance

This beta introduces the first Windows installers for Tezbar and significantly reduces idle resource use. It also improves startup feedback and automatically recovers when the backend crashes or stops responding.

## Highlights

- Added the first Windows x64 installer as a per-user NSIS setup executable (`.exe`).
- Reduced measured idle private RAM from 635.9 MiB to 502.8 MiB (20.9%), with additional lazy loading bringing the packaged app process tree to about 406.8 MiB in the final profile.
- Removed idle voice-input services and deferred Knowledge, Settings, extensions, terminals, agents, and AI providers until they are needed.
- Added clear startup and reconnection status, Retry controls, backend health checks, and recovery from crashes or hangs.
- Fixed macOS launcher placement: the first open is centered on the active display, and later opens restore the last valid position.
- Improved Windows launcher placement, snapping, mixed-DPI handling, terminal startup paths, drive-path completions, and application discovery.
- Added TokenRouter integration and improved OpenCode, DeepSeek, model selection, and chat working-directory support.
- Expanded the cross-platform extension catalog and synchronized application and tray icons.

## Reliability and performance

- Removed continuous Windows PowerShell clipboard polling.
- Bounded search, icon, and string caches to prevent unbounded memory growth.
- Streamed large file-index refreshes in batches instead of retaining the full document set in memory.
- Kept the Knowledge worker stopped during ordinary idle use and launcher searches; it now starts only for Knowledge features and deep search.
- Improved backend and worker shutdown cleanup.

## Installation

### Windows

- Use the x64 setup executable for a normal per-user installation.
- Windows may show a SmartScreen warning because this beta is not code-signed with a trusted commercial certificate.

### macOS

- Apple silicon: download the `aarch64.dmg` asset.

## Notes

- This is beta software. Please report startup, installer, window-placement, and recovery issues with Windows version and display-scaling details.
- Voice input has been removed from this build. Read-aloud remains available on demand.
