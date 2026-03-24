mod connectors;
mod models;
mod pricing;
mod settings;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{Datelike, Duration, Local, NaiveDate, Timelike};
use serde_json::Result as JsonResult;

use connectors::{
    collect_all, collect_all_with_progress, source_supports_estimated_cost, SourceReport,
    UsageEvent,
};
pub use models::DashboardSnapshot;
use models::{
    CalculationMethod, DailyUsagePoint, PeakUsagePoint, PeriodicBreakdownRow, PeriodicBreakdownSet,
    PricingCoverage, SessionGroup, SessionSummary, SourceDetailSnapshot, SourceStatus, SourceUsage,
    UsageWindowSummary, WindowDelta,
};
use pricing::estimate_cost_usd;
pub use settings::AppSettings;

#[tauri::command]
fn get_dashboard_snapshot() -> DashboardSnapshot {
    build_dashboard_snapshot()
}

#[tauri::command]
fn get_source_snapshot(source_id: String) -> Result<SourceDetailSnapshot, String> {
    build_source_snapshot(&source_id)
}

#[tauri::command]
fn get_app_settings() -> AppSettings {
    settings::load_app_settings().unwrap_or_default()
}

#[tauri::command]
fn set_cherry_backup_dir(path: String) -> Result<AppSettings, String> {
    settings::set_cherry_backup_dir(&path).map_err(|error| error.to_string())
}

#[tauri::command]
fn clear_cherry_backup_dir() -> Result<AppSettings, String> {
    settings::clear_cherry_backup_dir().map_err(|error| error.to_string())
}

pub fn build_dashboard_snapshot() -> DashboardSnapshot {
    let now = Local::now();
    let reports = collect_all();
    build_dashboard_snapshot_from_reports(reports, now)
}

pub fn build_source_snapshot(source_id: &str) -> Result<SourceDetailSnapshot, String> {
    let now = Local::now();
    let reports = collect_all();
    build_source_snapshot_from_reports(&reports, now, source_id)
        .ok_or_else(|| format!("Source `{source_id}` was not found"))
}

pub fn build_dashboard_snapshot_with_progress<F>(on_progress: F) -> DashboardSnapshot
where
    F: FnMut(usize, usize, &str),
{
    let now = Local::now();
    let reports = collect_all_with_progress(on_progress);
    build_dashboard_snapshot_from_reports(reports, now)
}

fn build_dashboard_snapshot_from_reports(
    reports: Vec<SourceReport>,
    now: chrono::DateTime<Local>,
) -> DashboardSnapshot {
    let source_statuses = reports
        .iter()
        .map(|report| report.status.clone())
        .collect::<Vec<SourceStatus>>();

    let connected_sources = source_statuses
        .iter()
        .filter(|status| !matches!(status.state, models::SourceState::Missing))
        .count() as u16;

    let mut source_names = HashMap::new();
    for status in &source_statuses {
        source_names.insert(status.id.clone(), status.name.clone());
    }

    let usage_events = reports
        .iter()
        .flat_map(|report| report.usage_events.iter())
        .collect::<Vec<_>>();

    let total_tokens_today = usage_events
        .iter()
        .filter(|event| event.occurred_at.with_timezone(&Local).date_naive() == now.date_naive())
        .map(|event| event.total_tokens)
        .sum::<u64>();
    let total_cost_today = usage_events
        .iter()
        .filter(|event| event.occurred_at.with_timezone(&Local).date_naive() == now.date_naive())
        .map(|event| event_cost_usd(event).unwrap_or(0.0))
        .sum::<f64>();

    let total_native_today = usage_events
        .iter()
        .filter(|event| {
            event.occurred_at.with_timezone(&Local).date_naive() == now.date_naive()
                && event.calculation_method == CalculationMethod::Native
        })
        .map(|event| event.total_tokens)
        .sum::<u64>();

    let exact_share = if total_tokens_today == 0 {
        0.0
    } else {
        total_native_today as f64 / total_tokens_today as f64
    };

    let active_sources = usage_events
        .iter()
        .filter(|event| event.occurred_at.with_timezone(&Local).date_naive() == now.date_naive())
        .map(|event| event.source_id)
        .collect::<HashSet<_>>()
        .len() as u16;

    let elapsed_hours = ((now.hour() as f64) + (now.minute() as f64 / 60.0)).max(1.0);
    let burn_rate_per_hour = (total_tokens_today as f64 / elapsed_hours).round() as u64;

    let week = build_weekly_usage(&usage_events, now);
    let daily_history = build_daily_history(&usage_events, now, 180);
    let sources = build_source_usage(&reports, &source_names, now);
    let sessions = build_recent_sessions(&reports);
    let session_groups = build_session_groups(&reports);

    DashboardSnapshot {
        headline_date: now.format("%B %-d, %Y").to_string(),
        total_tokens_today,
        total_cost_today,
        exact_share,
        connected_sources,
        active_sources,
        burn_rate_per_hour,
        week,
        daily_history,
        sources,
        sessions,
        session_groups,
        source_statuses,
    }
}

