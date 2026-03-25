# Burned Home, Localization, And Connector Analytics Reliability Design

## Goal

Reshape Burned so the homepage has one clear drilldown surface, the app supports mainstream locales with automatic system-language detection, and connector detail pages show trustworthy analytics instead of empty or misleading statistics.

This spec extends the earlier connector-detail work rather than replacing it. The pricing rules from [2026-03-24-connector-detail-and-real-pricing-design.md](./2026-03-24-connector-detail-and-real-pricing-design.md) still apply: real pricing takes priority over inferred pricing, Antigravity must not get fabricated USD totals, and incomplete pricing must be shown as incomplete rather than silently flattened to `$0.00`.

## Current Problems

### Problem 1: The homepage exposes duplicate connector surfaces

- The homepage already has a source list that acts as the practical entry point into connector detail.
- A second connector-card grid repeats the same entities lower on the page.
- That lower grid is not clickable and adds no new decision-making value, so it reads like dead UI.

### Problem 2: Localization is too narrow and too brittle

- The app currently treats locale as a hardcoded two-value choice: `en-US` or `zh-CN`.
- Initial locale detection is effectively `zh` versus default English.
- The current toggle shape does not scale to more than two languages.

### Problem 3: Detail analytics collapse because connector data is inconsistent

- The detail page aggregates from connector `usage_events`.
- Codex already provides a mostly usable event stream.
- Cursor and Antigravity mostly provide indexed sessions, but not enough structured usage events to build daily history and summary windows.
- The current aggregation path tends to collapse missing usage data into zero-valued windows, which is misleading because `unknown` and `zero` are not the same state.

## Product Decisions

### Decision 1: The homepage gets one connector drilldown surface

- The homepage keeps the source list as the single detail entry surface.
- The lower connector-card grid is removed entirely.
- Connector health remains useful, but it should live inside the connector detail surface rather than competing with navigation on the homepage.

### Decision 2: Burned localizes the shell, not the source-native content

- Burned translates application chrome, labels, empty states, buttons, section headers, summaries, and formatting.
- Source-native content such as session titles, previews, capability labels written by upstream tools, workspace names, and source notes remain source-native unless there is already a trustworthy local translation source.
- This preserves fidelity and avoids fake translation quality.

### Decision 3: Locale selection has explicit precedence

Locale selection follows this order:

1. User-selected locale stored locally
2. System locale detected at app startup
3. English fallback

Once the user manually selects a locale, Burned should honor that preference until the user changes it again.

### Decision 4: Analytics must distinguish `zero`, `pending`, and `unavailable`

- `zero` means Burned has trustworthy usage coverage for the relevant window and the actual value is zero.
- `pending` means Burned can see the source and maybe even its sessions, but daily usage aggregation is not yet available or not yet trustworthy enough.
- `unavailable` means Burned cannot provide the relevant data surface at all.

Unknown data must not be rendered as zero.

### Decision 5: Real-first pricing rules remain in force

- Codex can continue to show native or trusted derived pricing where already supported.
- Cursor only shows USD totals when explicit local pricing data is available.
- Antigravity remains `pricing pending` unless a stable local pricing or quota source is confirmed separately.
- Pricing completeness must be explicit at the session, day, and summary-window levels.

### Decision 6: This release is one user-facing milestone implemented through three coordinated workstreams

The work is intentionally decomposed into:

- homepage information architecture cleanup
- locale-system formalization
- connector analytics reliability for Codex, Cursor, and Antigravity

These workstreams should share one release goal but remain separable during implementation and testing.

## In Scope

- Remove the homepage connector-card grid.
- Keep the homepage source list as the sole connector detail entry surface.
- Expand localization from two hardcoded locales to a supported-locale registry.
- Support an initial mainstream locale set:
  - `en-US`
  - `zh-CN`
  - `ja-JP`
  - `ko-KR`
  - `de-DE`
  - `fr-FR`
  - `es-ES`
