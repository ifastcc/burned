# Burned Home, Localization, And Connector Analytics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the duplicate homepage connector surface, add mainstream-locale shell localization with system-language detection and manual override, and make connector detail analytics truthful for Codex, Cursor, and Antigravity.

**Architecture:** First lock the product contract with failing shell and Rust snapshot tests so `ready`, `session_only`, and `unavailable` cannot silently collapse into zero-valued analytics. Then extend the shared Rust/TypeScript snapshot model, update the React shell to render stateful homepage/detail/i18n behavior, and repair connector ingestion conservatively: Codex stays the reference path, Cursor only exposes totals when local artifacts justify them, and Antigravity never encodes unknown pricing as `$0.00`.

**Tech Stack:** Rust + Tauri snapshot layer, React 19 + TypeScript + Vite frontend, Node test runner, Cargo tests.

---

## File Structure

**Frontend shell and copy**
- Modify: `src/App.tsx`
  Purpose: remove the passive connector grid, render state-aware source rows, add scalable locale switching, and show ready/pending/unavailable connector detail states.
- Modify: `src/components/TodaySources.tsx`
  Purpose: keep secondary source usage surfaces type-safe when `SourceUsage` becomes nullable-by-state.
- Modify: `src/components/UsageBars.tsx`
  Purpose: keep legacy compiled source-usage views type-safe under the new row contract or retire dead assumptions cleanly.
- Modify: `src/i18n.ts`
  Purpose: expand locale support, formalize locale resolution precedence, and expose locale metadata/helpers.
- Modify: `src/showcase-copy.mjs`
  Purpose: add copy for the new locales and the new analytics-state messaging.
- Modify: `src/styles.css`
  Purpose: style the expanded locale switcher and the new homepage/detail pending and unavailable states.

**Shared contracts**
- Modify: `src/data/schema.ts`
  Purpose: mirror the expanded snapshot contract in TypeScript, including nullable row metrics, analytics state, row-level pricing coverage, and nullable detail summaries.
- Modify: `src-tauri/src/models.rs`
  Purpose: define shared Rust transport structs for analytics state, nullable row metrics, row-level pricing coverage, summary windows, and periodic breakdowns.
- Modify: `src-tauri/src/lib.rs`
  Purpose: compute the new row/detail states, preserve `null` versus zero, and aggregate detail analytics consistently across connectors.

**Connector-specific ingestion**
- Modify: `src-tauri/src/connectors/cursor.rs`
  Purpose: convert trustworthy local Cursor usage and pricing artifacts into usage events or explicitly remain `session_only`.
- Modify: `src-tauri/src/connectors/antigravity.rs`
  Purpose: tighten session discovery, attempt trustworthy usage extraction from confirmed local artifacts, and emit explicit `session_only` state when day-level analytics are not recoverable.
- Modify: `src-tauri/src/connectors/codex.rs`
  Purpose: keep Codex as the reference ready connector by tightening event/session parity under the new contract.

**Integration ownership**
- `src-tauri/src/lib.rs` stays under controller ownership during execution so connector workers can run in parallel without shared-write conflicts.
- `src/App.tsx` stays under controller ownership for the same reason.

**Tests**
- Modify: `scripts/product-shell.test.mjs`
  Purpose: lock in homepage surface removal, locale-scale affordances, and detail pending/unavailable UI markers.
- Modify: `src-tauri/src/lib.rs`
  Purpose: add snapshot aggregation tests for `ready`, `session_only`, and `unavailable`, plus `null` versus zero semantics.
- Modify: `src-tauri/src/connectors/cursor.rs`
  Purpose: add parser and fallback tests for Cursor usage/pricing extraction.
- Modify: `src-tauri/src/connectors/antigravity.rs`
  Purpose: add session-discovery and analytics-state fallback tests.
- Modify: `src-tauri/src/connectors/codex.rs`
  Purpose: add regression tests that keep Codex on the `ready` path.

## Task 1: Lock The Product Contract With Failing Tests