fn build_source_snapshot_from_reports(
    reports: &[SourceReport],
    now: chrono::DateTime<Local>,
    source_id: &str,
) -> Option<SourceDetailSnapshot> {
    let report = reports
        .iter()
        .find(|report| report.status.id == source_id)?;
    let usage_events = report.usage_events.iter().collect::<Vec<_>>();
    let week = build_weekly_usage(&usage_events, now);
    let daily_history = build_daily_history(&usage_events, now, 180);
    let today_days = window_days_ending_on(now.date_naive(), 1);
    let last_7d_days = window_days_ending_on(now.date_naive(), 7);
    let last_30d_days = window_days_ending_on(now.date_naive(), 30);
    let today_summary = summarize_window_with_previous_period(&usage_events, &today_days);
    let last7d_summary = summarize_window_with_previous_period(&usage_events, &last_7d_days);
    let last30d_summary = summarize_window_with_previous_period(&usage_events, &last_30d_days);
    let lifetime_days = distinct_event_days(&usage_events);
    let lifetime_summary = summarize_window(&usage_events, &lifetime_days);
    let periodic_breakdowns = build_periodic_breakdowns(&usage_events, now);
    let session_pricing = build_session_pricing_facts(reports);
    let mut sessions = report.sessions.iter().collect::<Vec<_>>();
    sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));

    Some(SourceDetailSnapshot {
        source_id: report.status.id.clone(),
        source_name: report.status.name.clone(),
        status: report.status.clone(),
        calculation_mix: report_calculation_mix(report),
        today_tokens: today_summary.tokens,
        today_cost_usd: today_summary.cost_usd,
        week,
        daily_history,
        sessions: sessions
            .into_iter()
            .map(|record| attach_session_cost(&record.summary, &session_pricing))
            .collect(),
        today_summary,
        last7d_summary,
        last30d_summary,
        lifetime_summary,
        periodic_breakdowns,
        billing_state: None,
    })
}

pub fn build_dashboard_snapshot_json() -> JsonResult<String> {
    serde_json::to_string(&build_dashboard_snapshot())
}

pub fn build_source_snapshot_json(source_id: &str) -> Result<String, String> {
    build_source_snapshot(source_id)
        .and_then(|snapshot| serde_json::to_string(&snapshot).map_err(|error| error.to_string()))
}

pub fn build_dashboard_snapshot_json_with_progress<F>(on_progress: F) -> JsonResult<String>
where
    F: FnMut(usize, usize, &str),
{
    serde_json::to_string(&build_dashboard_snapshot_with_progress(on_progress))
}

pub fn set_scan_detail_hook(hook: Option<Arc<dyn Fn(String, String) + Send + Sync>>) {
    connectors::set_scan_detail_hook(hook);
}

pub fn load_app_settings_json() -> JsonResult<String> {
    serde_json::to_string(&settings::load_app_settings().unwrap_or_default())
}

pub fn load_app_settings() -> AppSettings {
    settings::load_app_settings().unwrap_or_default()
}

pub fn update_cherry_backup_dir(path: String) -> Result<AppSettings, String> {
    settings::set_cherry_backup_dir(&path).map_err(|error| error.to_string())
}

pub fn reset_cherry_backup_dir() -> Result<AppSettings, String> {
    settings::clear_cherry_backup_dir().map_err(|error| error.to_string())
}

fn build_weekly_usage(
    usage_events: &[&connectors::UsageEvent],
    now: chrono::DateTime<Local>,
) -> Vec<DailyUsagePoint> {
    build_usage_window(usage_events, now, 7)
}

fn build_daily_history(
    usage_events: &[&connectors::UsageEvent],
    now: chrono::DateTime<Local>,
    day_count: usize,
) -> Vec<DailyUsagePoint> {
    build_usage_window(usage_events, now, day_count)
}

fn build_usage_window(
    usage_events: &[&UsageEvent],
    now: chrono::DateTime<Local>,
    day_count: usize,
) -> Vec<DailyUsagePoint> {
    window_days_ending_on(now.date_naive(), day_count)
        .into_iter()
        .map(|day| {
            let summary = summarize_window(usage_events, &[day]);
            DailyUsagePoint {
                date: day.format("%Y-%m-%d").to_string(),
                total_tokens: summary.tokens,
                total_cost_usd: summary.cost_usd,
                exact_share: summary.exact_share,
                active_sources: count_active_sources_for_day(usage_events, day),
                session_count: summary.sessions,
                priced_sessions: summary.priced_sessions,
                pending_pricing_sessions: summary.pending_pricing_sessions,
                pricing_coverage: summary.pricing_coverage,
            }
        })
        .collect()
}

fn build_source_usage(
    reports: &[SourceReport],
    source_names: &HashMap<String, String>,
    now: chrono::DateTime<Local>,
) -> Vec<SourceUsage> {
    let today = now.date_naive();
    let yesterday = today - Duration::days(1);
    let mut usage_by_source =
        HashMap::<String, (u64, u64, f64, HashSet<String>, HashSet<String>)>::new();

    for report in reports {
        for event in &report.usage_events {
            let local_day = event.occurred_at.with_timezone(&Local).date_naive();
            let entry = usage_by_source
                .entry(event.source_id.to_string())
                .or_insert((0, 0, 0.0, HashSet::new(), HashSet::new()));
            if local_day == today {
                entry.0 += event.total_tokens;
                entry.2 += event_cost_usd(event).unwrap_or(0.0);
                entry.3.insert(event.session_id.clone());
            } else if local_day == yesterday {
                entry.1 += event.total_tokens;
                entry.4.insert(event.session_id.clone());
            }
        }
    }

    reports
        .iter()
        .filter(|report| !matches!(report.status.state, models::SourceState::Missing))
        .map(|report| {
            let (today_tokens, yesterday_tokens, today_cost_usd, today_sessions, _) =
                usage_by_source.remove(&report.status.id).unwrap_or((
                    0,
                    0,
                    0.0,
                    HashSet::new(),
                    HashSet::new(),
                ));
            let trend = if today_tokens > yesterday_tokens + (yesterday_tokens / 20).max(1) {
                "up"
            } else if yesterday_tokens > today_tokens + (today_tokens / 20).max(1) {
                "down"
            } else {
                "flat"
            };

            let calculation_mix = report_calculation_mix(report);

            SourceUsage {
                source_id: report.status.id.clone(),
                source: source_names
                    .get(&report.status.id)
                    .cloned()
                    .unwrap_or_else(|| report.status.name.clone()),
                tokens: today_tokens,
                cost_usd: today_cost_usd,
                sessions: today_sessions.len() as u32,
                trend: trend.into(),
                calculation_mix,
            }
        })
        .collect()
}