- Ship complete app-shell copy packs for every locale in that initial set. English fallback is only for unsupported detected locales, not for missing strings inside a supported locale.
- Detect the current computer locale at startup and map it through exact-match, language-family fallback, then English fallback.
- Preserve a manual locale override in local settings.
- Refactor UI copy and formatting so the locale system scales beyond two languages.
- Make connector detail analytics trustworthy for Codex, Cursor, and Antigravity.
- Extend the snapshot contract so the UI can tell the difference between `stats ready`, `stats pending`, and `stats unavailable`.
- Show per-connector summary windows, daily history, and periodic breakdowns only when the underlying analytics state supports them.
- Keep session-level browsing available even when daily analytics are still pending.

## Out Of Scope

- Machine-translating session titles, previews, or other source-authored text.
- Cloud-backed translation services or downloadable language packs.
- Supporting every locale the browser reports on day one.
- Fabricating Cursor or Antigravity cost from plan tiers, model guesses, or quota heuristics.
- Building a second-level drilldown beneath `/sources/:sourceId`.
- Reworking the homepage into a new analytics workspace.
- Large charting-library changes.

## User Experience

### Homepage

The homepage should answer:

1. How much burn is happening overall?
2. How has it changed recently?
3. Which connectors are driving it today?
4. Which recent sessions deserve inspection?

The homepage should no longer answer a second time which connectors merely exist on disk.

#### Homepage Entry Surface Rules

- The source list remains the only connector detail entry surface.
- Every non-missing connector should still be representable in that list, even if its usage analytics are still pending or unavailable.
- Rows with ready analytics show tokens, pricing state, and trend.
- Rows with session-only coverage remain clickable but should show an explicit analytics-pending state instead of pretending they had zero usage.
- Rows with unavailable analytics remain clickable for diagnosis but render a stronger data-unavailable state.

### Locale Experience

- On first launch, Burned uses the current computer locale if it is supported.
- If the computer locale is unsupported but shares a language family with a supported locale, Burned picks the family fallback.
  - Example: `fr-CA` falls back to `fr-FR`
  - Example: `es-MX` falls back to `es-ES`
- If there is no supported family fallback, Burned uses `en-US`.
- Supported locales in the initial set must ship with complete shell copy. Burned should not mix a supported locale with English fallback strings because that turns localization gaps into a production UI state.
- A user-facing locale switcher remains available.
- The persisted manual-override key remains `burned.locale` unless a broader settings migration is introduced later.
- Once a user chooses a locale manually, future launches should honor that override rather than re-detecting the system locale each time.

### Connector Detail Experience

The connector detail page answers four questions in order:

1. What can Burned reliably see for this connector right now?
2. What does usage look like today, across the last week, and across the last month?
3. What periodic patterns show up across recent weeks or months?
4. Which sessions are driving the connector?

If the connector only has session indexing but not daily usage aggregation yet, the page should still be useful:

- header and status remain available
- recent sessions remain available
- summary cards and charts switch to an explicit analytics-pending state
- the page explains that session indexing is live but day-level aggregation is still pending

## Data Model

### Overview

The snapshot contract should stop treating missing analytics as zero-filled analytics. The UI needs explicit state to render the right surface.

### Source Usage Row Contract

Extend the homepage source-row contract with an explicit analytics state and state-aware metrics:

- `analyticsState`: `ready` | `session_only` | `unavailable`
- `tokens`: `number | null`
- `costUsd`: `number | null`
- `sessions`: `number | null`
- `trend`: `up` | `flat` | `down` | `null`
- `pricingCoverage`: `actual` | `partial` | `pending` | `null`

Row behavior:

- `ready`: tokens, session count, trend, and row-level pricing state are meaningful; `pricingCoverage` is always non-null, and `costUsd` is only non-null when the pricing coverage is `actual` or `partial`
- `session_only`: row is still clickable, row copy indicates that session indexing is available while aggregated usage is pending, the quantitative fields are `null` rather than zero-filled, and `pricingCoverage` is `null` because no row-level pricing total is authoritative yet
- `unavailable`: row is still clickable if the connector is non-missing, it renders a stronger data-unavailable state, the quantitative fields are `null`, and `pricingCoverage` is `null`

