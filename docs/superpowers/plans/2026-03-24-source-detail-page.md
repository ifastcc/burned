# Source Detail Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add daily price to the interactive trend inspectors and introduce a dedicated per-source detail page with its own history and recent sessions.

**Architecture:** Keep the homepage snapshot lightweight for the global view, then add a source-scoped snapshot shape and API/tauri command for `/sources/:sourceId`. Reuse the existing visual language so the detail page feels like the same product surface, not a separate mini-dashboard.

**Tech Stack:** Rust snapshot assembly, Tauri commands, tiny_http API handler, React/TypeScript app shell, Node test runner, Cargo tests.

---

### Task 1: Lock the new shell behavior with failing tests

**Files:**
- Modify: `scripts/product-shell.test.mjs`
- Test: `scripts/product-shell.test.mjs`

- [ ] **Step 1: Write failing shell assertions for source detail navigation and price-aware inspectors**
- [ ] **Step 2: Run `node --test scripts/product-shell.test.mjs` and verify failure**
- [ ] **Step 3: Keep the assertions minimal and tied to concrete code markers**

### Task 2: Add source-scoped snapshot support in Rust

**Files:**
- Modify: `src-tauri/src/models.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/bin/burned-web.rs`
- Test: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing Rust tests for a source detail snapshot that includes daily history, week history, totals, and sessions**
- [ ] **Step 2: Run targeted Cargo tests and verify failure**
- [ ] **Step 3: Add a source detail snapshot model and builder that reuses report aggregation**
- [ ] **Step 4: Expose the new snapshot through both Tauri and the local HTTP server**
- [ ] **Step 5: Re-run targeted Cargo tests and verify pass**

### Task 3: Add app-shell navigation and price-aware inspectors

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/data/schema.ts`
- Modify: `src/showcase-copy.mjs`
- Modify: `src/styles.css`
- Test: `scripts/product-shell.test.mjs`

- [ ] **Step 1: Introduce a minimal pathname-based app view state for home vs source detail**
- [ ] **Step 2: Extend the trend inspector to show token + day price + pending state**
- [ ] **Step 3: Make source rows navigate to the dedicated detail page**
- [ ] **Step 4: Build the source detail page with 30-day trend, 7-day detail, and recent sessions**
- [ ] **Step 5: Re-run the shell test to verify pass**

### Task 4: Verify end-to-end

**Files:**
- Modify: `package.json` (only if a new focused test command is needed)

- [ ] **Step 1: Run `pnpm test`**
- [ ] **Step 2: Run `cargo test --manifest-path src-tauri/Cargo.toml`**
- [ ] **Step 3: Run `pnpm build`**
