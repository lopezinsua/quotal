# Quotal

A tiny, always-on-top desktop widget that shows your **Claude usage** at a glance,
your **session (5h)** and **weekly (7d)** plan limits, plus the **context window**
of your active Claude Code session. Cross-platform (Windows, macOS, Linux), built
with **Tauri 2** (Rust backend + vanilla JS frontend, no framework).

> Quotal reuses the OAuth token that Claude Code already stores locally to read the
> **same** `/usage` data the CLI shows. It never creates or manages secrets of its own,
> and falls back to fully offline data when there's no network.

## Features

- **Real plan limits**, live from Claude — session %, weekly %, and reset times
  (the same numbers as `/usage`), not estimates.
- **Context window** of your active session (200k or 1M, model-aware), read from
  Claude Code's official `statusLine` JSON.
- **Pill mode** — collapses to a compact pill (3 styles: bar / ring / minimal) and
  expands on hover.
- **Tray icon** that changes color by severity (normal / warning / critical).
- **Open / Close with Claude Code** — optional hooks that show/hide the widget when
  you start/end a terminal session. Fully reversible.
- **11 languages**, auto-detected from your OS (English, Español, 中文, हिन्दी,
  العربية, Português, Français, Deutsch, 日本語, Русский, 한국어).
- **Remembers position & size**, snaps to screen edges, resizes proportionally.
- Offline-friendly: keeps the last good value and uses the `statusLine` data as a
  backup when the network is down.

## Install

Grab the installer for your OS from the [latest release](https://github.com/lopezinsua/quotal/releases):

- **Windows** — `.msi` or `.exe` (NSIS)
- **macOS** — `.dmg` (universal: Apple Silicon + Intel)
- **Linux** — `.AppImage` or `.deb`

> Until the app is code-signed, your OS may warn that it's from an unidentified
> developer. On Windows click *More info → Run anyway*; on macOS right-click the app
> → *Open*.

## Build from source

Requires [Rust](https://rustup.rs) and [Node.js](https://nodejs.org) 20+.

```bash
npm install
npm run dev      # run in development
npm run build    # produce a native installer for the current OS
```

On Linux you'll also need the WebKitGTK / app-indicator dev packages:

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev librsvg2-dev patchelf libayatana-appindicator3-dev
```

## How it works

Quotal runs two non-blocking data pipelines (everything lives on Tokio tasks, the UI
thread is never blocked):

1. **Context** (offline, event-driven): a `notify` file watcher reacts to Claude Code's
   transcripts and `statusLine` capture, consolidating the live context window.
2. **Plan limits** (online): polls the `/api/oauth/usage` endpoint every 60s using the
   local OAuth token, refreshing it when needed and writing it back atomically — exactly
   the way Claude Code does, so the two stay in sync.

All writes to `settings.json` and credential files are **atomic** (tmp + rename) and
**idempotent**, and any installed hook can be removed restoring your previous config.

## Release

Pushing a `vX.Y.Z` tag triggers the GitHub Actions workflow, which builds native
installers on Windows, macOS and Linux runners and attaches them to a draft release.

```bash
git tag v0.1.0
git push origin v0.1.0
```

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for the full list of changes per release.

## License

[MIT](LICENSE) © lopezinsua
