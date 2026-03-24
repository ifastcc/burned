# Pricing Estimates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add model-mapped cost estimates for sources that expose reliable model and token signals, while leaving opaque tools marked as pricing pending.

**Architecture:** Extend the shared Rust usage event model to preserve model and token breakdown fields, add a small local pricing catalog with alias normalization for OpenAI, Anthropic, and Gemini models, and compute session/source/day USD estimates inside snapshot assembly. Keep Cursor and Antigravity as non-billable until they expose trustworthy model and token fields. Surface the new `costUsd` values in the current UI without a large layout rewrite.

**Tech Stack:** Rust connector pipeline, Tauri snapshot assembly, React/TypeScript dashboard, Node test runner, Cargo tests.

---

### Task 1: Add pricing regression tests

**Files:**
- Create: `src-tauri/src/pricing.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/pricing.rs`

- [ ] **Step 1: Write failing Rust tests**
- [ ] **Step 2: Run targeted Cargo test to verify failures**
- [ ] **Step 3: Add minimal pricing catalog and alias normalization**
- [ ] **Step 4: Re-run targeted Cargo test to verify passes**

### Task 2: Preserve token breakdown and model data in usage events

**Files:**
- Modify: `src-tauri/src/connectors/mod.rs`
- Modify: `src-tauri/src/connectors/codex.rs`
- Modify: `src-tauri/src/connectors/claude_code.rs`
- Modify: `src-tauri/src/connectors/cherry_studio.rs`
- Test: `src-tauri/src/connectors/codex.rs`

- [ ] **Step 1: Extend shared usage event shape with model and token buckets**
- [ ] **Step 2: Populate new fields in Codex, Claude Code, and Cherry Studio connectors**
- [ ] **Step 3: Leave Cursor and Antigravity unpriced by design**
- [ ] **Step 4: Run targeted Cargo tests**

### Task 3: Compute estimated cost in snapshot assembly

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/models.rs`
- Test: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add snapshot-level cost calculations for daily history, source usage, sessions, and totals**
- [ ] **Step 2: Preserve existing calculation-method semantics**
- [ ] **Step 3: Run targeted Cargo tests**

### Task 4: Surface cost estimates in the existing UI

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/data/schema.ts`
- Modify: `src/i18n.ts`
- Test: `scripts/product-shell.test.mjs`

- [ ] **Step 1: Expose cost labels in source rows and recent sessions**
- [ ] **Step 2: Distinguish estimated pricing from pending pricing in copy**
- [ ] **Step 3: Run `pnpm test`**

### Task 5: Verify end-to-end

**Files:**
- Modify: `package.json` (only if a new test command is needed)

- [ ] **Step 1: Run `cargo test --manifest-path src-tauri/Cargo.toml`**
- [ ] **Step 2: Run `pnpm test`**
- [ ] **Step 3: Run `pnpm build`**
