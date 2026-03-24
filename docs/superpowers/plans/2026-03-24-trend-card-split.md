# Trend Card Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split the homepage trend area into two independent cards: a primary last-7-days card and a secondary 30-day context card.

**Architecture:** Replace the current mixed `TrendPanel` with two focused components in `src/App.tsx`. Keep the existing fire-themed visual language, but give each time window a single responsibility and independent layout. Reuse the existing snapshot fields rather than changing backend contracts.

**Tech Stack:** React 19, TypeScript, Vite, plain CSS, Node test runner

---

### Task 1: Lock the new layout contract with a failing shell test

**Files:**
- Modify: `/Users/kbaicai/Documents/codex_workspace/product/burned/scripts/product-shell.test.mjs`
- Test: `/Users/kbaicai/Documents/codex_workspace/product/burned/scripts/product-shell.test.mjs`

- [ ] **Step 1: Write the failing test**
- [ ] **Step 2: Run `pnpm test` and confirm the new trend-layout assertions fail**
- [ ] **Step 3: Keep the rest of the suite green**

### Task 2: Replace the mixed trend panel in `src/App.tsx`

**Files:**
- Modify: `/Users/kbaicai/Documents/codex_workspace/product/burned/src/App.tsx`
- Modify: `/Users/kbaicai/Documents/codex_workspace/product/burned/src/showcase-copy.mjs`

- [ ] **Step 1: Remove the current `TrendPanel` component**
- [ ] **Step 2: Add a primary 7-day card component with its own headline and chart**
- [ ] **Step 3: Add a secondary 30-day card component with sparkline and summary stats**
- [ ] **Step 4: Render the two cards as separate sections in the homepage flow**

### Task 3: Rework CSS so the two cards feel intentionally different

**Files:**
- Modify: `/Users/kbaicai/Documents/codex_workspace/product/burned/src/styles.css`

- [ ] **Step 1: Replace the shared mixed-panel styles with separate weekly and monthly card styles**
- [ ] **Step 2: Increase hierarchy for the 7-day card and reduce density in the 30-day card**
- [ ] **Step 3: Keep mobile behavior clean by stacking the cards without losing emphasis**

### Task 4: Verify and stabilize

**Files:**
- Verify: `/Users/kbaicai/Documents/codex_workspace/product/burned/src/App.tsx`
- Verify: `/Users/kbaicai/Documents/codex_workspace/product/burned/src/styles.css`
- Verify: `/Users/kbaicai/Documents/codex_workspace/product/burned/scripts/product-shell.test.mjs`

- [ ] **Step 1: Run `pnpm test`**
- [ ] **Step 2: Run `pnpm build`**
- [ ] **Step 3: Inspect the diff for accidental unrelated edits**
