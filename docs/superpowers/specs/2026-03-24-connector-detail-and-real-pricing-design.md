# Connector Detail And Real Pricing Design

## Goal

Extend Burned so connector detail pages become the primary place to inspect per-connector usage, history, and billing state. Implement real local cost tracking for Cursor when the source exposes trustworthy session pricing, while keeping Antigravity on a real-first policy: no synthetic session cost, with optional credits or quota shown only as a separate state if the source exposes it reliably.

## Product Decisions

### Decision 1: Real cost takes priority over inferred cost

- Cursor cost is only shown when local data includes explicit pricing information.
- Antigravity does not get a fabricated `costUsd` value from model guesses, subscription math, or quota heuristics.
- When pricing cannot be reconstructed faithfully, the UI shows `pricing pending`.
- Credits or quota, if available later, are a separate billing signal and must never be merged into `costUsd`.

### Decision 2: Connector detail is the deep-analysis surface

- The homepage remains a lightweight overview.
- The per-connector page is where totals, time-series history, periodic summaries, and session-level inspection live.
- Both source rows and connector cards route to the same detail page so there is one analysis surface per connector.

### Decision 3: Mockups are disposable

- Visual companion output may be used during design alignment only.
- No mockup HTML, demo assets, or temporary browser files should be committed as part of implementation.

## In Scope

- Make both the source usage rows and connector status cards open the connector detail page.
- Expand the source detail snapshot so it includes explicit summary windows instead of only raw daily arrays.
- Add richer historical and periodic statistics to the detail page.
- Parse real local Cursor pricing from `composerData.usageData` and aggregate it into session, daily, and source totals.
- Preserve Antigravity statistics and drilldown improvements even if real session cost remains unavailable.
- Show a separate billing state for Antigravity only if credits or quota can be read from a stable local source.

## Out Of Scope

- No session-level second page or nested drilldown beyond the connector detail page.
- No guessed Antigravity `costUsd` derived from models, plans, or usage proportions.
- No large charting dependency or visually heavy analytics rewrite.
- No redesign of the homepage into a full analytics workspace.
- No committed mockup artifacts.

## User Experience

### Entry Points

- Clicking a source row in the homepage usage section opens `/sources/:sourceId`.
- Clicking a connector card in the connected sources section opens the same route.
- Missing or partial connectors still open a detail page, but the page explains what is available and what is not.

### Connector Detail Information Hierarchy

The page answers four questions in order:

1. What is the connector doing overall right now?
2. How has usage changed across the last week and month?
3. Does the connector show periodic behavior across recent weeks or months?
4. Which sessions are driving the usage?

### Detail Page Layout

- Header: connector name, source state, pricing coverage, note, and capabilities.
- Summary cards: `Today`, `7D`, `30D`, `Lifetime`.
- Charts: existing 7-day and 30-day charts stay, but their inspectors become richer.
- Periodic breakdown: a compact table for weekly and monthly rollups.
- Sessions: recent sessions list with lightweight sorting options.

## Data Model

### New Summary Unit

Add a reusable summary structure for connector detail windows. Each summary should include:

- `tokens`
- `costUsd`
- `sessions`
- `pricedSessions`
- `pendingPricingSessions`
- `activeDays`
- `avgPerActiveDay`
- `exactShare`
- `peakDay`
- `pricingCoverage`
- `deltaVsPreviousPeriod`

`peakDay` should expose the day with the highest token count inside that window, along with the token and cost values for that day.

`pricingCoverage` should use a small fixed set of states:

- `actual`: every session in the window has real pricing data
- `partial`: some sessions are priced and some remain pending
- `pending`: no session in the window has real pricing data

`deltaVsPreviousPeriod` should compare the window against the immediately preceding window of the same length. Example:

- `last7d` compares against the prior 7 days
- `last30d` compares against the prior 30 days

The delta should include token delta and percent change. Cost and session delta can be added as parallel fields if the model remains readable.

### New Periodic Breakdown Unit

Add a connector detail breakdown list for normalized periods. The first implementation should support:

- `weekly`: recent eight weeks
- `monthly`: recent six months

Each row should include:

- period label
- start and end date
- tokens
- costUsd
- sessions
- pricedSessions
- pendingPricingSessions
- pricingCoverage
- activeDays

### Source Detail Snapshot Additions

Extend `SourceDetailSnapshot` with:

- `todaySummary`
- `last7dSummary`
- `last30dSummary`
- `lifetimeSummary`
- `periodicBreakdowns`

Keep the existing `week`, `dailyHistory`, `sessions`, `status`, and `calculationMix` fields because the current UI already depends on them.

### Billing State

If a connector exposes non-USD billing data such as credits or quota, add a dedicated billing-state object rather than overloading `costUsd`. That object should remain optional and source-specific in presentation, even if the transport shape is generic.

The billing-state object should use this transport shape:

- `kind`: `credits` | `quota`
- `state`: `ready` | `partial` | `unavailable`
- `current`: number | null
- `limit`: number | null
- `unit`: string | null
- `updatedAt`: string | null
- `note`: string | null

## Connector-Specific Behavior

### Cursor

Cursor already exposes local composer records in `state.vscdb`. Some records include a `usageData` object keyed by model name, where each entry contains explicit pricing values such as `costInCents`.

Cursor pricing behavior should be:

- Parse `usageData` from composer records.
- Sum `costInCents` across model entries to derive real session cost.
- Attribute that cost to the session and the session day.
- Aggregate real cost into daily history, summary cards, periodic breakdowns, and recent sessions.
- Keep sessions without usable `usageData` as `pricing pending`.

