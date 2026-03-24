# Connector Detail And Real Pricing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add richer connector detail analytics, real local Cursor cost tracking, and source-card drilldown while keeping Antigravity on a real-first `pricing pending` policy.

**Architecture:** Extend the Rust snapshot layer with explicit pricing coverage, window summaries, lifetime totals, and periodic breakdowns; teach the Cursor connector to emit real session pricing from local `usageData`; then expand the existing `/sources/:sourceId` React view to render summary cards, richer inspectors, periodic tables, and session sorting without replacing the current app shell.

**Tech Stack:** Rust connector pipeline, Tauri snapshot assembly, React 19 + TypeScript, Vite, Node test runner, Cargo tests.

---

### File Structure

**Backend snapshot and connector files**
- Modify: `src-tauri/src/connectors/mod.rs`
  Purpose: add explicit pricing fields and helpers to `UsageEvent`.
- Modify: `src-tauri/src/connectors/cursor.rs`
  Purpose: parse `composerData.usageData`, attach real session cost, and emit cost-carrying usage events.
- Modify: `src-tauri/src/connectors/antigravity.rs`
  Purpose: tighten session discovery/session counts so Antigravity detail stats stay coherent even when pricing is pending.
- Modify: `src-tauri/src/models.rs`
  Purpose: define connector-detail summary structs, pricing coverage enums, periodic breakdown rows, and optional billing state.
- Modify: `src-tauri/src/lib.rs`
  Purpose: aggregate new summary windows, lifetime totals, periodic breakdowns, pricing coverage, and session sorting data.

**Frontend files**
- Modify: `src/data/schema.ts`
  Purpose: mirror the new snapshot contract in TypeScript.
- Modify: `src/App.tsx`
  Purpose: keep the existing route shell, add connector-card navigation, summary cards, richer inspectors, session sorting, and periodic breakdown rendering.
- Modify: `src/showcase-copy.mjs`
  Purpose: add detail-page copy for summary cards, periodic table labels, pricing coverage, and session sorting.
- Modify: `src/styles.css`
  Purpose: style the new detail-page summary strip, periodic breakdown table, clickable connector cards, and pricing coverage states.

**Tests**
- Modify: `scripts/product-shell.test.mjs`
  Purpose: lock in UI structure changes and navigation markers.
- Modify: `src-tauri/src/lib.rs`
  Purpose: add aggregation tests.
- Modify: `src-tauri/src/connectors/cursor.rs`
  Purpose: add Cursor pricing parser tests.
- Modify: `src-tauri/src/connectors/antigravity.rs`
  Purpose: add targeted session discovery/count tests if helper extraction is needed.

### Task 1: Lock the Snapshot and UI Contract with Failing Tests

**Files:**
- Modify: `scripts/product-shell.test.mjs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/models.rs`
- Modify: `src/data/schema.ts`
- Test: `scripts/product-shell.test.mjs`
- Test: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add failing shell assertions for the expanded connector detail page**

Add assertions for:
- clickable connector cards
- summary-card markers such as `Today`, `7D`, `30D`, `Lifetime`
- periodic breakdown/table markers
- session sorting control markers
- homepage and source-row states that keep Antigravity out of synthetic USD totals

Suggested assertions:

```js
test("connector detail page exposes summary cards and periodic breakdowns", () => {
  assert.match(appSource, /source-summary-card/);
  assert.match(appSource, /periodic-breakdown/);
  assert.match(appSource, /session-sort/);
});

test("connector cards route into the source detail page", () => {
  assert.match(appSource, /className="conn-card"/);
  assert.match(appSource, /onOpenSource/);
});

test("source rows keep routing into the source detail page", () => {
  assert.match(appSource, /SourceList/);
  assert.match(appSource, /navigate\\(\\`\\/sources\\//);
});