fn build_recent_sessions(reports: &[SourceReport]) -> Vec<SessionSummary> {
    let session_costs = build_session_pricing_facts(reports);
    let mut sessions = reports
        .iter()
        .flat_map(|report| report.sessions.iter())
        .collect::<Vec<_>>();
    sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    sessions
        .into_iter()
        .take(8)
        .map(|record| attach_session_cost(&record.summary, &session_costs))
        .collect()
}

fn build_session_groups(reports: &[SourceReport]) -> Vec<SessionGroup> {
    let session_costs = build_session_pricing_facts(reports);
    let mut groups = reports
        .iter()
        .filter(|report| !matches!(report.status.state, models::SourceState::Missing))
        .map(|report| {
            let mut sessions = report.sessions.iter().collect::<Vec<_>>();
            sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));

            SessionGroup {
                source_id: report.status.id.clone(),
                source_name: report.status.name.clone(),
                source_state: report.status.state,
                sessions: sessions
                    .into_iter()
                    .map(|record| attach_session_cost(&record.summary, &session_costs))
                    .collect(),
            }
        })
        .collect::<Vec<_>>();

    groups.sort_by(|left, right| right.sessions.len().cmp(&left.sessions.len()));
    groups
}

fn count_active_sources_for_day(usage_events: &[&UsageEvent], day: chrono::NaiveDate) -> u16 {
    usage_events
        .iter()
        .filter(|event| event.occurred_at.with_timezone(&Local).date_naive() == day)
        .map(|event| event.source_id)
        .collect::<HashSet<_>>()
        .len() as u16
}

#[derive(Clone, Copy, Debug, Default)]
struct SessionPricingFact {
    total_tokens: u64,
    cost_usd: f64,
    coverage: PricingCoverage,
}

fn event_cost_usd(event: &UsageEvent) -> Option<f64> {
    event.explicit_cost_usd.or_else(|| {
        if source_supports_estimated_cost(event.source_id) {
            estimate_cost_usd(&event.model, event.token_breakdown)
        } else {
            None
        }
    })
}

fn derive_pricing_coverage(priced: u32, pending: u32) -> PricingCoverage {
    if pending == 0 && priced > 0 {
        PricingCoverage::Actual
    } else if priced > 0 && pending > 0 {
        PricingCoverage::Partial
    } else {
        PricingCoverage::Pending
    }
}

fn legacy_pricing_state(coverage: PricingCoverage) -> &'static str {
    match coverage {
        PricingCoverage::Actual => "actual",
        PricingCoverage::Partial | PricingCoverage::Pending => "pending",
    }
}

fn session_pricing_fact(
    total_events: u32,
    priced_events: u32,
    total_tokens: u64,
    cost_usd: f64,
) -> SessionPricingFact {
    SessionPricingFact {
        total_tokens,
        cost_usd,
        coverage: derive_pricing_coverage(
            priced_events,
            total_events.saturating_sub(priced_events),
        ),
    }
}

fn collect_session_pricing_facts<'a>(
    events: impl IntoIterator<Item = &'a UsageEvent>,
) -> HashMap<String, SessionPricingFact> {
    let mut facts = HashMap::<String, (u32, u32, u64, f64)>::new();
    for event in events {
        let entry = facts
            .entry(session_cost_key(event.source_id, &event.session_id))
            .or_insert((0, 0, 0, 0.0));
        entry.0 += 1;
        entry.2 += event.total_tokens;
        if let Some(cost_usd) = event_cost_usd(event) {
            entry.1 += 1;
            entry.3 += cost_usd;
        }
    }

    facts
        .into_iter()
        .map(|(key, (total_events, priced_events, total_tokens, cost_usd))| {
            (
                key,
                session_pricing_fact(total_events, priced_events, total_tokens, cost_usd),
            )
        })
        .collect()
}

fn summarize_window(events: &[&UsageEvent], days: &[NaiveDate]) -> UsageWindowSummary {
    if days.is_empty() {
        return UsageWindowSummary::default();
    }

    let window_events = filter_events_for_days(events, days);
    let tokens = window_events
        .iter()
        .map(|event| event.total_tokens)
        .sum::<u64>();
    let native_tokens = window_events
        .iter()
        .filter(|event| event.calculation_method == CalculationMethod::Native)
        .map(|event| event.total_tokens)
        .sum::<u64>();
    let cost_usd = window_events
        .iter()
        .map(|event| event_cost_usd(event).unwrap_or(0.0))
        .sum::<f64>();
    let session_facts = session_pricing_facts(&window_events);
    let sessions = session_facts.len() as u32;
    let priced_sessions = session_facts
        .values()
        .filter(|fact| fact.coverage == PricingCoverage::Actual)
        .count() as u32;
    let pending_pricing_sessions = session_facts
        .values()
        .filter(|fact| fact.coverage != PricingCoverage::Actual)
        .count() as u32;
    let active_days = distinct_event_days(&window_events).len() as u16;
    let avg_per_active_day = if active_days == 0 {
        0.0
    } else {
        tokens as f64 / active_days as f64
    };
    let exact_share = if tokens == 0 {
        0.0
    } else {
        native_tokens as f64 / tokens as f64
    };

    UsageWindowSummary {
        tokens,
        cost_usd,
        sessions,
        priced_sessions,
        pending_pricing_sessions,
        active_days,
        avg_per_active_day,
        exact_share,
        peak_day: peak_usage_point(&window_events, days),
        pricing_coverage: derive_pricing_coverage(priced_sessions, pending_pricing_sessions),
        delta_vs_previous_period: None,
    }
}