Cursor parsing fallback rules should stay conservative:

- Only numeric, non-negative `costInCents` values are considered valid.
- If `usageData` exists but every entry is missing or malformed, the session remains `pricing pending`.
- If `usageData` is only partially parseable across model entries, the whole session remains `pricing pending` rather than showing a partial USD total.

Cursor does not need token-based price estimation when `usageData` is present, because the source already provides a stronger billing signal.

### Antigravity

Antigravity currently appears to expose rich local state, model preferences, trajectory summaries, and possible credit or quota metadata, but not a confirmed per-session USD cost equivalent.

Antigravity behavior should be:

- Improve session indexing and usage detail presentation.
- Preserve `costUsd = 0` and render `pricing pending` when real session pricing is not recoverable.
- If a stable credits or quota source is confirmed, show it in a separate billing card or status strip.
- Never backfill `costUsd` from credits, plan tiers, or model selection alone.

## Aggregation Rules

### Daily History

- Source detail should expose longer history than the current 30-day cap.
- The backend should provide enough history to support the detail page summaries and periodic breakdowns without extra client recomputation.
- A 180-day daily history window is a reasonable default for source detail because it supports monthly summaries and lifetime-lite inspection without forcing the UI to process unbounded raw history.
- `dailyHistory` is a bounded UI-oriented slice only. It is not the authority for lifetime totals.

### Periodic Boundary Rules

- Weekly breakdowns should be calendar-aligned local weeks with Monday as the start of week.
- Monthly breakdowns should be calendar-aligned local months.
- The newest row may be a partial in-progress period and should still be included.
- Labels must reflect partial periods clearly, for example `This week`, `This month`, or date-ranged labels when needed.
- All day, week, and month boundaries should use the local system time zone already used by snapshot aggregation.
- DST transitions should follow local calendar boundaries, not fixed 24-hour or 168-hour offsets.

### Lifetime Summary

Lifetime is a cumulative total across all available usage events for the connector. It should include:

- total tokens
- total cost
- total sessions
- active day count
- average tokens per active day
- lifetime exact share
- peak day across the full available range

Lifetime must be computed from the full raw usage-event set collected for the snapshot, not from the bounded `dailyHistory` window. This keeps lifetime totals stable even when the chart only renders the most recent 180 days.

### Exact Share

Exact share remains the ratio of native tokens to total tokens inside the relevant window. It should be computed per summary window, not only per day.

## UI Behavior

### Summary Cards

Each summary card should show:

- primary values: tokens, cost, sessions
- secondary values: active days, avg per active day, exact share
- optional delta row for rolling windows

When `pricingCoverage` is `partial`, the card should show the known real subtotal while also indicating that pricing is incomplete for part of the window. The UI must not present the subtotal as if it were full coverage.

Pricing-state rendering should follow one rule at every level:

- session level: each session is either actual or pending
- day and summary-window level: `actual` shows full cost, `partial` shows the known subtotal plus a partial-coverage indicator, `pending` shows `pricing pending`

Cards should reuse the existing visual language rather than introducing a separate dashboard aesthetic.

### Trend Inspectors

The chart inspector should show:

- day label
- token total
- cost state
- session count
- exact share

This keeps the daily charts useful even when the user does not open the periodic table.

### Periodic Breakdown

The first implementation should be a table, not a second chart system. That keeps the page legible and avoids bringing in a heavier charting dependency.

The table can later evolve into toggleable weekly or monthly views, but the first version should not require multiple new interaction models.

### Sessions

Recent sessions stay on the detail page, but the list should support at least two orderings:

- newest first
- largest usage first

`largest usage first` should sort by `totalTokens` descending, with `updatedAt` descending as the tie-breaker. It should not switch to cost-based ordering, because priced and unpriced sessions must remain comparable under one rule.

This preserves the current quick-scan behavior while adding a direct way to inspect the heaviest sessions without making the ordering dependent on pricing coverage.

## Error Handling And Empty States

- Missing connector: show connector state, available local path, and an empty analytics view.
- Partial connector: show available statistics and clearly label missing pricing.
- No daily usage but indexed sessions: show sessions and metadata even if charts are empty.
- Pricing unavailable: use `pricing pending`, not `$0.00`, unless the real total is explicitly zero.
- Mixed pricing coverage: show the known actual subtotal plus a partial-coverage indicator rather than falling back to either full actual or full pending.

## Implementation Constraints

- Reuse the existing Tauri snapshot path and React route structure.
- Prefer additive changes to the snapshot contracts rather than replacing current fields.
- Keep the homepage payload lightweight.
- Avoid introducing new persisted storage or migrations for this feature.
- Do not commit design-time mockup artifacts.

## Testing Expectations

Planning and implementation should cover:

- Rust aggregation tests for summary windows, lifetime totals, and periodic breakdowns.
- Cursor connector tests for parsing `usageData.costInCents`.
- Shell tests for connector-card navigation and the new detail-page markers.
- UI tests or shell assertions that price states remain distinct: actual cost, pricing pending, optional credits or quota.

## Open Questions Resolved In This Spec

- Pricing policy: real-first, with no synthetic Antigravity USD cost.
- Connector detail priority: summary-first, then trends, then periodic breakdowns, then sessions.
- Entry points: both usage rows and connector cards lead to the same detail page.
- Mockups: allowed during design only, never committed.

## Planning Readiness

This work is a single coherent feature and should produce one implementation plan. The plan should split work into:

- snapshot and aggregation expansion
- Cursor pricing extraction
- detail-page UI expansion
- navigation and copy updates
- verification
