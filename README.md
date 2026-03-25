# Burned

Burned is a local usage dashboard for Codex and Claude Code.

Burned 是一个面向 Codex 和 Claude Code 的本地用量面板。

It reads local session data, turns it into a consistent usage layer, and lets you see token and cost trends without digging through tool-specific storage.

它会读取本地 session 数据，整理成统一的统计视图，让你不用翻各个工具自己的存储格式，也能看清 token 和成本变化。

## What It Does / 它能做什么

- Read local sessions from `Codex` and `Claude Code`.
  读取 `Codex` 和 `Claude Code` 的本地 session。
- Show daily tokens, cost, recent sessions, and source-level trends in one dashboard.
  在一个界面里查看每天的 token、成本、最近 session 和来源趋势。
- Keep source-native titles and previews so you can quickly find what actually caused a spike.
  保留来源原生的标题和预览，方便你快速定位是哪次对话把消耗拉高了。
- Mark usage as `native`, `derived`, or `estimated` so the numbers have clear provenance.
  用 `native`、`derived`、`estimated` 标明统计来源，数字来路更清楚。
- Run locally on your machine instead of sending your session history to a hosted service.
  所有分析都在本地完成，不需要把 session 历史发到托管服务。

## Supported Sources / 当前支持

- `Codex`
- `Claude Code`

## Quick Start / 快速开始

Install from npm:

从 npm 安装：

```bash
npm install -g burned
burned
```

Run from source:

从源码运行：

```bash
pnpm install
pnpm build
./burned
```

On first run, Burned builds the `burned-web` Rust binary locally, so you need a working Rust toolchain on `PATH`.

首次运行时，Burned 会在本地编译 `burned-web` 这个 Rust 二进制，所以你的 `PATH` 里需要有可用的 Rust 工具链。

## Development / 开发

```bash
pnpm dev
pnpm rust:check
pnpm tauri:dev
pnpm test
```

For quick local control while iterating on the browser-mode app:

如果你主要在调试浏览器模式，可以直接用下面这些命令：

```bash
./burned.sh start
./burned.sh status
./burned.sh restart
./burned.sh stop
```

## Release / 发布

```bash
./release.sh patch
./release.sh minor
./release.sh major
```

## Community support

Many thanks to the linux.do community; it's a very loving and professional community.

感谢 linux.do 社区。它一直都很有爱，也很专业。
