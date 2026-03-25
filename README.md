# Burned

[简体中文](./README.zh-CN.md)

Burned is a local usage dashboard for Codex and Claude Code.

It reads local session data, turns it into a consistent usage layer, and lets you see token and cost trends without digging through tool-specific storage.

## What It Does

- Read local sessions from `Codex` and `Claude Code`.
- Show daily tokens, cost, recent sessions, and source-level trends in one dashboard.
- Keep source-native titles and previews so you can quickly find what actually caused a spike.
- Mark usage as `native`, `derived`, or `estimated` so the numbers have clear provenance.
- Run locally on your machine instead of sending your session history to a hosted service.

## Supported Sources

- `Codex`
- `Claude Code`

## Quick Start

Install from npm:

```bash
npm install -g burned
burned
```

Run from source:

```bash
pnpm install
pnpm build
./burned
```

On first run, Burned builds the `burned-web` Rust binary locally, so you need a working Rust toolchain on `PATH`.

## Development

```bash
pnpm dev
pnpm rust:check
pnpm tauri:dev
pnpm test
```

For quick local control while iterating on the browser-mode app:

```bash
./burned.sh start
./burned.sh status
./burned.sh restart
./burned.sh stop
```

## Release

```bash
./release.sh patch
./release.sh minor
./release.sh major
```

## Community support

Many thanks to the linux.do community; it's a very loving and professional community.