Missing connectors should remain excluded from the homepage source list.

### Source Detail Analytics State

Add a dedicated detail analytics state to `SourceDetailSnapshot`:

- `analyticsState`: `ready` | `session_only` | `unavailable`

Semantics:

- `ready`: daily history and summary windows are safe to interpret
- `session_only`: at least one meaningful indexed-session surface is available, but aggregated daily usage is not yet ready
- `unavailable`: neither meaningful sessions nor usage analytics are currently available

### Summary Windows

Retain the connector-detail summary structure from the earlier connector-detail pricing design and use these windows:

- `todaySummary`
- `last7dSummary`
- `last30dSummary`
- `lifetimeSummary`

However, these summaries must become nullable:

- `UsageWindowSummary | null`

Rules:

- A `null` summary means analytics are not currently available for that window.
- A zero-valued summary means analytics are available and the true value is zero.
- Field semantics, pricing-coverage states, and period-comparison behavior remain aligned with the earlier connector-detail pricing design.

### Daily History

`dailyHistory` and other chart-oriented day arrays should become stateful rather than always zero-filled:

- When analytics are ready, Burned provides the actual windowed day series.
- When analytics are pending or unavailable, Burned provides an empty day series and lets the UI render a pending or unavailable state.

This avoids presenting thirty zero days when Burned actually has no trustworthy day-level data.

### Periodic Breakdowns

Keep the periodic breakdown design from the earlier connector-detail spec:

- weekly rows for recent eight weeks
- monthly rows for recent six months

But these breakdown sets should also be nullable or empty when analytics are not ready.

Calendar-boundary rules remain aligned with the earlier connector-detail pricing design:

- weekly periods are calendar-aligned local weeks with Monday as the start of week
- monthly periods are calendar-aligned local months
- the newest row may be an in-progress partial period

### Pricing Coverage

Keep the earlier pricing-coverage model:

- `actual`
- `partial`
- `pending`

This pricing state is separate from analytics availability:

- a connector can have `analyticsState = ready` and `pricingCoverage = pending`
- a connector can have `analyticsState = session_only`, in which case summary-window pricing is not rendered as authoritative because the window itself is unavailable
- row-level pricing follows the same rule: when row analytics are not ready, row pricing is not authoritative and `pricingCoverage` is `null`

## Connector-Specific Behavior

### Codex

Codex remains the strongest local connector and acts as the reference path for trustworthy analytics.

Expected outcome:

- recent sessions stay available
- day-level usage remains available
- summary windows, daily history, and periodic breakdowns are all ready
- session totals and aggregated totals stay internally consistent

Codex work is primarily about tightening contract alignment and ensuring the richer summary model stays correct.

### Cursor

Cursor currently exposes useful local session metadata but incomplete analytics aggregation.

Expected outcome:

- session browsing remains available immediately
- if explicit local usage or pricing data can be parsed reliably from Cursor’s local artifacts, Burned converts that into usage events and enables day-level summaries
- if only session metadata is available, Burned sets `analyticsState = session_only` and renders that explicitly
- Cursor must not fabricate USD totals when explicit pricing is absent or malformed

Cursor should prefer a conservative partial-read policy over a misleading full total.

### Antigravity

Antigravity currently exposes local artifacts and session discovery paths, but usage and pricing trustworthiness remain weaker.

Expected outcome:

- session indexing remains available
- Burned attempts to recover trustworthy usage-event data from confirmed local artifacts
- if daily analytics cannot be reconstructed faithfully, Antigravity remains `session_only`
- Antigravity pricing remains `pending` unless a separate stable local pricing or quota source is verified
- when Antigravity pricing is unknown, `costUsd` stays `null`; it must never be encoded as `0` just to satisfy a numeric field

