//! Native copies of Tezbar’s existing launcher glyphs and command mappings.
//! Source: src/renderer/launcherIcons.tsx (Apache-2.0); fixed compile-time SVG data only.
//! Source SHA-256: b0b8fdcb6c2f891e92454e904da03881bb89f6760e7b4d469f1fd17a347930ab
use super::{Action, Mode, Row};
use gpui::{prelude::*, *};

fn command_kind(row: &Row) -> &str {
    match &row.action {
        Action::InvokeCommand { command_id, .. } => match command_id.as_str() {
            "open-settings" => "settings",
            "open-extensions-settings" => "settings",
            "open-extensions" => "extensions",
            "open-snippets" => "snippets",
            "open-vault" => "key",
            "open-notes" => "notes",
            "open-emoji-picker" => "emoji",
            "open-indexing" => "indexing",
            "start-onboarding" => "onboarding",
            "download-app-update" => "update",
            "restart-app-update" => "update",
            "check-for-updates" => "update",
            "open-settings-updates" => "update",
            "activate-deep-search" => "deep-search",
            "show-updated-version" => "notes",
            _ => &row.category,
        },
        Action::RunNativeCommand { command_id } => match command_id.as_str() {
            "toggle-dark-mode" => "moon",
            "start-screen-saver" => "display",
            "sleep-display" => "display-sleep",
            "toggle-mute" => "volume-muted",
            "volume-up" => "volume",
            "volume-down" => "volume",
            "toggle-hide-desktop-icons" => "desktop",
            "toggle-autohide-dock" => "dock",
            "toggle-autohide-menu-bar" => "menu-bar",
            "restart-dock" => "dock",
            "restart-finder" => "finder",
            "restart-menu-bar" => "menu-bar",
            "start-keep-awake" => "awake",
            "stop-keep-awake" => "sleep",
            "sleep-system" => "sleep",
            "toggle-bluetooth" => "bluetooth",
            "toggle-wifi" => "wifi",
            "show-network-info" => "network",
            "show-public-ip" => "ip",
            "flush-dns-cache" => "dns",
            "toggle-vpn-menu" => "vpn",
            "empty-trash" => "trash",
            "lock-screen" => "lock",
            "open-downloads" => "folder-downloads",
            "open-applications" => "applications",
            "reveal-library" => "library",
            "copy-current-path" => "copy-path",
            "quit-tezbar" => "quit",
            "show-macos-version" => "macos",
            "show-cpu-info" => "cpu",
            "show-system-monitor" => "cpu",
            "show-memory-info" => "memory",
            "show-disk-usage" => "disk",
            "show-battery-status" => "battery",
            "git-root" => "git",
            "brew-outdated" => "brew",
            "brew-update" => "brew",
            "open-clipboard-history" => "clipboard",
            "open-snippets" => "snippets",
            "open-vault" => "key",
            "open-quick-notes" => "notes",
            "open-emoji-picker" => "emoji",
            "generate-password" => "key",
            _ => &row.category,
        },
        Action::RunExtensionCommand { extension_id, .. } => extension_id,
        _ => &row.category,
    }
}

