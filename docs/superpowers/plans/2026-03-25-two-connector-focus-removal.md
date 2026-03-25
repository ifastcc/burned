# Two-Connector Focus Removal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce Burned to Codex and Claude Code only by removing Cursor, Cherry Studio, and Antigravity from the backend, frontend, settings, and tests.

**Architecture:** Delete the non-primary connector modules instead of hiding them. Then prune the Cherry-specific settings/configuration surface and clean up frontend types/components so the product surface matches the reduced backend.

**Tech Stack:** Rust, Tauri, React, TypeScript, Cargo, Vite

---

### Task 1: Remove non-primary backend connectors

**Files:**
- Modify: `src-tauri/src/connectors/mod.rs`
- Delete: `src-tauri/src/connectors/cursor.rs`
- Delete: `src-tauri/src/connectors/cherry_studio.rs`
- Delete: `src-tauri/src/connectors/antigravity.rs`
- Modify: `src-tauri/Cargo.toml`

- [ ] Remove Cursor, Cherry Studio, and Antigravity module declarations and default registration.
- [ ] Delete backend connector modules that are no longer reachable.
- [ ] Remove Rust dependencies that were only needed by deleted connectors.

### Task 2: Remove Cherry settings and config APIs

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Delete: `src-tauri/src/settings.rs`
- Modify: `src-tauri/src/bin/burned-web.rs`

- [ ] Remove Tauri commands and exported helpers for app settings and Cherry backup directory management.
- [ ] Remove the HTTP API and CLI config commands that only served Cherry settings.
- [ ] Ensure the remaining binary/server surface still builds.

### Task 3: Remove dead frontend types and connector-management UI

**Files:**
- Modify: `src/data/schema.ts`
- Modify: `src/App.tsx`
- Delete: `src/components/SourceStatusBoard.tsx`

- [ ] Delete settings types that no longer exist on the backend.
- [ ] Remove unused connector-management/configuration UI.
- [ ] Keep homepage/detail rendering valid for the two remaining connectors.

### Task 4: Update tests and verify

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify any tests broken by source-name or settings removal

- [ ] Update backend tests that still mention removed connectors where necessary.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml`.
- [ ] Run `pnpm build`.
