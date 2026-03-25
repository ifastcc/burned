use serde::Serialize;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CalculationMethod {
    Native,
    Derived,
    Estimated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceState {
    Ready,
    Partial,
    Missing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalyticsState {
    Ready,
    SessionOnly,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PricingCoverage {
    Actual,
    Partial,
    Pending,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyUsagePoint {
    pub date: String,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub exact_share: f64,
    pub active_sources: u16,
    pub session_count: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeakUsagePoint {
    pub date: String,
    pub total_tokens: u64,
    pub total_cost_usd: Option<f64>,
    pub session_count: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowDelta {
    pub tokens_delta: i64,
    pub tokens_percent_change: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindowSummary {
    pub tokens: u64,
    pub cost_usd: Option<f64>,
    pub sessions: u32,
    pub priced_sessions: u32,
    pub pending_pricing_sessions: u32,
    pub active_days: u32,
    pub avg_per_active_day: f64,
    pub exact_share: f64,
    pub pricing_coverage: PricingCoverage,
    pub peak_day: Option<PeakUsagePoint>,
    pub delta_vs_previous_period: Option<WindowDelta>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodicBreakdownRow {
    pub label: String,
    pub start_date: String,
    pub end_date: String,
    pub tokens: u64,
    pub cost_usd: Option<f64>,
    pub sessions: u32,
    pub priced_sessions: u32,
    pub pending_pricing_sessions: u32,
    pub active_days: u32,
    pub pricing_coverage: PricingCoverage,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodicBreakdowns {
    pub weekly: Vec<PeriodicBreakdownRow>,
    pub monthly: Vec<PeriodicBreakdownRow>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceUsage {
    pub source_id: String,
    pub source: String,
    pub analytics_state: AnalyticsState,
    pub tokens: Option<u64>,
    pub cost_usd: Option<f64>,
    pub sessions: Option<u32>,
    pub trend: Option<String>,
    pub pricing_coverage: Option<PricingCoverage>,
    pub calculation_mix: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub source_id: String,
    pub title: String,
    pub preview: String,
    pub source: String,
    pub workspace: String,
    pub model: String,
    pub started_at: String,
    pub total_tokens: u64,
    pub cost_usd: f64,
    pub pricing_coverage: Option<PricingCoverage>,
    pub calculation_method: CalculationMethod,
    pub status: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    pub id: String,
    pub name: String,
    pub state: SourceState,
    pub capabilities: Vec<String>,
    pub note: String,
    pub local_path: Option<String>,
    pub session_count: Option<u32>,
    pub last_seen_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionGroup {
    pub source_id: String,
    pub source_name: String,
    pub source_state: SourceState,
    pub sessions: Vec<SessionSummary>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSnapshot {
    pub headline_date: String,
    pub total_tokens_today: u64,
    pub total_cost_today: f64,
    pub exact_share: f64,
    pub connected_sources: u16,
    pub active_sources: u16,
    pub burn_rate_per_hour: u64,
    pub week: Vec<DailyUsagePoint>,
    pub daily_history: Vec<DailyUsagePoint>,
    pub sources: Vec<SourceUsage>,
    pub sessions: Vec<SessionSummary>,
    pub session_groups: Vec<SessionGroup>,
    pub source_statuses: Vec<SourceStatus>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDetailSnapshot {
    pub source_id: String,
    pub source_name: String,
    pub status: SourceStatus,
    pub analytics_state: AnalyticsState,
    pub calculation_mix: String,
    pub today_summary: Option<UsageWindowSummary>,
    pub last7d_summary: Option<UsageWindowSummary>,
    pub last30d_summary: Option<UsageWindowSummary>,
    pub lifetime_summary: Option<UsageWindowSummary>,
    pub periodic_breakdowns: Option<PeriodicBreakdowns>,
    pub week: Vec<DailyUsagePoint>,
    pub daily_history: Vec<DailyUsagePoint>,
    pub sessions: Vec<SessionSummary>,
}