test("pricing states stay distinct across actual, pending, and non-USD billing", () => {
  assert.match(appSource, /pricing pending/);
  assert.match(appSource, /estimatedCost/);
  assert.doesNotMatch(appSource, /Antigravity.*\\$0\\.00/);
  assert.doesNotMatch(appSource, /antigravity.*estimatedCost/i);
});
```

- [ ] **Step 2: Run the shell test to verify it fails**

Run: `node --test scripts/product-shell.test.mjs`

Expected: FAIL on the new source-detail markers because the UI has not been expanded yet.

- [ ] **Step 3: Add failing Rust tests for summary windows, lifetime totals, and periodic breakdowns**

Add tests in `src-tauri/src/lib.rs` for:
- lifetime totals come from the full raw event set, not the bounded daily history
- source detail snapshot includes `todaySummary`, `last7dSummary`, `last30dSummary`, `lifetimeSummary`
- periodic breakdowns return weekly and monthly rows
- summary windows expose `deltaVsPreviousPeriod` with token delta and percent change for rolling windows
- periodic breakdowns clamp to recent eight weeks and recent six months when enough history exists
- mixed priced/unpriced windows produce partial pricing coverage and preserve the known subtotal rather than collapsing to zero or full pending

Suggested test names:

```rust
#[test]
fn source_detail_snapshot_includes_summary_windows_and_periodic_breakdowns() {}

#[test]
fn lifetime_summary_uses_full_event_history_not_bounded_daily_history() {}

#[test]
fn source_detail_pricing_coverage_is_partial_when_only_some_sessions_are_priced() {}

#[test]
fn source_detail_summary_windows_include_previous_period_deltas() {}

#[test]
fn source_detail_periodic_breakdowns_are_limited_to_recent_expected_periods() {}
```

- [ ] **Step 4: Run targeted Cargo tests to verify failure**

Run:
- `cargo test --manifest-path src-tauri/Cargo.toml source_detail_snapshot_includes_summary_windows_and_periodic_breakdowns`
- `cargo test --manifest-path src-tauri/Cargo.toml lifetime_summary_uses_full_event_history_not_bounded_daily_history`
- `cargo test --manifest-path src-tauri/Cargo.toml source_detail_pricing_coverage_is_partial_when_only_some_sessions_are_priced`
- `cargo test --manifest-path src-tauri/Cargo.toml source_detail_summary_windows_include_previous_period_deltas`
- `cargo test --manifest-path src-tauri/Cargo.toml source_detail_periodic_breakdowns_are_limited_to_recent_expected_periods`

Expected: FAIL because the models and aggregation helpers do not exist yet.

### Task 2: Add Pricing Coverage and Connector-Detail Snapshot Types

**Files:**
- Modify: `src-tauri/src/connectors/mod.rs`
- Modify: `src-tauri/src/models.rs`
- Modify: `src/data/schema.ts`
- Test: `src-tauri/src/lib.rs`

- [ ] **Step 1: Extend `UsageEvent` with explicit-cost support and pricing coverage helpers**

Add explicit pricing support in `src-tauri/src/connectors/mod.rs` so the aggregation layer can distinguish:
- explicit real cost from the connector
- estimated token-priced cost from existing connectors
- pending pricing
- source-specific pricing policy so real-first connectors can opt out of estimated USD fallback entirely

Target shape:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PricingCoverage {
    Actual,
    Partial,
    Pending,
}

#[derive(Clone)]
pub struct UsageEvent {
    pub source_id: &'static str,
    pub occurred_at: DateTime<Utc>,
    pub model: String,
    pub token_breakdown: TokenBreakdown,
    pub total_tokens: u64,
    pub calculation_method: CalculationMethod,
    pub session_id: String,
    pub explicit_cost_usd: Option<f64>,
}
```

Also add an explicit helper or flag such as `source_supports_estimated_cost(source_id)` so Cursor and Antigravity remain `pricing pending` unless they carry real connector-supplied pricing.

- [ ] **Step 2: Add new Rust snapshot structs for summary windows, peak-day data, periodic rows, and billing state**

Add serializable structs in `src-tauri/src/models.rs` for:
- `PricingCoverage`
- `PeakUsagePoint`
- `WindowDelta`
- `UsageWindowSummary`
- `PeriodicBreakdownRow`
- `PeriodicBreakdownSet`
- `BillingState`

Extend `DailyUsagePoint` and `SessionSummary` with the fields needed by the spec:
- `priced_sessions`
- `pending_pricing_sessions`
- `pricing_coverage`
- `pricing_state` on `SessionSummary`, using `actual | pending`

Extend `SourceDetailSnapshot` with:
- `today_summary`
- `last7d_summary`
- `last30d_summary`
- `lifetime_summary`
- `periodic_breakdowns`
- `billing_state`

