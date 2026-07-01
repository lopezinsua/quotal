# Contributing to Quotal

Thanks for your interest in Quotal! It's a small, native desktop widget that shows
your live Claude usage. This guide covers how to build it, the quality bar CI
enforces, and how to extend it.

If you're touching internals, read [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
first — it explains the two data pipelines, the data contract, and the extension
points. The broader technical plan lives in [`docs/PLAN.md`](docs/PLAN.md).

## Prerequisites

- [Rust](https://rustup.rs) (stable) and [Node.js](https://nodejs.org) 20+.
- On **Linux**, the WebKitGTK / app-indicator dev packages:

  ```bash
  sudo apt-get install -y libwebkit2gtk-4.1-dev librsvg2-dev patchelf libayatana-appindicator3-dev
  ```

## Getting started

```bash
npm install
npm run dev      # run the app in development
npm run build    # produce a native installer for the current OS
```

## Before you open a PR

CI runs on every push and pull request and must be green. Run the same checks
locally first:

**Frontend (JS)**

```bash
npm run lint     # eslint (mainly catches no-undef)
npm test         # vitest
```

**Backend (Rust)** — from `src-tauri/`:

```bash
cargo fmt --all --check              # formatting
cargo clippy --all-targets -- -D warnings   # lint; warnings fail the build
cargo test --all                     # unit + integration tests
```

CI additionally runs, on Linux:

- **Coverage** via `cargo llvm-cov --workspace --fail-under-lines 45`. Don't let
  line coverage drop below the floor; ratchet it up when you add tests.
- **`cargo audit`** — fails on vulnerabilities (informational unsound/unmaintained
  advisories don't break the build).
- **`npm audit --audit-level=high`**.

And a cross-platform `cargo check` on Windows and macOS, so platform-specific code
(`#[cfg(...)]`, `if cfg!(windows)`) can't break silently.

## Conventions

- **Keep the UI thread free.** All file watching and network I/O runs on Tokio
  tasks; never block the main thread.
- **Security first.** Quotal reuses Claude Code's local OAuth token and touches its
  files. Never log tokens; keep credential writes atomic (tmp + rename) and
  compare-and-swap against Claude Code; respect read-only mode (see below).
- **Match the surrounding style.** Comment density and naming follow the existing
  code (comments are in Spanish; user-facing strings go through i18n).
- **Commit messages**: conventional style (`feat:`, `fix:`, `test:`, `docs:`,
  `refactor:`, `chore:`). Do **not** add `Co-Authored-By` trailers.
- **Small, atomic PRs** are preferred over large ones.

### i18n

User-facing strings live in `src/i18n.js` (the English `BASE`) and per-language
files in `src/locales/*.json`. Missing keys fall back to `BASE`, so at minimum add
your key to `i18n.js`; add `es.json` too when you can. Never hardcode UI text.

### Read-only mode

Quotal has an observer mode (`app_config.rs`) that must never write to Claude
Code's files. If you add anything that writes to disk under `~/.claude`, gate it on
`app_config::is_read_only()` and add a test proving it's a no-op when enabled.

## Adding a usage source (provider)

Context sources implement the `ContextProvider` trait in `src-tauri/src/providers.rs`.
This is the seam meant to keep Quotal decoupled from Claude Code internals — e.g. a
future official usage API would be just another provider. To add one:

1. Implement `ContextProvider` for a new struct; `read()` returns
   `UsageMetrics::from_tokens(id, label, used, limit, age)`.
2. Add it to `providers::all()` at its priority position.
3. Register its `id` in `SharedState::set_context` and `Dirty::is_set` (poller), and
   its priority in `SharedState::active`.
4. If it's file-backed, map its path in `poller::classify`.

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the full picture.

## Reporting bugs

Open an [issue](https://github.com/lopezinsua/quotal/issues) with your OS, Quotal
version (Settings → about, or `get_config`), and steps to reproduce. If it looks
like Quotal stopped reading Claude Code correctly, check
`~/.claude-usage-widget/schema_error.log` — that's the schema-drift safety net, and
its contents (with the Claude Code version) are very helpful.

## License

By contributing, you agree that your contributions are licensed under the project's
[MIT license](LICENSE).