fn summarize_window_with_previous_period(
    events: &[&UsageEvent],
    days: &[NaiveDate],
) -> UsageWindowSummary {
    let mut summary = summarize_window(events, days);
    let previous_days = previous_period_days(days);
    let previous_summary = summarize_window(events, &previous_days);
    summary.delta_vs_previous_period = Some(window_delta(summary.tokens, previous_summary.tokens));
    summary
}

fn build_periodic_breakdowns(
    usage_events: &[&UsageEvent],
    now: chrono::DateTime<Local>,
) -> PeriodicBreakdownSet {
    let today = now.date_naive();
    let current_week_start = start_of_week(today);
    let current_month_start = start_of_month(today);

    let weekly = (0..8)
        .rev()
        .map(|offset| current_week_start - Duration::days((offset * 7) as i64))
        .map(|start| {
            let end = start + Duration::days(6);
            let effective_end = end.min(today);
            periodic_breakdown_row(
                usage_events,
                start,
                effective_end,
                if start == current_week_start && today < end {
                    "This week".into()
                } else {
                    format!("{} to {}", start.format("%Y-%m-%d"), end.format("%Y-%m-%d"))
                },
            )
        })
        .collect();

    let monthly = (0..6)
        .rev()
        .map(|offset| shift_month_start(current_month_start, -(offset as i32)))
        .map(|start| {
            let end = end_of_month(start);
            let effective_end = end.min(today);
            periodic_breakdown_row(
                usage_events,
                start,
                effective_end,
                if start == current_month_start && today < end {
                    "This month".into()
                } else {
                    start.format("%B %Y").to_string()
                },
            )
        })
        .collect();

    PeriodicBreakdownSet { weekly, monthly }
}

fn periodic_breakdown_row(
    usage_events: &[&UsageEvent],
    start: NaiveDate,
    end: NaiveDate,
    label: String,
) -> PeriodicBreakdownRow {
    let days = inclusive_days(start, end);
    let summary = summarize_window(usage_events, &days);

    PeriodicBreakdownRow {
        label,
        start_date: start.format("%Y-%m-%d").to_string(),
        end_date: end.format("%Y-%m-%d").to_string(),
        tokens: summary.tokens,
        cost_usd: summary.cost_usd,
        sessions: summary.sessions,
        priced_sessions: summary.priced_sessions,
        pending_pricing_sessions: summary.pending_pricing_sessions,
        pricing_coverage: summary.pricing_coverage,
        active_days: summary.active_days,
    }
}

fn build_session_pricing_facts(reports: &[SourceReport]) -> HashMap<String, SessionPricingFact> {
    collect_session_pricing_facts(reports.iter().flat_map(|report| report.usage_events.iter()))
}

fn attach_session_cost(
    summary: &SessionSummary,
    session_costs: &HashMap<String, SessionPricingFact>,
) -> SessionSummary {
    let mut summary = summary.clone();
    if let Some(fact) = session_costs.get(&session_cost_key(&summary.source_id, &summary.id)) {
        if fact.total_tokens > 0 {
            summary.total_tokens = fact.total_tokens;
        }
        summary.cost_usd = fact.cost_usd;
        summary.priced_sessions = u32::from(fact.coverage == PricingCoverage::Actual);
        summary.pending_pricing_sessions = u32::from(fact.coverage != PricingCoverage::Actual);
        summary.pricing_coverage = fact.coverage;
        summary.pricing_state = legacy_pricing_state(fact.coverage).into();
    } else {
        summary.cost_usd = 0.0;
        summary.priced_sessions = 0;
        summary.pending_pricing_sessions = 1;
        summary.pricing_coverage = PricingCoverage::Pending;
        summary.pricing_state = legacy_pricing_state(PricingCoverage::Pending).into();
    }
    summary
}

fn session_cost_key(source_id: &str, session_id: &str) -> String {
    format!("{source_id}::{session_id}")
}

fn filter_events_for_days<'a>(
    events: &'a [&'a UsageEvent],
    days: &[NaiveDate],
) -> Vec<&'a UsageEvent> {
    let day_set = days.iter().copied().collect::<HashSet<_>>();
    events
        .iter()
        .copied()
        .filter(|event| day_set.contains(&event_local_day(event)))
        .collect()
}

fn session_pricing_facts(events: &[&UsageEvent]) -> HashMap<String, SessionPricingFact> {
    collect_session_pricing_facts(events.iter().copied())
}

fn peak_usage_point(events: &[&UsageEvent], days: &[NaiveDate]) -> Option<PeakUsagePoint> {
    days.iter()
        .filter_map(|day| {
            let day_events = events
                .iter()
                .copied()
                .filter(|event| event_local_day(event) == *day)
                .collect::<Vec<_>>();
            if day_events.is_empty() {
                return None;
            }

            Some(PeakUsagePoint {
                date: day.format("%Y-%m-%d").to_string(),
                total_tokens: day_events.iter().map(|event| event.total_tokens).sum(),
                total_cost_usd: day_events
                    .iter()
                    .map(|event| event_cost_usd(event).unwrap_or(0.0))
                    .sum(),
            })
        })
        .max_by(|left, right| {
            left.total_tokens
                .cmp(&right.total_tokens)
                .then_with(|| left.date.cmp(&right.date))
        })
}

fn event_local_day(event: &UsageEvent) -> NaiveDate {
    event.occurred_at.with_timezone(&Local).date_naive()
}