For this iteration, keep `billing_state` unset unless implementation proves a stable decoded local source exists. Do not invent or decode Antigravity credits/quota on speculation just because the transport field exists.

- [ ] **Step 3: Mirror the same contract in `src/data/schema.ts`**

Add TS unions and types so the React layer can consume the new snapshot without `any` or ad hoc shape checks.

Suggested TS additions:

```ts
export type PricingCoverage = "actual" | "partial" | "pending";

export type WindowDelta = {
  tokensDelta: number;
  tokensPercentChange: number | null;
};

export type PeakUsagePoint = {
  date: string;
  totalTokens: number;
  totalCostUsd: number;
};

export type BillingState = {
  kind: "credits" | "quota";
  state: "ready" | "partial" | "unavailable";
  current: number | null;
  limit: number | null;
  unit: string | null;
  updatedAt: string | null;
  note: string | null;
};

export type UsageWindowSummary = {
  tokens: number;
  costUsd: number;
  sessions: number;
  pricedSessions: number;
  pendingPricingSessions: number;
  activeDays: number;
  avgPerActiveDay: number;
  exactShare: number;
  pricingCoverage: PricingCoverage;
  peakDay: PeakUsagePoint | null;
  deltaVsPreviousPeriod?: WindowDelta | null;
};

export type PeriodicBreakdownRow = {
  label: string;
  startDate: string;
  endDate: string;
  tokens: number;
  costUsd: number;
  sessions: number;
  pricedSessions: number;
  pendingPricingSessions: number;
  pricingCoverage: PricingCoverage;
  activeDays: number;
};

export type PeriodicBreakdownSet = {
  weekly: PeriodicBreakdownRow[];
  monthly: PeriodicBreakdownRow[];
};
```

Also expand `SourceDetailSnapshot` in `src/data/schema.ts` with:
- `todaySummary`
- `last7dSummary`
- `last30dSummary`
- `lifetimeSummary`
- `periodicBreakdowns`
- `billingState`

Keep the existing `week`, `dailyHistory`, `sessions`, `status`, and `calculationMix` fields intact in both Rust and TypeScript. This migration must be additive.

- [ ] **Step 4: Run the targeted Cargo tests again and keep them failing only on missing aggregation logic**

Run:
- `cargo test --manifest-path src-tauri/Cargo.toml source_detail_snapshot_includes_summary_windows_and_periodic_breakdowns`
- `cargo test --manifest-path src-tauri/Cargo.toml lifetime_summary_uses_full_event_history_not_bounded_daily_history`
- `cargo test --manifest-path src-tauri/Cargo.toml source_detail_pricing_coverage_is_partial_when_only_some_sessions_are_priced`
- `cargo test --manifest-path src-tauri/Cargo.toml source_detail_summary_windows_include_previous_period_deltas`
- `cargo test --manifest-path src-tauri/Cargo.toml source_detail_periodic_breakdowns_are_limited_to_recent_expected_periods`

Expected: still FAIL, but now due to unimplemented aggregation/builders rather than missing types.

### Task 3: Parse Real Cursor Pricing and Tighten Antigravity Session Discovery

**Files:**
- Modify: `src-tauri/src/connectors/cursor.rs`
- Modify: `src-tauri/src/connectors/antigravity.rs`
- Modify: `src-tauri/src/connectors/mod.rs`
- Test: `src-tauri/src/connectors/cursor.rs`
- Test: `src-tauri/src/connectors/antigravity.rs`

- [ ] **Step 1: Add failing Cursor connector tests for `usageData.costInCents` parsing**

Add tests for:
- a session with valid `usageData` entries produces a real USD total
- malformed or missing `costInCents` leaves the session pricing pending
- partially parseable `usageData` still resolves to pending instead of partial real cost
- trustworthy local Cursor token-count fields, when present, are preserved on the session instead of being replaced by pricing-only events

Suggested test names:

```rust
#[test]
fn parses_cursor_usage_data_into_real_session_cost() {}

#[test]
fn cursor_session_stays_pending_when_usage_data_costs_are_invalid() {}

#[test]
fn cursor_session_preserves_token_totals_when_token_count_is_present() {}
```