Antigravity must not be shown as `$0.00` simply because pricing is unknown.

## UI Behavior

### Homepage Source Rows

- The homepage source list remains clickable for every non-missing connector.
- Ready connectors render their normal quantitative row, including pricing state derived from row-level `pricingCoverage`.
- Session-only connectors render a subdued pending state:
  - no misleading “zero burn” framing
  - no numeric totals, session counts, or trend glyphs
  - explicit copy that analytics are pending
  - still routes to the detail page
- Unavailable connectors render a stronger data-unavailable state, remain clickable for diagnosis, and show neither quantitative totals nor trend glyphs.

### Homepage Connector Grid

- Remove the connector grid completely.
- Do not replace it with another passive card strip.

### Locale Switcher

- Replace the current two-button switch with a scalable control.
- The control does not need to be visually heavy, but it must handle more than two locales without crowding the top bar.
- Locale labels should be sourced from the locale registry and shown as short native-language labels with enough clarity to distinguish similar options.

### Source Detail Header

The header should show:

- connector name
- source state
- analytics state
- pricing coverage or pricing policy summary
- last seen / note / capabilities as supporting context

This makes it obvious whether the user is looking at a fully aggregated connector or a session-only connector.

### Source Detail Body

When `analyticsState = ready`:

- render summary cards
- render daily and periodic analytics views
- render session list

When `analyticsState = session_only`:

- render an explicit pending callout in place of summary cards and charts
- keep the session list visible
- explain that Burned can read sessions but not yet day-level usage

When `analyticsState = unavailable`:

- render a stronger unavailable state
- keep any useful connector note and capabilities visible

## Aggregation Rules

### Unknown Must Not Collapse Into Zero

Aggregation helpers should only emit numeric analytics windows when the connector truly has usable usage coverage.

Examples:

- No usage-event feed yet: summary windows are `null`, not zero
- Real event feed but no activity in the last seven days: summary window is present with zero values

### Session Counts Versus Analytics Availability

Session availability and analytics availability are independent:

- a connector can have many indexed sessions and still have pending day-level analytics
- the UI must reflect both facts without contradiction

### Lifetime Totals

Lifetime remains a cumulative total across the full available event set, not across bounded chart windows. When analytics are not ready, lifetime is also unavailable rather than synthesized.

## Release Decomposition

### Workstream A: Homepage Information Architecture

- remove the passive connector grid
- keep one drilldown surface
- make session-only connectors legible in the source list

### Workstream B: Formal Locale System

- replace two-locale hardcoding with a locale registry
- add supported mainstream locales
- implement exact-match, language-family, then English fallback
- preserve user override
- update formatting and switcher UI accordingly

### Workstream C: Connector Analytics Reliability

- align the snapshot contract around explicit analytics availability
- make Codex the reference implementation for ready analytics
- repair Cursor and Antigravity so they either emit trustworthy usage analytics or clearly advertise session-only coverage
- ensure the detail page can render both cases honestly

These workstreams should be planned and executed in a way that allows parallel implementation with minimal file overlap where possible.

## Success Criteria

- The homepage no longer shows a second passive connector-card strip.
- Users can still reach connector detail from one clear surface on the homepage.
- On first launch, Burned follows the current computer locale when supported.
- Users can manually override the locale and keep that choice across launches.
- Burned can distinguish “real zero usage” from “stats not available yet.”
- Codex detail analytics remain trustworthy after the snapshot contract changes.
- Cursor and Antigravity detail pages no longer pretend to have full analytics when only sessions are available.
- Pricing remains conservative and never fabricates unsupported USD totals.

## Risks And Guardrails

- The largest product risk is confusing `pending` with `zero`; the contract must prevent this explicitly.
- The largest implementation risk is partial connector parsing that appears complete; connector-specific tests must assert conservative fallback behavior.
- The largest UX risk is replacing one redundant surface with a new ambiguous one; the homepage should stay lean.
