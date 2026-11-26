---
name: OpenDrive Control Center
description: Dark Neo-Brutalism (with Light Neo-Brutalist toggle) pure-Rust egui design system pairing dot-matrix canvas (#111217), dual Pink/Cyan 0px-blur hard offset card shadows, crisp 1.5px borders, tactile press displacement, bordered status badges, thin pastel usage bars, and clean frameless chrome.
colors:
  bg-canvas: '#111217'
  bg-dot-matrix: '#252834'
  bg-card-primary: '#181a20'
  bg-card-secondary: '#1e2029'
  bg-pill-neutral: '#232631'
  accent-pink: '#ff5e93'
  accent-cyan: '#22d3ee'
  accent-indigo: '#7c74ff'
  accent-white: '#f4f5f8'
  border-subtle: '#2e3241'
  border-strong: '#090a0f'
  text-primary: '#f3f4f6'
  text-secondary: '#9ca3af'
  text-inverted: '#111217'
  success-bg: '#133629'
  success-fg: '#34d399'
  success-border: '#1d523e'
  danger-bg: '#2a161d'
  danger-fg: '#f87171'
  danger-border: '#5c2432'
  warn-bg: '#332714'
  warn-fg: '#fbbf24'
  warn-border: '#5e4720'
  cloud-bg: '#0f2942'
  cloud-fg: '#38bdf8'
  cloud-border: '#1d4e89'
  pastel-green: '#86efac'
  pastel-orange: '#fdba74'
  pastel-red: '#fca5a5'
typography:
  title:
    fontFamily: "system-ui, -apple-system, 'Segoe UI', sans-serif"
    fontSize: '18px'
    fontWeight: 700
  card-heading:
    fontFamily: "system-ui, -apple-system, 'Segoe UI', sans-serif"
    fontSize: '18px'
    fontWeight: 600
  body:
    fontFamily: "system-ui, -apple-system, 'Segoe UI', sans-serif"
    fontSize: '14px'
    fontWeight: 400
  mono:
    fontFamily: "ui-monospace, 'Cascadia Mono', 'SFMono-Regular', Menlo, Consolas, monospace"
    fontSize: '12.5px'
rounded:
  card: '12px'
  tile: '10px'
  button: '8px'
  badge: '7px'
  avatar: '9px'
spacing:
  shell-padding: '18px'
  card-padding: '18px'
  section-gap: '16px'
---

# Design System: Dark Neo-Brutalism for `egui` (OpenDrive)

This document unifies OpenDrive's UI specification using **Dark Neo-Brutalism** (default) with an optional harmonized **Light Neo-Brutalist** toggle (`Color`), zero-blur hard offset shadows, dual Pink/Cyan chromatic card depth (Image 3 style), crisp `1.5px` strokes, tactile press displacement, and thin pastel account quota bars.

---

## 1. Core Aesthetic Principles

1. **Zero-Blur Hard Offset Shadows (Image 3 Style):** Depth never uses blurry shadows (`blur: 0`). Macro cards use solid chromatic hard offsets: dual top-left Pink (`(-2.5, -2.5)` `#FF5E93`) and bottom-right Cyan (`(+2.5, +2.5)` `#22D3EE`) ridges on `Storage & Workspace` & Settings Modal, single `(+3.5, +3.5)` Pink/Cyan offsets on macro Bento panels, and crisp `1.5px` `#090A0F` separation borders.
2. **Tactile Press Mechanics:** All enabled interactive buttons (`Primary`, `Default`, `TactilePill`, `Danger`, `Toggle`) carry `(+2.0, +2.0)` hard shadows, shift `(+1.0, +1.0)` on hover, and translate `(+2.0, +2.0)` toward the shadow when pressed (`is_pointer_button_down_on()`).
3. **High-Contrast Strokes & Selection Visibility:** Every container, subcard, `8px` button, and `7px` semantic status badge renders a deliberate `1.5px` stroke. Selection highlight (`visuals.selection`) pairs indigo fill (`#7C74FF` / `#4F46E5`) with crisp `#FFFFFF` selected text foreground.
4. **Dot-Matrix Bento Hierarchy & Keyboard Zoom:** Rounded macro cards (`12px`) sit over a `24px`-spaced `1.5px` dot-matrix canvas (`#111217` / `#252834` in Dark; soft matte `#D6D4F0` / `#B6B3DC` in Light), nesting secondary subcards (`10px`). Floating error/status toasts also render crisp `(+3.5, +3.5)` Neo-Pop offset shadows. `Ctrl/Cmd + =`, `Ctrl/Cmd + -`, and `Ctrl/Cmd + 0` adjust `egui` zoom (`0.7..=2.0`).

---

## 2. Color Tokens (`theme.rs`)

### 2.1 Dark Neo-Brutalism (Default — `ThemeMode::DarkStudio`)

| Token                         | Hex                   | RGB                                   | Usage                                                                                |
| :---------------------------- | :-------------------- | :------------------------------------ | :----------------------------------------------------------------------------------- |
| `canvas_bg`                   | `#111217`             | `(17, 18, 23)`                        | Root background behind `24px` dot grid                                               |
| `backdrop_dot`                | `#252834`             | `(37, 40, 52)`                        | `1.5px` dot-matrix grid                                                              |
| `card_bg`                     | `#181A20`             | `(24, 26, 32)`                        | Primary Bento containers (`Storage & Workspace`, `Sync Pulse`, `Connected Accounts`) |
| `card_alt`                    | `#1E2029`             | `(30, 32, 41)`                        | Nested stat boxes, account cards, settings panels                                    |
| `pill_neutral`                | `#232631`             | `(35, 38, 49)`                        | Default toolbar/action pills (`Open Settings`, `Refresh`, icon-only `Sync` toggle)   |
| `pop_pink`                    | `#FF5E93`             | `(255, 94, 147)`                      | Top-left dual shadow ridge & primary quota bar                                       |
| `pop_cyan`                    | `#22D3EE`             | `(34, 211, 238)`                      | Bottom-right dual shadow ridge & hover borders                                       |
| `accent_blue`                 | `#7C74FF`             | `(124, 116, 255)`                     | Primary CTAs (`+ Connect`, `Save`, `Export`), active switches, text selection fill   |
| `dock_bg` / `dock_fg`         | `#F4F5F8` / `#111217` | `(244, 245, 248)` / `(17, 18, 23)`    | High-contrast inverted tactile pill buttons (`Open Folder`, `Color`, `Copy Link`)    |
| `card_border` / `tactile_ink` | `#2E3241` / `#090A0F` | `(46, 50, 65)` / `(9, 10, 15)`        | `1.5px` container/pill borders and card separation ink                               |
| `cloud_fg` / `cloud_tint`     | `#38BDF8` / `#0F2942` | `(56, 189, 248)` / `(15, 41, 66)`     | Cloud Explorer `Cloud` & `Syncing` badge ink/fill (`#0284C7` / `#E0F2FE` in Light)   |
| `text_primary` / `text_muted` | `#F3F4F6` / `#9CA3AF` | `(243, 244, 246)` / `(156, 163, 175)` | Titles/metrics and paths/subtitles                                                   |

### 2.2 Semantic Status Badges (`status_pill`, `icon_status_pill`, `tooltip.rs`)

All status badges render as compact `28x24` icon-only vector pills with crisp `1.5px` borders and viewport-aware hover tooltips (`compute_smart_tooltip_rect`):

- **Active / Connected / Local:** `#133629` bg, `#34D399` fg, `#1D523E` border (pulse dot / `DownloadCheck`)
- **Cloud / Syncing:** `#0F2942` bg, `#38BDF8` fg, `#1D4E89` border (`cloud_tint` / `cloud_fg` / `cloud_border`; `#E0F2FE` / `#0284C7` in Light)
- **Paused / Standby:** `#232631` bg, `#9CA3AF` fg, `#2E3241` border
- **Warning / Pending Upload:** `#332714` bg, `#FBBF24` fg, `#5E4720` border
- **Error / Conflict:** `#2A161D` bg, `#F87171` fg, `#5C2432` border

### 2.3 Account Card Thin Bottom Pastel Usage Bar (`thin_usage_bar`)

A `5px` full-width progress line anchored along the bottom of each account card:

- `< 75%` usage: Pastel Green (`#86EFAC` Dark / `#6EE7B7` Light)
- `75%` to `< 90%` usage: Pastel Orange (`#FDBA74`)
- `>= 90%` usage: Pastel Red (`#FCA5A5`)

---

## 3. Window Chrome, Layout & Interactions

- **Frameless Resizable Window (`desktop_window.rs`, `desktop_resize.rs`):** Initial `[960, 720]`, resizable down to `[540, 480]` via 8-edge/corner perimeter hit-zones (`ViewportCommand::BeginResize`). Closing minimizes to the System Tray (`desktop_tray.rs`) while tray is active.
- **Minimal Custom Titlebar (`desktop/titlebar.rs`):** Displays ONLY the OpenDrive portal logo + non-selectable `"OpenDrive"` title on left, and `Color`/`Dark` theme switch (using 3 main app color vector dots `ThemeColor` on `"Color"` and monochrome vector dots `ThemeMono` on `"Dark"`), icon-only `Sync` pause/resume toggle (`Pause` when active, `Play` `Toggle { selected: true }` when all accounts paused, disabled with `0` accounts), icon-only `Settings` gear button (`IconKind::Settings`, cloned with tooltip, toggling Settings open/closed) to the right of `Sync`, a thin `1.5x18` vertical `card_border` divider (`vertical_divider`), and `Minimize` / `Maximize` / `Close` window controls on the right (wrapping `[Theme] [Sync] [Settings]` cleanly below `< 460px` width without overlapping). Empty space handles drag-to-move and double-click maximize; child buttons keep pointer priority.
- **Bento Overview & Main Window (`desktop_bento.rs`, `desktop_ledger.rs`, `desktop_modal.rs`):**
  - **Storage & Workspace** (`Dual` Pink/Cyan Image-3 shadow): Host mount path, `This PC Active` badge, combined storage summary + quota bar, `Open Folder`, `Open Settings`, and **Explore** (opens Cloud Explorer on the first account in list order by default).
  - **Sync Pulse** (`Cyan` shadow): Docs mode badge + global sync health pill (`Sync Active` / `Attention` / `Standby`), Cache readout, and 3-column `Transfers` / `Queue` / `Errors` subcards.
  - **Connected Accounts** (`Pink` shadow): Plain top-right `"N account(s)"` counter + `+ Connect` / `Close` trigger (no green pill badge on header). Each account card separates header (`Avatar`, editable `Alias` defaulting to email local-part before `@`, `Email`, and top-right **Explore**, **Open**, icon-only `Status` badge), wrapped action bar (`Sign In`/`Copy Link`, `Sync` switch, `Read-Only`, `Alias` inline edit with `"Also rename local mount folder"` checkbox, `Remove`, and `move_up`/`move_down` reorder controls that persist and reorder Explorer's sidebar without touching OS mounts), a full-width space-between footer row (`folder path` left, quota usage right) directly above the bottom `5px` pastel usage bar.
  - **Full-Viewport Drive Explorer (`explorer/**`, `explorer_actions.rs`):** Split Neo-Pop Bento sidebar (with **+ New Account** in the top-right header and compact account cards) + file browser with full-width breadcrumbs on Row 1 and a responsive Row 2 combining selection/action controls on the left with `[Search folder... | List | Grid | Upload | Refresh]` on the right on wide viewports (splitting into two left-aligned rows with the search/view/upload/refresh row above and selection action bar directly below when narrower). Action controls include a conditional `"{N} selected"` clear pill (shown only when `N > 0`), single-selection `Open` (`Primary`, navigating into folders or opening files), a compact **Sync** menu button (`Sync Down`, `Sync Up`, `Sync Up & Clear Local`; disabled when nothing is selected), always-visible `Copy`/`Move`/`Delete` pills (disabled when nothing mutable is selected), optional `Rename`/`Copy Stream URL` on eligible selection, and active clipboard `Paste`/`Clear`. The right-side **Upload** menu button (`Upload Files` / `Upload Folder`) sits immediately to the left of `Refresh`. Supports `List`/`Grid` views (Grid showing icon, sync badge, name, date modified, and size without clipping), sortable List columns (`Name`, `Sync State`, `Modified`, `Size`) with theme checkboxes and click-outside unselection, multi-selection (`Ctrl/Cmd+Click`, `Shift+Click`, `Ctrl/Cmd+Shift+Click`, `Ctrl/Cmd+A`, `Escape`), icon-only `Sync State` badges with hover tooltips, drag-and-drop uploads (read-only guarded), compact content-fitted context-menu pills, and centered Rename/Delete confirmation modals.
  - **Settings Modal (`desktop/modals/{settings,open_rules_ui,open_rules_actions}.rs`):** Scrollable centered modal (`768px` max width) containing `Autostart on Login`, `Show OpenDrive in This PC` (driverless `This PC` namespace integration with legacy `subst` cleanup and automatic exit/watchdog unmounting), `Export Google Docs as Office Files`, `Cache Explorer Folder Paths` (default ON; caches visited Explorer folder entries across back/forth navigation until user mutation or manual refresh), **Bandwidth Limit** & **Local Cache Limit** sliders, **Open With Rules (Custom Format Apps)** (top Search + `+ Add` row, collapsible 3-control aligned editor card with top-right `Cancel`/`Save` header buttons and `Extension / Group`, `Application`, `Browse` validation enforcing unique extension-to-app mappings, and live-filtered `Edit`/`Remove` cards), **Encrypted Credential Backup (`.odcred`)** export/import (`> 8` chars + upper + lower + digit + symbol via PBKDF2-HMAC-SHA256 + AES-256-GCM & OS file picker), and `Purge Unpinned Cache`.
