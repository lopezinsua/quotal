# Changelog

All notable changes to Quotal are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- **Never overwrites a broken `settings.json`.** If Claude Code's `settings.json`
  can't be parsed (a stray trailing comma, or a read while Claude Code was
  rewriting it), installing or removing any hook used to start from `{}` and
  write over it, **wiping your whole Claude Code config**. Quotal now leaves the
  file untouched and tells you why.
- **Rate limits (429) are respected.** Quotal honours the server's `Retry-After`
  and otherwise backs off exponentially (2 → 4 → 8 … up to 30 min) instead of
  retrying every 3 minutes, which could keep the limit going for hours. The
  refresh button no longer hits the endpoint while the limit is active.
- **No more "99% · resets now" from a dead window.** When the live data can't be
  refreshed, a session or weekly window whose reset time has passed is no longer
  shown frozen: Quotal uses the statusLine data if it's current, or shows "—".
  The same applies to the cached plan shown at startup.
- **Context of your session, not a subagent's.** Subagent transcripts
  (`subagents/agent-*.jsonl`) were picked as "the latest session" while they ran.
  They're now ignored, sessions over 200k are measured against the 1M window, and
  the watcher no longer rescans every transcript on each write.
- **Placement respects the taskbar / menu bar.** The position grid and the
  default position now use the monitor's work area, so bottom positions no longer
  land under the Windows taskbar (or the top one under the macOS menu bar).
- **Corrupted preferences can't hide the widget.** Unreadable saved preferences
  used to stop the UI from starting, leaving the (initially hidden) window
  invisible. They now fall back to defaults.
- Usage notifications no longer fire twice in the same window when the server's
  reset timestamp shifts by a few seconds between polls.
- Linux: no more false "3 missing system dependencies" warning when `ldconfig`
  isn't on the user's `PATH` (Debian).
- "Resets in 0m" during the last minute now reads "Resets in 1m".

### Changed
- **Toggle errors are explained.** When "Open/Close with Claude Code" or
  "Official context" can't be applied (read-only mode, a broken `settings.json`,
  Node.js missing), Settings now says why instead of silently flipping back.
- The statusLine bridge now refuses to install when Node.js isn't on the `PATH`
  (it would have left you with no status line), and keeps your `statusLine`
  options such as `padding`.
- **Translated tray menu and tooltip** (they were always in Spanish), and 11
  strings that were still in English in 9 languages are now translated. A test
  keeps every language in sync with the English source.

### Security
- Updated dependencies with published advisories: `rustls` 0.23.45 and `h2`
  0.4.20 (used by the updater's HTTPS), `byte-unit` 5.2.6 (drops the vulnerable
  `rkyv` 0.7), and the build/test toolchain (`vitest`, `postcss`, `nanoid`,
  `brace-expansion`). `cargo audit` and `npm audit` are clean again.

## [0.3.4] — 2026-07-10

### Fixed
- **Window position now behaves as documented.**
  - **Edge snapping is real**: releasing a drag near a screen edge — or with the
    window partly off-screen — aligns it to that edge with the standard margin,
    respecting the monitor's work area (it won't slide under the taskbar).
  - **Multi-monitor restore**: the remembered position is restored on *its own*
    monitor (per-monitor DPI aware), instead of being clamped onto whichever
    monitor the window spawns on at startup.
  - The corner anchor now **always tracks the window** after a move or resize,
    even with "Remember position" off, so collapsing/expanding no longer jumps
    back to a stale corner. The preference only governs restore-on-launch.
  - Pausing mid-drag no longer collapses the widget "in your hand": the drag-end
    fallback now asks the OS whether the primary mouse button is still down
    (swapped-buttons aware) before ending the gesture.
