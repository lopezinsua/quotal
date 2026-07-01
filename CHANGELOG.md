# Changelog

All notable changes to Quotal are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.3.3]: https://github.com/lopezinsua/quotal/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/lopezinsua/quotal/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/lopezinsua/quotal/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/lopezinsua/quotal/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/lopezinsua/quotal/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/lopezinsua/quotal/releases/tag/v0.1.0