- [ ] **Step 2: Run the focused Cursor tests to verify failure**

Run:
- `cargo test --manifest-path src-tauri/Cargo.toml parses_cursor_usage_data_into_real_session_cost`
- `cargo test --manifest-path src-tauri/Cargo.toml cursor_session_stays_pending_when_usage_data_costs_are_invalid`
- `cargo test --manifest-path src-tauri/Cargo.toml cursor_session_preserves_token_totals_when_token_count_is_present`

Expected: FAIL because Cursor does not parse pricing yet.

- [ ] **Step 3: Implement Cursor session-pricing extraction and cost-carrying events**

Inside `src-tauri/src/connectors/cursor.rs`:
- parse `usageData` from each `composerData:*` record
- parse trustworthy local Cursor token-count fields when present, and keep those token totals on `SessionSummary.total_tokens`
- validate every `costInCents` entry before accepting the session as priced
- set `SessionSummary.cost_usd` when the session is fully priced
- emit one cost-carrying `UsageEvent` per session using the session update timestamp, the parsed token total if available, and `explicit_cost_usd: Some(real_cost)`

Use helper functions so the parsing logic is testable:

```rust
fn parse_usage_data_cost_usd(value: &Value) -> Option<f64> {}
fn parse_cursor_session_token_total(value: &Value) -> Option<u64> {}
fn build_cursor_pricing_event(...) -> UsageEvent {}
```

- [ ] **Step 4: Tighten Antigravity’s minimum deliverable to indexed sessions only**

Inside `src-tauri/src/connectors/antigravity.rs`:
- align `session_count` with the same `brain` directory the connector already indexes
- keep billing state absent and `cost_usd = 0.0`
- do not attempt credits/quota decoding in this implementation

If helper extraction is needed, add a small test around the session-count path selection to avoid regressions.

- [ ] **Step 5: Re-run the focused connector tests and verify pass**

Run:
- `cargo test --manifest-path src-tauri/Cargo.toml parses_cursor_usage_data_into_real_session_cost`
- `cargo test --manifest-path src-tauri/Cargo.toml cursor_session_stays_pending_when_usage_data_costs_are_invalid`
- `cargo test --manifest-path src-tauri/Cargo.toml cursor_session_preserves_token_totals_when_token_count_is_present`

Expected: PASS.

### Task 4: Build Summary Windows, Lifetime Totals, and Periodic Breakdowns in Rust

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/models.rs`
- Modify: `src-tauri/src/connectors/mod.rs`
- Test: `src-tauri/src/lib.rs`

- [ ] **Step 1: Implement canonical cost and coverage helpers in the snapshot layer**

Add helpers in `src-tauri/src/lib.rs` for:
- event cost selection: explicit cost first; only fall back to existing estimated token cost for connectors whose pricing policy explicitly allows estimates
- session-level pricing coverage
- window-level `pricedSessions`, `pendingPricingSessions`, and `pricingCoverage`

Suggested helpers:

```rust
fn event_cost_usd(event: &UsageEvent) -> Option<f64> {}
fn summarize_window(events: &[&UsageEvent], days: &[NaiveDate]) -> UsageWindowSummary {}
fn derive_pricing_coverage(priced: u32, pending: u32) -> PricingCoverage {}
```

Make the rule explicit in implementation notes:
- `cursor` and `antigravity` must return `None` from `event_cost_usd(...)` when `explicit_cost_usd` is absent
- existing estimated token pricing remains available only for connectors that already rely on model-price estimation

- [ ] **Step 2: Extend daily history and weekly history rows with pricing coverage counts**

Update `build_usage_window` so each `DailyUsagePoint` carries:
- `priced_sessions`
- `pending_pricing_sessions`
- `pricing_coverage`

This lets the frontend render partial windows without guessing from `costUsd > 0`.

- [ ] **Step 3: Build `today`, `7D`, `30D`, and `lifetime` summaries from the raw event set**

Implement summary builders that:
- use the local system timezone for day boundaries
- use the full raw event set for `lifetime`
- keep `dailyHistory` capped at 180 days for UI rendering
- compute `delta_vs_previous_period` for `today`, `last7d`, and `last30d` using the immediately preceding local-calendar window of the same length
- leave `lifetime.delta_vs_previous_period` unset
- preserve the existing `week`, `dailyHistory`, `sessions`, `status`, and `calculationMix` fields while adding the new summary blocks

- [ ] **Step 4: Build weekly and monthly periodic breakdown sets**

Implement calendar-aligned local aggregation:
- week starts Monday and returns the most recent eight weekly rows
- month uses local calendar month and returns the most recent six monthly rows
- newest row may be partial and stays visible
- partial current-week rows should surface a `This week` label
- partial current-month rows should surface a `This month` label
- completed periods should use deterministic date-ranged or month-name labels emitted from the backend

Add explicit label fields and row shapes the frontend can render directly.

- [ ] **Step 5: Re-run the source-detail aggregation tests and verify pass**

Run:
- `cargo test --manifest-path src-tauri/Cargo.toml source_detail_snapshot_includes_summary_windows_and_periodic_breakdowns`
- `cargo test --manifest-path src-tauri/Cargo.toml lifetime_summary_uses_full_event_history_not_bounded_daily_history`
- `cargo test --manifest-path src-tauri/Cargo.toml source_detail_pricing_coverage_is_partial_when_only_some_sessions_are_priced`
- `cargo test --manifest-path src-tauri/Cargo.toml source_detail_summary_windows_include_previous_period_deltas`
- `cargo test --manifest-path src-tauri/Cargo.toml source_detail_periodic_breakdowns_are_limited_to_recent_expected_periods`

Expected: PASS.

### Task 5: Expand the React Source Detail Page and Drilldown Navigation

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/data/schema.ts`
- Modify: `src/showcase-copy.mjs`
- Modify: `src/styles.css`
- Test: `scripts/product-shell.test.mjs`