- **"Close with Claude Code" with several sessions open**: the `SessionEnd` hook
  now closes the widget only when the ending session was the **last** live
  Claude Code session. Previously, any session ending — a stray `claude -p`, one
  of several terminals, or an IDE session — took the widget down while others
  (e.g. your editor's integrated terminal) were still running, which made
  auto-open appear broken in IDEs.

## [0.3.3] — 2026-07-01

### Added
- **Light & dark themes** plus a **pickable accent color** (green / blue / violet
  / amber), from the Settings panel. Applied live, no restart.
- **Desktop notifications** when your session or weekly usage crosses a
  **configurable threshold** (75–95%). Opt-in, with edge detection so it warns
  once per window and never spams on startup.
- **Read-only / observer mode**: a switch that stops Quotal writing anything back
  (OAuth token refresh write-back and hook installs), for users who want a purely
  passive widget.
- **Actionable schema-drift banner**: when Claude Code changes a data format
  Quotal no longer recognises, the banner now offers to check for a Quotal update
  and shows the observed Claude Code version.

### Changed
- **Resilience**: the context watcher falls back to polling when the OS file
  watcher (`notify`) is unavailable, and the `statusLine` wrapper is regenerated
  on startup so it keeps pointing at the current executable after an update.

### Security
- Every release now publishes a **`SHA256SUMS.txt`** so you can verify your
  installer hasn't been tampered with before running it.

## [0.3.2] — 2026-06-30

### Changed
- Restored the v0.3.1 window morph (240 ms, ease-in-out) and made the pill **ring**
  style reflect the live session percentage.
- The periodic refresh now pauses while the window is hidden, and deferred
  settings are flushed, to cut idle work.

### Fixed
- Claude Code hooks are **auto-repaired on startup**, re-pointing the
  open/close launchers at the current executable path if the app moved or updated.

## [0.3.1] — 2026-06-28

### Changed
- Smoother pill ⇆ full morph: the window resize loop is now driven by real
  elapsed time (no drift) and raises the system timer resolution to 1 ms during
  the animation, so frames are even instead of stuttering. The content crossfade
  also scales and shares the same ease-in-out curve as the window.

## [0.3.0] — 2026-06-28

### Added
- **In-app update notice**: instead of updating silently, Quotal now shows a
  banner when a new version is available, with **Update**, **Dismiss** and
  **Don't show again** (the last one mutes that version until a newer one ships).
  A new "Updates" section in Settings shows the installed version and a manual
  "Check for updates" button.
- **System dependency check (Linux)**: on startup Quotal detects missing native
  libraries (WebKitGTK, Ayatana AppIndicator, librsvg) and, if any are missing,
  opens the widget to show how many, which ones, and the exact install command
  (apt/dnf/pacman) with a copy button. No-op on Windows/macOS.

### Changed
- The auto-updater no longer installs and restarts on its own; updates are now
  user-initiated from the notice.

## [0.2.0] — 2026-06-28

First release with **automatic updates**.

### Added
- **Auto-updater**: on startup the app silently checks for a newer release,
  downloads it, **verifies its minisign signature** and restarts to apply it.
  Updates are cryptographically signed, so even though the installers aren't
  OS-code-signed, every update is guaranteed authentic and tamper-free.
  (Users on 0.1.0 won't auto-update — those builds predate the updater — and
  need a one-time manual update to 0.2.0.)

### Changed
- CI now upgrades `actions/checkout` and `actions/setup-node` to v5 (Node 24
  runtime), removing the Node 20 deprecation warnings.

### Fixed
- Cross-platform Clippy failures: `ANIM_GEN` and the `Ordering` import are now
  gated to Windows (they were only used in the Windows animation path and broke
  `clippy -D warnings` on Linux/macOS).
- `usage_api`: `wrote` is now gated outside macOS, where it was never read and
  triggered an `unused_variables` error under `clippy -D warnings`.
- Restored `clippy -- -D warnings` in CI and removed the blanket
  `#![allow(dead_code)]`, so warnings are fixed at the source instead of hidden.

### Security
- Enabled GitHub secret scanning and push protection on the repository.
- Hardened `.gitignore` to never commit secret material (`.env`, `*.key`,
  `*.pfx`, `*.p12`, `*.kdbx`, …).

## [0.1.0] — 2026-06-24

Initial release.

### Added
- Always-on-top desktop widget showing Claude usage: session (5h) and weekly
  (7d) plan limits plus the active session's context window.
- Hybrid data pipeline: offline, event-driven context via a `notify` file
  watcher; live plan limits polled from `/api/oauth/usage` using Claude Code's
  local OAuth token.
- Pill mode with three styles (bar / ring / minimal), expanding on hover.
- Tray icon that changes color by severity (normal / warning / critical).
- Optional "open/close with Claude Code" hooks (fully reversible).
- 11 languages, auto-detected from the OS.
- Remembers position and size, snaps to screen edges, resizes proportionally.
- Cross-platform installers (Windows, macOS, Linux) built automatically on tag.

[0.3.4]: https://github.com/lopezinsua/quotal/compare/v0.3.3...v0.3.4
[0.3.3]: https://github.com/lopezinsua/quotal/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/lopezinsua/quotal/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/lopezinsua/quotal/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/lopezinsua/quotal/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/lopezinsua/quotal/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/lopezinsua/quotal/releases/tag/v0.1.0