**Files:**
- Modify: `scripts/product-shell.test.mjs`
- Modify: `src/data/schema.ts`
- Modify: `src-tauri/src/models.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `scripts/product-shell.test.mjs`
- Test: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add failing shell assertions for the homepage and locale changes**

Add or update assertions for:
- the passive homepage connector grid is gone
- source rows still navigate into `/sources/:sourceId`
- the top bar no longer hardcodes a two-button locale toggle
- the supported locale registry includes `en-US`, `zh-CN`, `ja-JP`, `ko-KR`, `de-DE`, `fr-FR`, and `es-ES`
- source-detail UI includes analytics-state copy for pending and unavailable connectors

Suggested assertions:

```js
test("homepage removes the passive connector grid", () => {
  assert.doesNotMatch(appSource, /function ConnectorGrid\(/);
  assert.doesNotMatch(appSource, /SectionHeader label=\{sc\.connected\}/);
});

test("supported locales are no longer a hardcoded two-option switch", () => {
  assert.match(appSource, /burned\.locale/);
  assert.match(appSource, /ja-JP/);
  assert.match(appSource, /fr-FR/);
  assert.doesNotMatch(appSource, /copy\.app\.locale\.english/);
});

test("source detail renders analytics pending and unavailable states", () => {
  assert.match(appSource, /analyticsState/);
  assert.match(appSource, /session_only/);
  assert.match(appSource, /unavailable/);
});
```

- [ ] **Step 2: Run the shell test to verify it fails**

Run: `node --test scripts/product-shell.test.mjs`
Expected: FAIL because the current homepage still renders the connector grid and the locale affordance is still two-value.

- [ ] **Step 3: Add failing Rust tests for row/detail analytics state and null-versus-zero behavior**

Add tests in `src-tauri/src/lib.rs` for:
- homepage source rows expose `ready`, `session_only`, and `unavailable` correctly
- `session_only` and `unavailable` rows keep `tokens`, `costUsd`, `sessions`, `trend`, and `pricingCoverage` nullable instead of zero-filled
- detail snapshots use `null` summaries when analytics are pending or unavailable
- ready snapshots preserve real zero-valued summaries when analytics exist but activity is zero
- row-level `pricingCoverage` distinguishes `actual`, `partial`, and `pending`

Suggested test names:

```rust
#[test]
fn source_rows_distinguish_ready_session_only_and_unavailable() {}

#[test]
fn session_only_rows_keep_quantitative_metrics_null() {}

#[test]
fn source_detail_uses_null_summaries_when_analytics_are_pending() {}

#[test]
fn source_detail_preserves_zero_for_ready_but_idle_windows() {}

#[test]
fn source_rows_expose_row_level_pricing_coverage() {}
```

- [ ] **Step 4: Run targeted Cargo tests to verify failure**

Run:
- `cargo test --manifest-path src-tauri/Cargo.toml source_rows_distinguish_ready_session_only_and_unavailable`
- `cargo test --manifest-path src-tauri/Cargo.toml session_only_rows_keep_quantitative_metrics_null`
- `cargo test --manifest-path src-tauri/Cargo.toml source_detail_uses_null_summaries_when_analytics_are_pending`
- `cargo test --manifest-path src-tauri/Cargo.toml source_detail_preserves_zero_for_ready_but_idle_windows`
- `cargo test --manifest-path src-tauri/Cargo.toml source_rows_expose_row_level_pricing_coverage`

Expected: FAIL because the shared models and aggregation helpers do not support these states yet.

- [ ] **Step 5: Commit the failing-test baseline**

```bash
git add scripts/product-shell.test.mjs src/data/schema.ts src-tauri/src/models.rs src-tauri/src/lib.rs
git commit -m "test: lock burned analytics state contract"
```

## Task 2: Implement The Shared Snapshot Contract And Aggregation Rules

**Files:**
- Modify: `src/data/schema.ts`
- Modify: `src-tauri/src/models.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/lib.rs`

- [ ] **Step 1: Extend the shared Rust and TypeScript transport models**

Add the shared state types needed by the spec:
- `AnalyticsState = ready | session_only | unavailable`
- row-level `pricingCoverage`
- nullable `tokens`, `costUsd`, `sessions`, and `trend` on homepage source rows when analytics are not ready
- nullable `todaySummary`, `last7dSummary`, `last30dSummary`, `lifetimeSummary`
- nullable or empty periodic breakdowns
- explicit summary-window and periodic-breakdown types that carry `pricingCoverage`

Representative TypeScript target shape:

```ts
export type AnalyticsState = "ready" | "session_only" | "unavailable";
export type PricingCoverage = "actual" | "partial" | "pending";

export type SourceUsage = {
  sourceId: string;
  source: string;
  analyticsState: AnalyticsState;
  tokens: number | null;
  costUsd: number | null;
  sessions: number | null;
  trend: "up" | "flat" | "down" | null;
  pricingCoverage: PricingCoverage | null;
  calculationMix: CalculationMethod | "mixed";
};

export type UsageWindowSummary = {
  tokens: number;
  costUsd: number | null;
  sessions: number;
  pricedSessions: number;
  pendingPricingSessions: number;
  activeDays: number;
  avgPerActiveDay: number;
  exactShare: number;
  pricingCoverage: PricingCoverage;
};

export type PeriodicBreakdownRow = {
  label: string;
  startDate: string;
  endDate: string;
  tokens: number;
  costUsd: number | null;
  sessions: number;
  pricedSessions: number;
  pendingPricingSessions: number;
  activeDays: number;
  pricingCoverage: PricingCoverage;
};
```

- [ ] **Step 2: Implement shared aggregation helpers that preserve `null` versus zero**

Add helpers in `src-tauri/src/lib.rs` that:
- determine row/detail analytics state from the presence of meaningful sessions and usage events
- only produce numeric summary windows when analytics are actually ready
- keep ready-but-idle windows numeric and zero-valued
- preserve row-level pricing coverage without inferring it from `costUsd`
- preserve summary-window and periodic-row pricing coverage without inferring it from `costUsd`

Do not encode unknown totals as `0.0` simply to satisfy serialization.

- [ ] **Step 3: Make detail snapshots stateful**

Update `build_source_snapshot_from_reports` so:
- `ready` connectors get daily history, summary windows, periodic breakdowns, and session-level pricing coverage
- `session_only` connectors keep sessions and status but return `null` summaries and empty chart series
- `unavailable` connectors keep only the diagnostic surfaces that are actually meaningful

- [ ] **Step 4: Run the targeted Cargo tests until they pass**

Run the same targeted commands from Task 1 Step 4.
Expected: PASS.

- [ ] **Step 5: Commit the shared snapshot contract**

```bash
git add src/data/schema.ts src-tauri/src/models.rs src-tauri/src/lib.rs
git commit -m "feat: add burned analytics state contract"
```

## Task 3: Implement Homepage Cleanup, Locale Scaling, And Detail State Rendering

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/components/TodaySources.tsx`
- Modify: `src/components/UsageBars.tsx`
- Modify: `src/i18n.ts`
- Modify: `src/showcase-copy.mjs`
- Modify: `src/styles.css`
- Modify: `scripts/product-shell.test.mjs`
- Test: `scripts/product-shell.test.mjs`
- Test: `pnpm build`

- [ ] **Step 1: Make the shell tests fail on the current UI structure**

If Task 1 shell assertions are still too loose, tighten them now so they check for:
- removal of the homepage connector grid
- one source-row drilldown surface
- locale registry usage instead of a two-button locale toggle
- explicit pending/unavailable detail messaging

- [ ] **Step 2: Implement locale registry, resolution precedence, and metadata**

Update `src/i18n.ts` and `src/showcase-copy.mjs` so:
- the supported locale set is explicit and exported
- `detectInitialLocale()` resolves `burned.locale`, then exact system-locale match, then language-family fallback, then `en-US`
- every supported locale ships with full app-shell copy
- locale labels come from registry metadata instead of hardcoded `EN / 中文`

- [ ] **Step 3: Update the homepage shell**

In `src/App.tsx`:
- remove the passive connector grid entirely
- keep the source list as the sole detail entry surface
- render state-aware source rows:
  - ready rows show tokens, trend, and pricing state
  - session-only rows show pending copy with no fake numeric totals
  - unavailable rows show unavailable copy with no fake numeric totals

Audit every compiled `SourceUsage` consumer under `src/` and either:
- update it for nullable row metrics
- or remove dead assumptions if the component is no longer part of the current shell

- [ ] **Step 4: Update the source detail page**

In `src/App.tsx` and `src/styles.css`:
- render header analytics state explicitly
- show summary cards and periodic analytics only for `ready`
- render a pending callout plus session list for `session_only`
- render a stronger unavailable surface for `unavailable`

- [ ] **Step 5: Run frontend verification**

Run:
- `node --test scripts/product-shell.test.mjs`
- `pnpm build`

Expected: PASS.

- [ ] **Step 6: Commit the frontend shell changes**

```bash
git add src/App.tsx src/components/TodaySources.tsx src/components/UsageBars.tsx src/i18n.ts src/showcase-copy.mjs src/styles.css scripts/product-shell.test.mjs
git commit -m "feat: update burned home and locale shell"
```

## Task 4: Repair Cursor Connector Analytics Conservatively

**Files:**
- Modify: `src-tauri/src/connectors/cursor.rs`
- Test: `src-tauri/src/connectors/cursor.rs`

- [ ] **Step 1: Add failing Cursor parser tests**

Add tests in `src-tauri/src/connectors/cursor.rs` for:
- explicit local usage/pricing records produce usage events
- malformed or partial pricing data leaves the session pending instead of fabricating totals
- session-only Cursor reports map to `analyticsState = session_only`

Suggested test names:

```rust
#[test]
fn cursor_usage_data_produces_usage_events_when_explicit_values_exist() {}

#[test]
fn cursor_malformed_usage_data_keeps_pricing_pending() {}

#[test]
fn cursor_without_usage_events_remains_session_only() {}
```

- [ ] **Step 2: Run targeted Cargo tests to verify failure**

Run:
- `cargo test --manifest-path src-tauri/Cargo.toml cursor_usage_data_produces_usage_events_when_explicit_values_exist`
- `cargo test --manifest-path src-tauri/Cargo.toml cursor_malformed_usage_data_keeps_pricing_pending`
- `cargo test --manifest-path src-tauri/Cargo.toml cursor_without_usage_events_remains_session_only`

Expected: FAIL until the parser and report wiring are implemented.

- [ ] **Step 3: Parse trustworthy local Cursor artifacts into usage events**

Implement the minimum conservative path:
- read explicit local usage and pricing fields from Cursor artifacts only when the values are numeric and internally coherent
- convert those records into `UsageEvent`s tied to the right session/day
- keep sessions indexed even when usage-event extraction fails
- leave `costUsd` null and pricing pending when the local pricing signal is incomplete
- keep any shared-contract wiring inside the already-owned Task 2 `lib.rs` surface instead of editing shared aggregation code here

- [ ] **Step 4: Verify targeted Cursor tests and shared snapshot tests**

Run:
- the three targeted Cursor tests from Step 2
- `cargo test --manifest-path src-tauri/Cargo.toml source_rows_distinguish_ready_session_only_and_unavailable`

Expected: PASS.

- [ ] **Step 5: Commit the Cursor connector work**

```bash
git add src-tauri/src/connectors/cursor.rs
git commit -m "feat: add conservative cursor analytics"
```

## Task 5: Repair Antigravity Connector Analytics Without Fabricating Pricing

**Files:**
- Modify: `src-tauri/src/connectors/antigravity.rs`
- Test: `src-tauri/src/connectors/antigravity.rs`

- [ ] **Step 1: Add failing Antigravity tests**

Add tests for:
- session discovery remains live even when usage-event extraction is absent
- reports with sessions but no trustworthy usage events resolve to `session_only`
- unknown pricing keeps `costUsd` null and `pricingCoverage` pending instead of `0`

Suggested test names:

```rust
#[test]
fn antigravity_sessions_keep_connector_visible_without_usage_events() {}

#[test]
fn antigravity_without_trustworthy_usage_events_is_session_only() {}

#[test]
fn antigravity_unknown_pricing_never_flattens_to_zero() {}
```

- [ ] **Step 2: Run targeted Cargo tests to verify failure**

Run:
- `cargo test --manifest-path src-tauri/Cargo.toml antigravity_sessions_keep_connector_visible_without_usage_events`
- `cargo test --manifest-path src-tauri/Cargo.toml antigravity_without_trustworthy_usage_events_is_session_only`
- `cargo test --manifest-path src-tauri/Cargo.toml antigravity_unknown_pricing_never_flattens_to_zero`

Expected: FAIL until the connector emits the right state and pricing semantics.

- [ ] **Step 3: Tighten Antigravity report construction**

Implement the connector so it:
- keeps indexed sessions and diagnostics visible when only session artifacts are trustworthy
- attempts usage-event extraction only from confirmed local fields with stable timestamps and token counts
- remains `session_only` when day-level analytics are not reconstructable
- never coerces unknown pricing into numeric zero
- keeps shared snapshot wiring in Task 2 `lib.rs` so this task remains parallel-safe

- [ ] **Step 4: Run targeted Antigravity tests and the shared state tests**

Run:
- the three targeted Antigravity tests from Step 2
- `cargo test --manifest-path src-tauri/Cargo.toml source_detail_uses_null_summaries_when_analytics_are_pending`

Expected: PASS.

- [ ] **Step 5: Commit the Antigravity connector work**

```bash
git add src-tauri/src/connectors/antigravity.rs
git commit -m "feat: make antigravity analytics state explicit"
```

## Task 6: Keep Codex On The Reference `ready` Path

**Files:**
- Modify: `src-tauri/src/connectors/codex.rs`
- Test: `src-tauri/src/connectors/codex.rs`

- [ ] **Step 1: Add failing Codex regression tests**

Add tests that keep Codex from regressing under the new contract:
- ready Codex reports still generate numeric row metrics
- detail snapshots for Codex still produce non-null summary windows and daily history
- session/event joins preserve session-level cost and totals

Suggested test names:

```rust
#[test]
fn codex_reports_remain_ready_under_the_new_contract() {}

#[test]
fn codex_detail_summaries_stay_numeric_when_usage_exists() {}

#[test]
fn codex_session_costs_still_join_to_usage_events() {}
```

- [ ] **Step 2: Run targeted Cargo tests to verify failure**

Run:
- `cargo test --manifest-path src-tauri/Cargo.toml codex_reports_remain_ready_under_the_new_contract`
- `cargo test --manifest-path src-tauri/Cargo.toml codex_detail_summaries_stay_numeric_when_usage_exists`
- `cargo test --manifest-path src-tauri/Cargo.toml codex_session_costs_still_join_to_usage_events`

Expected: FAIL until the contract migration is reflected everywhere Codex depends on it.

- [ ] **Step 3: Update Codex report and aggregation parity**

Implement only the contract-alignment changes needed so Codex remains the reference `ready` connector:
- keep event/session linkage stable
- keep pricing coverage coherent with existing native pricing behavior
- ensure the new row/detail state helpers classify Codex as `ready` when usage events exist
- keep shared snapshot wiring in Task 2 `lib.rs` so this task stays isolated from the other connector workers

- [ ] **Step 4: Run targeted Codex tests and the shared snapshot suite**

Run:
- the three targeted Codex tests from Step 2
- `cargo test --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

- [ ] **Step 5: Commit the Codex regression hardening**

```bash
git add src-tauri/src/connectors/codex.rs
git commit -m "test: preserve codex analytics parity"
```

## Task 7: Final Verification And Release-Readiness Check

**Files:**
- Modify: any remaining touched files from Tasks 1-6
- Test: `scripts/product-shell.test.mjs`
- Test: `pnpm test`
- Test: `pnpm build`
- Test: `cargo test --manifest-path src-tauri/Cargo.toml`
- Test: `cargo check --manifest-path src-tauri/Cargo.toml`

- [ ] **Step 1: Run the complete frontend verification suite**

Run:
- `node --test scripts/product-shell.test.mjs`
- `pnpm test`
- `pnpm build`

Expected: PASS.

- [ ] **Step 2: Run the complete Rust verification suite**

Run:
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

- [ ] **Step 3: Spot-check the user-facing flows**

Verify manually in the app:
- homepage shows no passive connector grid
- locale switcher offers all supported locales
- system-locale detection works when no manual override is stored
- source rows still navigate into detail
- Codex detail renders ready analytics
- Cursor and Antigravity render either ready analytics or truthful `session_only` / `unavailable` states

- [ ] **Step 4: Commit any final integration fixes**

```bash
git add src/App.tsx src/i18n.ts src/showcase-copy.mjs src/styles.css src/data/schema.ts src-tauri/src/models.rs src-tauri/src/lib.rs src-tauri/src/connectors/cursor.rs src-tauri/src/connectors/antigravity.rs src-tauri/src/connectors/codex.rs scripts/product-shell.test.mjs
git commit -m "feat: ship burned analytics reliability update"
```