fn distinct_event_days(events: &[&UsageEvent]) -> Vec<NaiveDate> {
    let mut days = events
        .iter()
        .map(|event| event_local_day(event))
        .collect::<Vec<_>>();
    days.sort_unstable();
    days.dedup();
    days
}

fn window_days_ending_on(end: NaiveDate, day_count: usize) -> Vec<NaiveDate> {
    if day_count == 0 {
        return Vec::new();
    }

    (0..day_count)
        .map(|offset| end - Duration::days((day_count - 1 - offset) as i64))
        .collect()
}

fn previous_period_days(days: &[NaiveDate]) -> Vec<NaiveDate> {
    let Some(first_day) = days.first().copied() else {
        return Vec::new();
    };
    let day_count = days.len() as i64;
    let start = first_day - Duration::days(day_count);
    (0..days.len())
        .map(|offset| start + Duration::days(offset as i64))
        .collect()
}

fn window_delta(current_tokens: u64, previous_tokens: u64) -> WindowDelta {
    let tokens_delta = current_tokens as i64 - previous_tokens as i64;
    let tokens_percent_change = if previous_tokens == 0 {
        None
    } else {
        Some(tokens_delta as f64 / previous_tokens as f64)
    };

    WindowDelta {
        tokens_delta,
        tokens_percent_change,
    }
}

fn inclusive_days(start: NaiveDate, end: NaiveDate) -> Vec<NaiveDate> {
    if end < start {
        return Vec::new();
    }

    let span = (end - start).num_days();
    (0..=span)
        .map(|offset| start + Duration::days(offset))
        .collect()
}

fn start_of_week(day: NaiveDate) -> NaiveDate {
    day - Duration::days(day.weekday().num_days_from_monday() as i64)
}

fn start_of_month(day: NaiveDate) -> NaiveDate {
    day.with_day(1).expect("valid first day of month")
}

fn end_of_month(start: NaiveDate) -> NaiveDate {
    shift_month_start(start, 1) - Duration::days(1)
}

fn shift_month_start(start: NaiveDate, offset: i32) -> NaiveDate {
    let month_index = start.year() * 12 + start.month0() as i32 + offset;
    let year = month_index.div_euclid(12);
    let month = month_index.rem_euclid(12) as u32 + 1;
    NaiveDate::from_ymd_opt(year, month, 1).expect("valid shifted month")
}

fn report_calculation_mix(report: &SourceReport) -> String {
    if report.usage_events.is_empty() {
        return "estimated".into();
    }

    let methods = report
        .usage_events
        .iter()
        .map(|event| event.calculation_method)
        .collect::<HashSet<_>>();
    if methods.len() == 1 {
        methods
            .into_iter()
            .next()
            .map(method_label)
            .unwrap_or_else(|| "estimated".into())
    } else {
        "mixed".into()
    }
}