fn icon(kind: &str) -> (u32, &'static str) {
    match kind {
        "settings" => (
            0x64748b,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M20 7h-9M14 17H5"></path><circle cx="17" cy="17" r="3"></circle><circle cx="7" cy="7" r="3"></circle></svg>"##,
        ),
        "onboarding" => (
            0x6366f1,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M4 22V4s1-1 4-1 5 2 8 2 4-1 4-1v11s-1 1-4 1-5-2-8-2-4 1-4 1"></path></svg>"##,
        ),
        "deep-search" => (
            0x06b6d4,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M3 7V5a2 2 0 0 1 2-2h2M17 3h2a2 2 0 0 1 2 2v2M21 17v2a2 2 0 0 1-2 2h-2M7 21H5a2 2 0 0 1-2-2v-2"></path><circle cx="12" cy="12" r="3"></circle><path d="m16 16-1.9-1.9"></path></svg>"##,
        ),
        "indexing" => (
            0x0ea5e9,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><ellipse cx="12" cy="5" rx="9" ry="3"></ellipse><path d="M3 5v14a9 3 0 0 0 18 0V5M3 12a9 3 0 0 0 18 0"></path></svg>"##,
        ),
        "extensions" | "store" => (
            0xf97316,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><rect width="7" height="7" x="14" y="3" rx="1"></rect><path d="M10 21V8a1 1 0 0 0-1-1H4a1 1 0 0 0-1 1v12a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-5a1 1 0 0 0-1-1H3"></path></svg>"##,
        ),
        "snippets" | "code" => (
            0x14b8a6,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3H7a2 2 0 0 0-2 2v5a2 2 0 0 1-2 2 2 2 0 0 1 2 2v5c0 1.1.9 2 2 2h1"></path><path d="M16 21h1a2 2 0 0 0 2-2v-5c0-1.1.9-2 2-2a2 2 0 0 1-2-2V5a2 2 0 0 0-2-2h-1"></path></svg>"##,
        ),
        "notes" | "quick-notes" => (
            0xf59e0b,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M15.5 3H5a2 2 0 0 0-2 2v14c0 1.1.9 2 2 2h14a2 2 0 0 0 2-2V8.5Z"></path><path d="M15 3v6h6"></path></svg>"##,
        ),
        "edit" | "pencil" => (
            0xf43f5e,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z"></path><path d="m15 5 4 4"></path></svg>"##,
        ),
        "trophy" | "sports" => (
            0xf59e0b,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9H4.5a2.5 2.5 0 0 1 0-5H6"></path><path d="M18 9h1.5a2.5 2.5 0 0 0 0-5H18"></path><path d="M4 22h16"></path><path d="M10 14.66V17c0 .55-.45 1-1 1H8v4h8v-4h-1c-.55 0-1-.45-1-1v-2.34"></path><path d="M6 4h12v5a6 6 0 0 1-12 0V4Z"></path></svg>"##,
        ),
        "weather" | "sun" => (
            0xf59e0b,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="4"></circle><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"></path></svg>"##,
        ),
        "ai" => (
            0x06b6d4,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="m12 3-1.9 5.8a2 2 0 0 1-1.3 1.3L3 12l5.8 1.9a2 2 0 0 1 1.3 1.3L12 21l1.9-5.8a2 2 0 0 1 1.3-1.3L21 12l-5.8-1.9a2 2 0 0 1-1.3-1.3Z"></path></svg>"##,
        ),
        "emoji" => (
            0xf59e0b,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><path d="M8 14s1.5 2 4 2 4-2 4-2M9 9h.01M15 9h.01"></path></svg>"##,
        ),
        "clipboard" => (
            0x64748b,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><rect width="8" height="4" x="8" y="2" rx="1"></rect><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"></path></svg>"##,
        ),
        "terminal" | "commands" | "native-command" | "mac-cli" => (
            0x27272a,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="m4 17 6-6-6-6"></path><path d="M12 19h8"></path></svg>"##,
        ),
        "key" | "token" | "auth" | "vault" => (
            0x22c55e,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="m15.5 7.5 2.3 2.3a1 1 0 0 0 1.4 0l2.1-2.1a1 1 0 0 0 0-1.4L19 4M21 2l-9.6 9.6"></path><circle cx="7.5" cy="15.5" r="5.5"></circle></svg>"##,
        ),
        "moon" => (
            0x8b5cf6,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"></path></svg>"##,
        ),
        "display" => (
            0x3b82f6,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><rect width="20" height="14" x="2" y="3" rx="2"></rect><path d="M8 21h8M12 17v4"></path></svg>"##,
        ),
        "display-sleep" => (
            0x3b82f6,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><rect width="20" height="14" x="2" y="3" rx="2"></rect><path d="M8 21h8M12 17v4"></path><path d="M10 7.5h4l-4 4h4"></path></svg>"##,
        ),
        "volume" => (
            0xec4899,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M11 4.7a.7.7 0 0 0-1.2-.5L6.4 7.6A1.4 1.4 0 0 1 5.4 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.4a1.4 1.4 0 0 1 1 .4l3.4 3.4a.7.7 0 0 0 1.2-.5Z"></path><path d="M16 9a5 5 0 0 1 0 6M19.4 18.4a9 9 0 0 0 0-12.8"></path></svg>"##,
        ),
        "volume-muted" => (
            0xec4899,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M11 4.7a.7.7 0 0 0-1.2-.5L6.4 7.6A1.4 1.4 0 0 1 5.4 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.4a1.4 1.4 0 0 1 1 .4l3.4 3.4a.7.7 0 0 0 1.2-.5Z"></path><path d="m22 9-6 6M16 9l6 6"></path></svg>"##,
        ),
        "desktop" => (
            0x3b82f6,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><rect width="20" height="14" x="2" y="3" rx="2"></rect><path d="M8 21h8M12 17v4"></path><path d="M6 7h2M6 10.5h2"></path></svg>"##,
        ),
        "dock" => (
            0x6366f1,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2"></rect><path d="M3 15h18M8 18h.01M12 18h.01M16 18h.01"></path></svg>"##,
        ),
        "menu-bar" => (
            0x6366f1,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2"></rect><path d="M3 9h18"></path></svg>"##,
        ),
        "finder" => (
            0x0ea5e9,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2"></rect><path d="M12 3c-1.5 3-2 6-1.5 10H13M8 8.5v1M16 8.5v1M7.5 16c2.5 1.5 6.5 1.5 9 0"></path></svg>"##,
        ),
        "awake" | "amphetamine" | "coffee" => (
            0xf59e0b,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M17 8h1a4 4 0 1 1 0 8h-1"></path><path d="M3 8h14v9a4 4 0 0 1-4 4H7a4 4 0 0 1-4-4Z"></path><line x1="6" y1="2" x2="6" y2="4"></line><line x1="10" y1="2" x2="10" y2="4"></line><line x1="14" y1="2" x2="14" y2="4"></line></svg>"##,
        ),
        "sleep" => (
            0x8b5cf6,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M4 5h6l-6 7h6M14 11h6l-6 8h6"></path></svg>"##,
        ),
        "bluetooth" => (
            0x3b82f6,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="m7 7 10 10-5 5V2l5 5L7 17"></path></svg>"##,
        ),
        "wifi" => (
            0x06b6d4,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h.01M2 8.8a15 15 0 0 1 20 0M5 12.9a10 10 0 0 1 14 0M8.5 16.4a5 5 0 0 1 7 0"></path></svg>"##,
        ),
        "network" => (
            0x06b6d4,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20M2 12h20"></path></svg>"##,
        ),
        "ip" => (
            0x06b6d4,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M20 10c0 5-5.5 10.2-7.4 11.8a1 1 0 0 1-1.2 0C9.5 20.2 4 15 4 10a8 8 0 0 1 16 0"></path><circle cx="12" cy="10" r="3"></circle></svg>"##,
        ),
        "dns" => (
            0x06b6d4,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><rect width="20" height="8" x="2" y="2" rx="2"></rect><rect width="20" height="8" x="2" y="14" rx="2"></rect><path d="M6 6h.01M6 18h.01"></path></svg>"##,
        ),
        "vpn" => (
            0x22c55e,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M20 13c0 5-3.5 7.5-7.7 9a1 1 0 0 1-.6 0C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.2-2.7a1.2 1.2 0 0 1 1.6 0C14.5 3.8 17 5 19 5a1 1 0 0 1 1 1Z"></path><path d="m9 12 2 2 4-4"></path></svg>"##,
        ),
        "trash" => (
            0xf43f5e,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"></path></svg>"##,
        ),
        "lock" => (
            0x71717a,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="11" x="3" y="11" rx="2"></rect><path d="M7 11V7a5 5 0 0 1 10 0v4"></path></svg>"##,
        ),
        "folder-downloads" => (
            0x0ea5e9,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"></path><path d="M12 10v6M15 13l-3 3-3-3"></path></svg>"##,
        ),
        "applications" | "application" => (
            0x6366f1,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><rect width="7" height="7" x="3" y="3" rx="1.5"></rect><rect width="7" height="7" x="14" y="3" rx="1.5"></rect><rect width="7" height="7" x="14" y="14" rx="1.5"></rect><rect width="7" height="7" x="3" y="14" rx="1.5"></rect></svg>"##,
        ),
        "library" => (
            0xf97316,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M3 22h18M6 18v-7M10 18v-7M14 18v-7M18 18v-7M12 2l8 5H4Z"></path></svg>"##,
        ),
        "copy-path" => (
            0x64748b,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2"></rect><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"></path></svg>"##,
        ),
        "quit" | "kill-process" => (
            0xf43f5e,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2v10M18.4 6.6a9 9 0 1 1-12.8 0"></path></svg>"##,
        ),
        "macos" => (
            0x71717a,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M18 5a2 2 0 0 1 2 2v8.5a2 2 0 0 0 .2.9l1.1 2.1a1 1 0 0 1-.9 1.5H3.6a1 1 0 0 1-.9-1.5l1.1-2.1a2 2 0 0 0 .2-.9V7a2 2 0 0 1 2-2ZM20 16H4"></path></svg>"##,
        ),
        "cpu" | "system-monitor" => (
            0x27272a,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M22 12h-4l-3 9L9 3l-3 9H2"></path></svg>"##,
        ),
        "memory" => (
            0x14b8a6,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M2 7a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2Z"></path><path d="M6 19v-3M10 19v-3M14 19v-3M18 19v-3M8 11V9M12 11V9M16 11V9"></path></svg>"##,
        ),
        "disk" => (
            0x14b8a6,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M22 12H2M5.5 5.1 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.5-6.9A2 2 0 0 0 16.8 4H7.2a2 2 0 0 0-1.7 1.1Z"></path><path d="M6 16h.01M10 16h.01"></path></svg>"##,
        ),
        "battery" => (
            0x22c55e,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M15 7h1a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2h-2M6 7H4a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2h1M11 7l-3 5h4l-3 5M22 11v2"></path></svg>"##,
        ),
        "ports" | "port-manager" => (
            0xf97316,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="14" x="3" y="5" rx="2"></rect><path d="M7 15v4M11 15v4M15 15v4M7 9h10"></path></svg>"##,
        ),
        "git" => (
            0xf97316,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M6 3v12M18 9a9 9 0 0 1-9 9"></path><circle cx="18" cy="6" r="3"></circle><circle cx="6" cy="18" r="3"></circle></svg>"##,
        ),
        "brew" => (
            0xf59e0b,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M6 2v2M10 2v2M14 2v2M16 8a1 1 0 0 1 1 1v8a4 4 0 0 1-4 4H7a4 4 0 0 1-4-4V9a1 1 0 0 1 1-1h14a4 4 0 1 1 0 8h-1"></path></svg>"##,
        ),
        "update" => (
            0x22c55e,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M12 17V3M6 11l6 6 6-6M19 21H5"></path></svg>"##,
        ),
        "chat" => (
            0x8b5cf6,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z"></path></svg>"##,
        ),
        "audio-device" | "audio" => (
            0xec4899,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M3 14h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-7a9 9 0 0 1 18 0v7a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3"></path></svg>"##,
        ),
        "cleanshotx" | "cleanshot" | "clean-shot" | "clean-shot-x" => (
            0x6366f1,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z"></path><circle cx="12" cy="13" r="3"></circle></svg>"##,
        ),
        "color-picker" | "colorpicker" => (
            0xf43f5e,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="m14 7 3 3"></path><path d="M4 17 2 22l5-2"></path><path d="m19.5 8.5-5-5a2.12 2.12 0 0 0-3 0L3.5 11.5a2.12 2.12 0 0 0 0 3l5 5a2.12 2.12 0 0 0 3 0l8-8a2.12 2.12 0 0 0 0-3Z"></path></svg>"##,
        ),
        "gif-search" | "gif" | "gifs" => (
            0x06b6d4,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2" ry="2"></rect><circle cx="9" cy="9" r="2"></circle><path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"></path></svg>"##,
        ),
        "google-calendar" | "calendar" => (
            0x3b82f6,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="4" rx="2"></rect><line x1="16" y1="2" x2="16" y2="6"></line><line x1="8" y1="2" x2="8" y2="6"></line><line x1="3" y1="10" x2="21" y2="10"></line><rect width="2" height="2" x="8" y="14" rx="0.5"></rect><rect width="2" height="2" x="14" y="14" rx="0.5"></rect></svg>"##,
        ),
        "google-search" => (
            0x3b82f6,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"></circle><line x1="21" y1="21" x2="16.65" y2="16.65"></line></svg>"##,
        ),
        "perplexity" => (
            0x14b8a6,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="m12 3-1.9 5.8a2 2 0 0 1-1.3 1.3L3 12l5.8 1.9a2 2 0 0 1 1.3 1.3L12 21l1.9-5.8a2 2 0 0 1 1.3-1.3L21 12l-5.8-1.9a2 2 0 0 1-1.3-1.3Z"></path></svg>"##,
        ),
        "speedtest" | "speed-test" => (
            0x22c55e,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2v2M4.93 4.93l1.41 1.41M2 12h2M20 12h2M17.66 6.34l1.41-1.41"></path><path d="M12 18a6 6 0 1 0 0-12 6 6 0 0 0 0 12Z"></path><path d="m14 10-3 3"></path></svg>"##,
        ),
        "timers" | "timer" => (
            0xf59e0b,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><line x1="10" y1="2" x2="14" y2="2"></line><line x1="12" y1="14" x2="12" y2="10"></line><circle cx="12" cy="14" r="8"></circle></svg>"##,
        ),
        "directory" => (
            0x0ea5e9,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"></path></svg>"##,
        ),
        "file" | "files" | "knowledge" => (
            0x3b82f6,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"></path><path d="M14 2v4a2 2 0 0 0 2 2h4M16 13H8M16 17H8M10 9H8"></path></svg>"##,
        ),
        "quick-links" => (
            0x3b82f6,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"></path><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"></path></svg>"##,
        ),
        _ => {
            // Match the old deterministic extension tile without storing per-row images.
            const COLORS: [u32; 12] = [
                0x3b82f6, 0x0ea5e9, 0x06b6d4, 0x14b8a6, 0x22c55e, 0xf59e0b, 0xf97316, 0xf43f5e,
                0xec4899, 0x8b5cf6, 0x6366f1, 0x64748b,
            ];
            let hash = kind.encode_utf16().fold(0i32, |hash, unit| {
                hash.wrapping_mul(31).wrapping_add(unit as i32)
            });
            (
                COLORS[hash.unsigned_abs() as usize % COLORS.len()],
                icon("extensions").1,
            )
        }
    }
}