- [ ] **Step 1: Make connector cards navigate into the existing source detail route**

Update `ConnectorGrid` in `src/App.tsx` so cards behave like buttons and call the same route transition as `SourceList`.

Do not change the existing `SourceList` entry point away from `/sources/:sourceId`; instead, preserve it and add shell assertions so both homepage drilldown paths are covered.

- [ ] **Step 2: Add source-detail summary cards and richer inspector content**

Render `todaySummary`, `last7dSummary`, `last30dSummary`, and `lifetimeSummary` above the charts.

Each card should show:
- tokens
- cost or `pricing pending`
- sessions
- active days
- average per active day
- exact share
- partial-coverage indicator when needed

Update `TrendInspector` to show:
- session count
- exact share
- partial pricing coverage when the day is mixed

- [ ] **Step 3: Add periodic breakdown rendering and session sorting**

Inside `SourceDetailPage`:
- render weekly/monthly breakdown tables
- add a `newest` / `largest usage` sort toggle
- sort `largest usage` by `totalTokens` descending, then `updatedAt` descending
- render partial current rows using backend-provided `This week` / `This month` labels, and completed rows using backend-provided deterministic period labels

- [ ] **Step 4: Update copy and styles without introducing mockup leftovers**

Add the new labels to `src/showcase-copy.mjs` and corresponding layout styles to `src/styles.css`.

Style targets:
- `.source-summary-grid`
- `.source-summary-card`
- `.pricing-coverage`
- `.periodic-breakdown`
- `.session-sort`
- button affordances for `.conn-card`

- [ ] **Step 5: Re-run the shell test and verify pass**

Run: `node --test scripts/product-shell.test.mjs`

Expected: PASS, including the new assertion that actual cost, `pricing pending`, and the absence of synthetic Antigravity USD totals remain distinct.

### Task 6: Run Full Verification

**Files:**
- Modify: `package.json` only if a new focused test command proves necessary

- [ ] **Step 1: Run the full Rust test suite**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

- [ ] **Step 2: Run the full Node test suite**

Run: `pnpm test`

Expected: PASS.

- [ ] **Step 3: Run a production frontend build**

Run: `pnpm build`

Expected: PASS with no new TypeScript or Vite errors.

- [ ] **Step 4: Smoke-check the app shell manually if needed**

Run: `pnpm dev` or `pnpm dev:app`

Verify:
- connector cards open the detail page
- Cursor shows real cost when local `usageData` is present
- Antigravity detail renders stats while cost remains pending
- no mockup artifacts or temporary files are introduced
