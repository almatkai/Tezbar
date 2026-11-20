# Popular Raycast extensions: Rust port inventory

Observation: 2026-11-20. Read-only public [Most Popular](https://www.raycast.com/store/popular) pages 1?5 were fetched. They contain **50 ranked listings**, of which **33 have no native catalog mapping**. The native catalog has **105 records**. A mapping is metadata only, and does not prove implementation, command parity, desktop acceptance or publication.

All **362** observed popular pages have now been visited. They exposed **3,618 listing slots** but only **3,377 distinct extension IDs**: 241 slots repeat identities, especially when download counts tie. These are observations, not a claim that every Store identity was recovered. `completePageEnumeration` is true; `completeStoreEnumeration` remains false. Duplicate evidence is in `pagination-duplicates.json`. Of the observed unique entries, 3,279 have no native mapping in the 105-record baseline catalog. Store counts and ordering change between responses.

## Highest-priority unported entries

Rank below is the observed global popularity order; counts are the public download_count at fetch time. Source command counts/preferences come from each Store-linked immutable Git commit package.json, not from marketing text. Full original command/preference objects are retained in [the source-pinned inventory](popular-unported-top20.json).

| Rank | Extension / source directory | Downloads | Source commands | Platforms |
| --- | --- | ---: | ---: | --- |
| 3 | [Google Chrome](https://www.raycast.com/Codely/google-chrome) / `google-chrome` | 598,460 | 11 | macOS |
| 7 | [Linear](https://www.raycast.com/linear/linear) / `linear` | 421,138 | 14 | macOS, Windows |
| 8 | [Slack](https://www.raycast.com/mommertf/slack) / `slack` | 353,068 | 9 | macOS, Windows |
| 9 | [Brew](https://www.raycast.com/nhojb/brew) / `brew` | 311,497 | 12 | macOS |
| 10 | [Notion](https://www.raycast.com/notion/notion) / `notion` | 294,480 | 6 | Windows, macOS |
| 11 | [ChatGPT](https://www.raycast.com/abielzulio/chatgpt) / `chatgpt` | 271,766 | 9 | macOS, Windows |
| 12 | [1Password](https://www.raycast.com/khasbilegt/1password) / `1password` | 266,206 | 4 | macOS, Windows |
| 13 | [Arc](https://www.raycast.com/the-browser-company/arc) / `arc` | 262,252 | 16 | macOS |
| 14 | [GitHub](https://www.raycast.com/raycast/github) / `github` | 222,064 | 25 | macOS, Windows |
| 16 | [Obsidian](https://www.raycast.com/marcjulian/obsidian) / `obsidian` | 186,228 | 13 | macOS, Windows |
| 17 | [Coffee](https://www.raycast.com/mooxl/coffee) / `coffee` | 164,188 | 9 | macOS, Windows |
| 18 | [Apple Notes](https://www.raycast.com/raycast/apple-notes) / `apple-notes` | 162,917 | 7 | macOS |
| 20 | [Video Downloader](https://www.raycast.com/vimtor/video-downloader) / `video-downloader` | 143,311 | 1 | macOS, Windows |
| 22 | [Apple Reminders](https://www.raycast.com/raycast/apple-reminders) / `apple-reminders` | 128,688 | 10 | macOS |
| 23 | [System Monitor](https://www.raycast.com/hossammourad/raycast-system-monitor) / `system-monitor` | 127,898 | 2 | macOS |
| 25 | [Warp](https://www.raycast.com/warpdotdev/warp) / `warp` | 122,432 | 5 | macOS, Windows |
| 26 | [Zoom](https://www.raycast.com/raycast/zoom) / `zoom` | 121,616 | 5 | macOS, Windows |
| 27 | [Pomodoro](https://www.raycast.com/asubbotin/pomodoro) / `pomodoro` | 118,321 | 5 | macOS |
| 29 | [YouTube](https://www.raycast.com/tonka3000/youtube) / `youtube` | 115,913 | 4 | macOS, Windows |
| 32 | [Music](https://www.raycast.com/fedevitaledev/music) / `music` | 105,782 | 30 | macOS |

## Feasibility and required host contracts

Generic host contracts already expose clipboard, selection, shell/open, apps, dialogs, AI, tray, notifications and durable background ownership. Source-specific OAuth providers and arbitrary Raycast IPC do not automatically exist in this SDK. Prefer standalone HTTP/file/process domain implementations, retaining exact source command IDs and preferences.

- **Google Chrome:** macOS AppleScript tab/window control plus Chromium SQLite profile data. Existing host shell execution and Rust SQLite can cover core behavior; platform acceptance requires a Mac.
- **Linear:** Linear GraphQL, issue/project/cycle/inbox state and forms. Source uses Linear SDK/OAuth; Rust needs actual OAuth/token ownership and paginated requests. Never claim acceptance without account-backed tests.
- **Slack:** Slack Web API with an existing accessToken preference, optional proxy, channels/messages/status. Implement token-prefixed HTTP and retained forms; live send/update checks require explicit synthetic/test account.
- **Brew:** Homebrew CLI JSON and process/terminal execution. Source has customBrewPath and terminalApp preferences. Real package mutations stay out of verification; macOS-only live acceptance remains required.
- **Notion:** Notion API token preference, page/database search, capture and rich block conversion. Auth token path is present upstream; full markdown/HTML/database property parity is substantial.
- **ChatGPT:** OpenAI/Azure chat, streaming/history/vision/TTS and proxies with 21 original preferences. Existing app AI host service may help, but API-specific compatibility and configuration require dedicated review.
- **1Password:** 1Password SDK/CLI, secrets, OTP and item actions. Preserve native credential boundaries and never log vault output; requires installed CLI/application and dedicated user-authorized live fixtures.
- **Arc:** macOS browser tabs/windows plus persisted Arc data. Needs browser-specific AppleScript and local profile parsing; do not relabel Chrome compatibility as full Arc parity.
- **GitHub:** GitHub REST/GraphQL, token/OAuth, searches and repository clone workflows. 25 commands plus tools; token preference exists. Repository mutations need synthetic/local targets.
- **Obsidian:** Cross-platform vault files, search/indexing/YAML, daily notes and obsidian:// URLs. Existing vaultPath preference and generic shell/open permit standalone Rust implementation; test only disposable vaults.
- **Coffee:** 9 keep-awake, schedule and menu commands. Can reuse generic background/tray contracts but needs real durable schedule semantics and OS-specific caffeinate behavior.
- **Apple Notes:** macOS Notes automation/read-only database search plus note mutations. Requires AppleScript/native database handling and Mac acceptance; use disposable notes only.
- **Video Downloader:** yt-dlp/FFmpeg external processes with 14 preferences and browser/clipboard inputs. Executable discovery, cancellation and output streaming required; no broad cookie/profile reads during tests.
- **Apple Reminders:** macOS Reminders automation with dates, lists and completion. EventKit/AppleScript permission and recurrence/date parity require Mac acceptance.
- **System Monitor:** System Monitor: source directory system-monitor. Rust sysinfo plus GPUI list/tray; source declares OS-specific CPU/memory/disk/battery behavior.
- **Warp:** 5 terminal/session commands, Warp/Warp Preview selection and YAML launch configurations. Good bounded local-file/URL/CLI candidate; macOS declaration preserved.
- **Zoom:** 5 meeting/control commands, app automation and meeting parsing. Generic shell/open plus platform-specific integration; live meeting/account behavior needs manual acceptance.
- **Pomodoro:** 5 commands, intervals/history/tray/background lifecycle, sounds and optional Giphy. Existing durable background/tray/notification services fit, but timer state and restart semantics need tests.
- **YouTube:** YouTube API key preference, search/recent videos/channels and grid. Straightforward paginated HTTP/data flow with network fixtures; API key and thumbnails require explicit configuration.
- **Music:** 30 Apple Music controls, library/search/playlist commands. macOS automation with substantial typed state; requires real Apple Music acceptance.

## Recommended implementation order

Begin the top unported priority group in independent repositories: Google Chrome, Linear, Slack, Brew, Notion, ChatGPT, 1Password and Arc. Chrome is assigned by the parent. Linear and Slack can be implemented with fixture-backed HTTP before account acceptance; Brew can be implemented with fixture-backed CLI before Mac acceptance. Existing source credentials/OAuth UX must remain explicit and must not silently become app-wide credentials.

For further work that avoids account prerequisites while higher-ranked integrations are underway, Obsidian, Coffee, Warp and Pomodoro have mostly local behavior. This is a concurrency suggestion, not permission to mark higher-ranked entries blocked or to skip their implementation.

## Existing native mappings with upstream command drift

Exact Store-command-name comparison identified these missing native manifest commands:

- Google Translate: `quick-translate-view` (Store 7, native 6).
- Visual Studio Code: `open-agents-window` (Store 7, native 6).
- Google Search: `aiSearch` (Store 3, native 2).
- Lorem Ipsum: `html`, `markdown` (Store 6, native 4).

Kill Process has three extra native commands also represented as upstream tools; this is not an uncovered Store command. This comparison checks identifiers only; matching names do not prove runtime parity. Full evidence: [mapped command audit](popular-mapped-command-audit.json).

## Reproduce and continue

Run `python scripts/raycast-store-inventory.py --pages 5` for the first 50 ranked entries, `--pages 10` for the first 100, or `--pages 0` for all observed numbered pages. Cached pages resume without refetching; `--refresh` updates observations. The script retries transient network failures with bounded backoff, fetches sequentially with a 0.5-second delay, fails on unexpected markup, and excludes the three repeated featured cards from ranked counts. It records page traversal separately from identity coverage and flags repeated listing slots.

No builds, account mutations, releases, catalog writes, checkout changes or child agents were used for this research.