fn method_label(method: CalculationMethod) -> String {
    match method {
        CalculationMethod::Native => "native".into(),
        CalculationMethod::Derived => "derived".into(),
        CalculationMethod::Estimated => "estimated".into(),
    }
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_dashboard_snapshot,
            get_source_snapshot,
            get_app_settings,
            set_cherry_backup_dir,
            clear_cherry_backup_dir
        ])
        .run(tauri::generate_context!())
        .expect("error while running Burned");
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    use crate::connectors::{SessionRecord, SourceReport, UsageEvent};
    use crate::models::{SourceState, SourceStatus};
    use crate::pricing::TokenBreakdown;

    #[test]
    fn dashboard_snapshot_rolls_up_estimated_costs_across_views() {
        let now = Local
            .with_ymd_and_hms(2026, 3, 24, 12, 0, 0)
            .single()
            .expect("local datetime");
        let occurred_at = now.with_timezone(&Utc);
        let expected_cost = 0.006_375;
        let report = SourceReport {
            status: ready_status("codex", "Codex"),
            usage_events: vec![UsageEvent {
                source_id: "codex",
                occurred_at,
                model: "gpt-5.4".into(),
                token_breakdown: TokenBreakdown {
                    input_tokens: 1_000,
                    cached_input_tokens: 500,
                    output_tokens: 250,
                    ..TokenBreakdown::default()
                },
                total_tokens: 1_750,
                calculation_method: CalculationMethod::Native,
                session_id: "session-1".into(),
                explicit_cost_usd: None,
            }],
            sessions: vec![SessionRecord {
                updated_at: occurred_at,
                summary: SessionSummary {
                    id: "session-1".into(),
                    source_id: "codex".into(),
                    title: "Session".into(),
                    preview: "Preview".into(),
                    source: "Codex".into(),
                    workspace: "burned".into(),
                    model: "gpt-5.4".into(),
                    started_at: "Mar 24 12:00".into(),
                    total_tokens: 1_750,
                    cost_usd: 0.0,
                    priced_sessions: 0,
                    pending_pricing_sessions: 0,
                    pricing_coverage: models::PricingCoverage::Pending,
                    pricing_state: "pending".into(),
                    calculation_method: CalculationMethod::Native,
                    status: "indexed".into(),
                },
            }],
        };

        let snapshot = build_dashboard_snapshot_from_reports(vec![report], now);

        approx_eq(snapshot.total_cost_today, expected_cost);
        approx_eq(snapshot.sources[0].cost_usd, expected_cost);
        approx_eq(snapshot.sessions[0].cost_usd, expected_cost);
        approx_eq(snapshot.week[6].total_cost_usd, expected_cost);
        approx_eq(
            snapshot
                .daily_history
                .last()
                .expect("daily point")
                .total_cost_usd,
            expected_cost,
        );
    }

    #[test]
    fn unsupported_models_keep_cost_pending() {
        let now = Local
            .with_ymd_and_hms(2026, 3, 24, 12, 0, 0)
            .single()
            .expect("local datetime");
        let occurred_at = now.with_timezone(&Utc);
        let report = SourceReport {
            status: ready_status("cursor", "Cursor"),
            usage_events: vec![UsageEvent {
                source_id: "cursor",
                occurred_at,
                model: "unknown".into(),
                token_breakdown: TokenBreakdown {
                    other_tokens: 8_000,
                    ..TokenBreakdown::default()
                },
                total_tokens: 8_000,
                calculation_method: CalculationMethod::Estimated,
                session_id: "cursor-1".into(),
                explicit_cost_usd: None,
            }],
            sessions: vec![SessionRecord {
                updated_at: occurred_at,
                summary: SessionSummary {
                    id: "cursor-1".into(),
                    source_id: "cursor".into(),
                    title: "Session".into(),
                    preview: "Preview".into(),
                    source: "Cursor".into(),
                    workspace: "burned".into(),
                    model: "unknown".into(),
                    started_at: "Mar 24 12:00".into(),
                    total_tokens: 8_000,
                    cost_usd: 0.0,
                    priced_sessions: 0,
                    pending_pricing_sessions: 0,
                    pricing_coverage: models::PricingCoverage::Pending,
                    pricing_state: "pending".into(),
                    calculation_method: CalculationMethod::Estimated,
                    status: "indexed".into(),
                },
            }],
        };

        let snapshot = build_dashboard_snapshot_from_reports(vec![report], now);

        assert_eq!(snapshot.total_tokens_today, 8_000);
        approx_eq(snapshot.total_cost_today, 0.0);
        approx_eq(snapshot.sources[0].cost_usd, 0.0);
        approx_eq(snapshot.sessions[0].cost_usd, 0.0);
    }

    #[test]
    fn source_detail_snapshot_rolls_up_source_history_and_costs() {
        let now = Local
            .with_ymd_and_hms(2026, 3, 24, 12, 0, 0)
            .single()
            .expect("local datetime");
        let occurred_at = now.with_timezone(&Utc);
        let expected_cost = 0.006_375;
        let report = SourceReport {
            status: ready_status("codex", "Codex"),
            usage_events: vec![UsageEvent {
                source_id: "codex",
                occurred_at,
                model: "gpt-5.4".into(),
                token_breakdown: TokenBreakdown {
                    input_tokens: 1_000,
                    cached_input_tokens: 500,
                    output_tokens: 250,
                    ..TokenBreakdown::default()
                },
                total_tokens: 1_750,
                calculation_method: CalculationMethod::Native,
                session_id: "session-1".into(),
                explicit_cost_usd: None,
            }],
            sessions: vec![SessionRecord {
                updated_at: occurred_at,
                summary: SessionSummary {
                    id: "session-1".into(),
                    source_id: "codex".into(),
                    title: "Session".into(),
                    preview: "Preview".into(),
                    source: "Codex".into(),
                    workspace: "burned".into(),
                    model: "gpt-5.4".into(),
                    started_at: "Mar 24 12:00".into(),
                    total_tokens: 1_750,
                    cost_usd: 0.0,
                    priced_sessions: 0,
                    pending_pricing_sessions: 0,
                    pricing_coverage: models::PricingCoverage::Pending,
                    pricing_state: "pending".into(),
                    calculation_method: CalculationMethod::Native,
                    status: "indexed".into(),
                },
            }],
        };

        let snapshot =
            build_source_snapshot_from_reports(&[report], now, "codex").expect("source snapshot");

        assert_eq!(snapshot.source_id, "codex");
        approx_eq(snapshot.today_cost_usd, expected_cost);
        approx_eq(snapshot.week[6].total_cost_usd, expected_cost);
        approx_eq(snapshot.sessions[0].cost_usd, expected_cost);
    }

    #[test]
    fn source_detail_snapshot_includes_summary_windows_and_periodic_breakdowns() {
        let now = Local
            .with_ymd_and_hms(2026, 3, 24, 12, 0, 0)
            .single()
            .expect("local datetime");
        let report = source_report_with_days(
            "antigravity",
            "Antigravity",
            now,
            &[
                (0, 2_000, "antigravity-1"),
                (7, 3_000, "antigravity-2"),
                (35, 4_000, "antigravity-3"),
            ],
        );

        let snapshot = build_source_snapshot_from_reports(&[report], now, "antigravity")
            .expect("source snapshot");

        assert_eq!(snapshot.today_summary.tokens, 2_000);
        assert_eq!(snapshot.last7d_summary.tokens, 2_000);
        assert_eq!(snapshot.last30d_summary.tokens, 5_000);
        assert_eq!(snapshot.lifetime_summary.tokens, 9_000);
        assert!(!snapshot.periodic_breakdowns.weekly.is_empty());
        assert!(!snapshot.periodic_breakdowns.monthly.is_empty());
    }

    #[test]
    fn lifetime_summary_uses_full_event_history_not_bounded_daily_history() {
        let now = Local
            .with_ymd_and_hms(2026, 3, 24, 12, 0, 0)
            .single()
            .expect("local datetime");
        let report = source_report_with_days(
            "cursor",
            "Cursor",
            now,
            &[
                (0, 1_000, "cursor-1"),
                (45, 2_000, "cursor-2"),
                (210, 3_000, "cursor-3"),
            ],
        );

        let snapshot =
            build_source_snapshot_from_reports(&[report], now, "cursor").expect("source snapshot");

        assert_eq!(snapshot.lifetime_summary.tokens, 6_000);
        assert_eq!(snapshot.daily_history.len(), 180);
    }

    #[test]
    fn source_detail_pricing_coverage_is_partial_when_only_some_sessions_are_priced() {
        let now = Local
            .with_ymd_and_hms(2026, 3, 24, 12, 0, 0)
            .single()
            .expect("local datetime");
        let occurred_at = now.with_timezone(&Utc);
        let report = SourceReport {
            status: ready_status("codex", "Codex"),
            usage_events: vec![
                UsageEvent {
                    source_id: "codex",
                    occurred_at,
                    model: "gpt-5.4".into(),
                    token_breakdown: TokenBreakdown {
                        input_tokens: 1_000,
                        cached_input_tokens: 500,
                        output_tokens: 250,
                        ..TokenBreakdown::default()
                    },
                    total_tokens: 1_750,
                    calculation_method: CalculationMethod::Native,
                    session_id: "session-priced".into(),
                    explicit_cost_usd: None,
                },
                UsageEvent {
                    source_id: "codex",
                    occurred_at,
                    model: "unknown".into(),
                    token_breakdown: TokenBreakdown {
                        other_tokens: 2_000,
                        ..TokenBreakdown::default()
                    },
                    total_tokens: 2_000,
                    calculation_method: CalculationMethod::Estimated,
                    session_id: "session-pending".into(),
                    explicit_cost_usd: None,
                },
            ],
            sessions: vec![
                SessionRecord {
                    updated_at: occurred_at,
                    summary: SessionSummary {
                        id: "session-priced".into(),
                        source_id: "codex".into(),
                        title: "Session".into(),
                        preview: "Preview".into(),
                        source: "Codex".into(),
                        workspace: "burned".into(),
                        model: "gpt-5.4".into(),
                        started_at: "Mar 24 12:00".into(),
                        total_tokens: 1_750,
                        cost_usd: 0.0,
                        priced_sessions: 0,
                        pending_pricing_sessions: 0,
                        pricing_coverage: models::PricingCoverage::Pending,
                        pricing_state: "pending".into(),
                        calculation_method: CalculationMethod::Native,
                        status: "indexed".into(),
                    },
                },
                SessionRecord {
                    updated_at: occurred_at,
                    summary: SessionSummary {
                        id: "session-pending".into(),
                        source_id: "codex".into(),
                        title: "Session".into(),
                        preview: "Preview".into(),
                        source: "Codex".into(),
                        workspace: "burned".into(),
                        model: "unknown".into(),
                        started_at: "Mar 24 12:00".into(),
                        total_tokens: 2_000,
                        cost_usd: 0.0,
                        priced_sessions: 0,
                        pending_pricing_sessions: 0,
                        pricing_coverage: models::PricingCoverage::Pending,
                        pricing_state: "pending".into(),
                        calculation_method: CalculationMethod::Estimated,
                        status: "pending".into(),
                    },
                },
            ],
        };

        let snapshot =
            build_source_snapshot_from_reports(&[report], now, "codex").expect("source snapshot");

        assert_eq!(snapshot.today_cost_usd, 0.006_375);
        assert_eq!(
            snapshot.today_summary.pricing_coverage,
            models::PricingCoverage::Partial
        );
        assert!(snapshot.today_summary.pending_pricing_sessions > 0);
    }

    #[test]
    fn attach_session_cost_keeps_legacy_pricing_state_pending_for_partial_coverage() {
        let summary = SessionSummary {
            id: "session-mixed".into(),
            source_id: "codex".into(),
            title: "Session".into(),
            preview: "Preview".into(),
            source: "Codex".into(),
            workspace: "burned".into(),
            model: "gpt-5.4".into(),
            started_at: "Mar 24 12:00".into(),
            total_tokens: 3_750,
            cost_usd: 0.0,
            priced_sessions: 0,
            pending_pricing_sessions: 0,
            pricing_coverage: models::PricingCoverage::Pending,
            pricing_state: "pending".into(),
            calculation_method: CalculationMethod::Native,
            status: "indexed".into(),
        };
        let session_costs = HashMap::from([(
            session_cost_key("codex", "session-mixed"),
            SessionPricingFact {
                total_tokens: 3_750,
                cost_usd: 0.006_375,
                coverage: models::PricingCoverage::Partial,
            },
        )]);

        let attached = attach_session_cost(&summary, &session_costs);

        approx_eq(attached.cost_usd, 0.006_375);
        assert_eq!(attached.priced_sessions, 0);
        assert_eq!(attached.pending_pricing_sessions, 1);
        assert_eq!(attached.pricing_coverage, models::PricingCoverage::Partial);
        assert_eq!(attached.pricing_state, "pending");
    }

    #[test]
    fn attach_session_cost_uses_event_sourced_tokens_when_available() {
        let summary = SessionSummary {
            id: "session-priced".into(),
            source_id: "codex".into(),
            title: "Session".into(),
            preview: "Preview".into(),
            source: "Codex".into(),
            workspace: "burned".into(),
            model: "gpt-5.4".into(),
            started_at: "Mar 24 12:00".into(),
            total_tokens: 31_351_672,
            cost_usd: 0.0,
            priced_sessions: 0,
            pending_pricing_sessions: 0,
            pricing_coverage: models::PricingCoverage::Pending,
            pricing_state: "pending".into(),
            calculation_method: CalculationMethod::Native,
            status: "indexed".into(),
        };
        let session_costs = HashMap::from([(
            session_cost_key("codex", "session-priced"),
            SessionPricingFact {
                total_tokens: 72_033_066,
                cost_usd: 104.277_195_5,
                coverage: models::PricingCoverage::Partial,
            },
        )]);

        let attached = attach_session_cost(&summary, &session_costs);

        assert_eq!(attached.total_tokens, 72_033_066);
        approx_eq(attached.cost_usd, 104.277_195_5);
    }

    #[test]
    fn source_detail_summary_windows_include_previous_period_deltas() {
        let now = Local
            .with_ymd_and_hms(2026, 3, 24, 12, 0, 0)
            .single()
            .expect("local datetime");
        let report = source_report_with_days(
            "claude",
            "Claude Code",
            now,
            &[(0, 200, "claude-1"), (7, 100, "claude-2")],
        );

        let snapshot =
            build_source_snapshot_from_reports(&[report], now, "claude").expect("source snapshot");

        assert!(matches!(
            snapshot.last7d_summary.delta_vs_previous_period,
            Some(models::WindowDelta {
                tokens_delta: 100,
                tokens_percent_change: Some(percent),
            }) if (percent - 1.0).abs() < 1e-9
        ));
    }

    #[test]
    fn source_detail_periodic_breakdowns_are_limited_to_recent_expected_periods() {
        let now = Local
            .with_ymd_and_hms(2026, 3, 24, 12, 0, 0)
            .single()
            .expect("local datetime");
        let report = source_report_with_weekly_history("cherry", "Cherry Studio", now, 30);

        let snapshot =
            build_source_snapshot_from_reports(&[report], now, "cherry").expect("source snapshot");

        assert_eq!(snapshot.periodic_breakdowns.weekly.len(), 8);
        assert_eq!(snapshot.periodic_breakdowns.monthly.len(), 6);
    }

    fn ready_status(id: &str, name: &str) -> SourceStatus {
        SourceStatus {
            id: id.into(),
            name: name.into(),
            state: SourceState::Ready,
            capabilities: Vec::new(),
            note: String::new(),
            local_path: None,
            session_count: Some(1),
            last_seen_at: None,
        }
    }

    fn source_report_with_days(
        id: &'static str,
        name: &str,
        now: chrono::DateTime<Local>,
        days: &[(i64, u64, &str)],
    ) -> SourceReport {
        let usage_events = days
            .iter()
            .copied()
            .map(|(days_ago, total_tokens, session_id)| {
                let occurred_at = now
                    .checked_sub_signed(Duration::days(days_ago))
                    .expect("shifted local datetime")
                    .with_timezone(&Utc);
                UsageEvent {
                    source_id: id,
                    occurred_at,
                    model: "gpt-5.4".into(),
                    token_breakdown: TokenBreakdown {
                        input_tokens: total_tokens,
                        ..TokenBreakdown::default()
                    },
                    total_tokens,
                    calculation_method: CalculationMethod::Native,
                    session_id: session_id.into(),
                    explicit_cost_usd: None,
                }
            })
            .collect::<Vec<_>>();

        let sessions = days
            .iter()
            .copied()
            .map(|(days_ago, total_tokens, session_id)| {
                let occurred_at = now
                    .checked_sub_signed(Duration::days(days_ago))
                    .expect("shifted local datetime")
                    .with_timezone(&Utc);
                SessionRecord {
                    updated_at: occurred_at,
                    summary: SessionSummary {
                        id: session_id.into(),
                        source_id: id.into(),
                        title: "Session".into(),
                        preview: "Preview".into(),
                        source: name.into(),
                        workspace: "burned".into(),
                        model: "gpt-5.4".into(),
                        started_at: format!("{name} {days_ago}"),
                        total_tokens,
                        cost_usd: 0.0,
                        priced_sessions: 0,
                        pending_pricing_sessions: 0,
                        pricing_coverage: models::PricingCoverage::Pending,
                        pricing_state: "pending".into(),
                        calculation_method: CalculationMethod::Native,
                        status: "indexed".into(),
                    },
                }
            })
            .collect::<Vec<_>>();

        SourceReport {
            status: ready_status(id, name),
            usage_events,
            sessions,
        }
    }

    fn source_report_with_weekly_history(
        id: &'static str,
        name: &str,
        now: chrono::DateTime<Local>,
        week_count: usize,
    ) -> SourceReport {
        let mut usage_events = Vec::new();
        let mut sessions = Vec::new();

        for week in 0..week_count {
            let days_ago = (week * 7) as i64;
            let occurred_at = now
                .checked_sub_signed(Duration::days(days_ago))
                .expect("shifted local datetime")
                .with_timezone(&Utc);
            let session_id = format!("{id}-{week}");
            usage_events.push(UsageEvent {
                source_id: id,
                occurred_at,
                model: "gpt-5.4".into(),
                token_breakdown: TokenBreakdown {
                    input_tokens: 1_000 + week as u64,
                    ..TokenBreakdown::default()
                },
                total_tokens: 1_000 + week as u64,
                calculation_method: CalculationMethod::Native,
                session_id: session_id.clone(),
                explicit_cost_usd: None,
            });
            sessions.push(SessionRecord {
                updated_at: occurred_at,
                summary: SessionSummary {
                    id: session_id,
                    source_id: id.into(),
                    title: "Session".into(),
                    preview: "Preview".into(),
                    source: name.into(),
                    workspace: "burned".into(),
                    model: "gpt-5.4".into(),
                    started_at: format!("{name} {days_ago}"),
                    total_tokens: 1_000 + week as u64,
                    cost_usd: 0.0,
                    priced_sessions: 0,
                    pending_pricing_sessions: 0,
                    pricing_coverage: models::PricingCoverage::Pending,
                    pricing_state: "pending".into(),
                    calculation_method: CalculationMethod::Native,
                    status: "indexed".into(),
                },
            });
        }

        SourceReport {
            status: ready_status(id, name),
            usage_events,
            sessions,
        }
    }

    fn approx_eq(left: f64, right: f64) {
        let delta = (left - right).abs();
        assert!(delta < 1e-9, "left={left}, right={right}, delta={delta}");
    }
}
