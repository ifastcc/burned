# Burned

Burned is a desktop-first usage observability app for AI tooling. It is designed to index local sessions from multiple tools, preserve raw artifacts per source, and expose a stable usage layer for daily token and cost tracking.

## Recommended Stack

- `Tauri 2`: native desktop shell with low memory overhead and direct access to the local machine
- `React + TypeScript + Vite`: fast UI iteration for the dashboard and source setup flows
- `Rust backend`: source adapters, filesystem scanning, and SQLite writes
- `SQLite`: local usage ledger, session index, and aggregation store

This repository starts with a desktop shell, a product-shaped dashboard, and a minimal Rust command surface that will later be backed by real adapters and persistence.

## Getting Started

```bash
pnpm install
pnpm dev
pnpm rust:check
pnpm tauri:dev
```

To open the browser-based dashboard directly from the workspace:

```bash
./burned
```

That command reuses Burned's Rust collectors, serves the built dashboard locally, computes a fresh snapshot, and opens your default browser. If you want a bare `burned` command everywhere, symlink this launcher into a directory that is already on your `PATH`.

## Installable CLI Package

Burned now exposes an npm-style CLI entrypoint:

```bash
pnpm pack
pnpm add -g ./burned-0.1.0.tgz
burned
```

You can also run the packed tarball without a global install:

```bash
npm exec --yes --package=./burned-0.1.0.tgz burned -- config show
```

The package is source-visible and ships the built frontend with the Rust source. On first run it builds the `burned-web` Rust binary into `~/.burned/cargo-target`, so a working Rust toolchain must be available on `PATH`.

Cherry Studio backup directory configuration is persistent:

```bash
./burned config show
./burned config set cherry-backup-dir "/Users/you/Documents/cherry_data_backup"
./burned config clear cherry-backup-dir
```

## Initial Product Boundary

- index sessions from many AI tools without forcing one transcript schema
- extract or recompute per-session usage when possible
- classify usage as `native`, `derived`, or `estimated`
- aggregate daily totals by source, model, and workspace

## Next Steps

1. Add SQLite migrations and a persistent usage ledger in Rust.
2. Build connector adapters for Codex, Claude Code, Cursor, Cline, Cherry Studio, and LobeHub.
3. Implement session recomputation and source health diagnostics.
4. Add source onboarding and workspace-level filters.

## Cherry Studio Compatibility

- Prefer the local History API when it is available.
- Fall back to `agents.db` for agent sessions.
- Fall back again to legacy Cherry backup zips that include `data.json`, which can restore ordinary topic metadata and token usage without the History API.
