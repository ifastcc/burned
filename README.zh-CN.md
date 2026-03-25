# Burned

[English](./README.md)

Burned 是一个给 Codex 和 Claude Code 用的本地用量面板。

它会读取本地 session 数据，整理成统一的统计视图，让你不用翻各个工具自己的存储格式，也能看清 token 和成本变化。

## 它能做什么

- 读取 `Codex` 和 `Claude Code` 的本地 session。
- 在一个界面里查看每天的 token、成本、最近 session 和来源趋势。
- 保留来源原生的标题和预览，方便你快速定位是哪次对话把消耗拉高了。
- 用 `native`、`derived`、`estimated` 标明统计来源，数字来路更清楚。
- 所有分析都在本地完成，不需要把 session 历史发到托管服务。

## 当前支持

- `Codex`
- `Claude Code`

## 快速开始

从 npm 安装：

```bash
npm install -g burned
burned
```

从源码运行：

```bash
pnpm install
pnpm build
./burned
```

首次运行时，Burned 会在本地编译 `burned-web` 这个 Rust 二进制，所以你的 `PATH` 里需要有可用的 Rust 工具链。

## 开发

```bash
pnpm dev
pnpm rust:check
pnpm tauri:dev
pnpm test
```

如果你主要在调试浏览器模式，可以直接用下面这些命令：

```bash
./burned.sh start
./burned.sh status
./burned.sh restart
./burned.sh stop
```

## 发布

```bash
./release.sh patch
./release.sh minor
./release.sh major
```

## Community support

Many thanks to the linux.do community; it's a very loving and professional community.

感谢 linux.do 社区。它一直都很有爱，也很专业。