pub(super) fn tile(row: &Row, edge: f32) -> Div {
    let raw = command_kind(row).trim().to_lowercase();
    let kind = ["com.tezbar.native.", "raycast.", "com.tezbar."]
        .iter()
        .find_map(|prefix| raw.strip_prefix(prefix))
        .unwrap_or(&raw);
    let (color, data) = icon(kind);
    div()
        .size(px(edge))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px((edge * 0.27).round()))
        .bg(rgb(color))
        .child(
            svg()
                .data(data.as_bytes())
                .size(px((edge * 0.58).round()))
                .text_color(rgb(0xffffff)),
        )
}

pub(super) fn search_badge(mode: &Mode, zoom: f32) -> Div {
    let mut badge = div()
        .size(px(24. * zoom))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(7. * zoom))
        .text_color(rgb(0xffffff));
    if *mode == Mode::Ai {
        return badge
            .bg(rgb(0x8b5cf6))
            .text_size(px(11. * zoom))
            .font_weight(FontWeight::SEMIBOLD)
            .child("AI");
    }
    let (color, glyph) = match mode {
        Mode::Terminal => (0x059669, icon("terminal").1),
        Mode::Applications => (0x6366f1, icon("applications").1),
        Mode::Deep => (0x06b6d4, icon("deep-search").1),
        _ => (
            0x5b5ef0,
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 14 14" fill="none" stroke="white"><circle cx="6" cy="6" r="4.1" stroke-width="1.3"/><path d="m9.3 9.3 2.4 2.4" stroke-width="1.3" stroke-linecap="round"/></svg>"#,
        ),
    };
    badge = badge.bg(rgb(color));
    badge.child(
        svg()
            .data(glyph.as_bytes())
            .size(px(14. * zoom))
            .text_color(rgb(0xffffff)),
    )
}
