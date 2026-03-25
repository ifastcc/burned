# Burned

Burned is a desktop-first usage observability app focused on Codex and Claude Code. It indexes local sessions from those two sources and exposes a stable usage layer for daily token and cost tracking.

## Recommended Stack

- `Tauri 2`: native desktop shell with low memory overhead and direct access to the local machine
- `React + TypeScript + Vite`: fast UI iteration for the dashboard and source setup flows
- `Rust backend`: source adapters, filesystem scanning, and SQLite writes
- `SQLite`: local usage ledger, session index, and aggregation store

This repository starts with a desktop shell, a product-shaped dashboard, and a Rust command surface centered on two trustworthy local adapters.

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

For quick local control while iterating on the browser-mode app:

```bash
./burned.sh start
./burned.sh status
./burned.sh restart
./burned.sh stop
```

## Installable CLI Package

Burned now exposes an npm-style CLI entrypoint:

```bash
pnpm pack
pnpm add -g ./burned-0.1.0.tgz
burned
```

You can also run the packed tarball without a global install:

```bash
npm exec --yes --package=./burned-0.1.0.tgz burned
```

The package is source-visible and ships the built frontend with the Rust source. On first run it builds the `burned-web` Rust binary into `~/.burned/cargo-target`, so a working Rust toolchain must be available on `PATH`.

Release helpers also live at the project root now:

```bash
./release.sh patch
./release.sh minor
./release.sh major
```

## Initial Product Boundary

- index sessions from Codex and Claude Code without forcing one transcript schema
- extract or recompute per-session usage from those two local sources
- classify usage as `native`, `derived`, or `estimated`
- aggregate daily totals by source, model, and workspace

## Next Steps

1. Add SQLite migrations and a persistent usage ledger in Rust.
2. Keep Codex and Claude Code analytics internally consistent as the snapshot model grows.
3. Implement session recomputation and source health diagnostics.
4. Add source onboarding and workspace-level filters.
