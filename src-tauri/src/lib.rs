mod connectors;
mod models;
mod pricing;
mod settings;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{Datelike, Duration, Local, NaiveDate, Timelike};
use serde_json::Result as JsonResult;

use connectors::{collect_all, collect_all_with_progress, SourceReport};
pub use models::DashboardSnapshot;
use models::{
    AnalyticsState, CalculationMethod, DailyUsagePoint, PeakUsagePoint, PeriodicBreakdownRow,
    PeriodicBreakdowns, PricingCoverage, SessionGroup, SessionSummary, SourceDetailSnapshot,
    SourceStatus, SourceUsage, UsageWindowSummary, WindowDelta,
};
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
        .map(|event| event.estimated_cost_usd().unwrap_or(0.0))
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
    let report = reports.iter().find(|report| report.status.id == source_id)?;
    let usage_events = report.usage_events.iter().collect::<Vec<_>>();
    let analytics_state = report_analytics_state(report);
    let week = if analytics_state == AnalyticsState::Ready {
        build_weekly_usage(&usage_events, now)
    } else {
        Vec::new()
    };
    let daily_history = if analytics_state == AnalyticsState::Ready {
        build_daily_history(&usage_events, now, 30)
    } else {
        Vec::new()
    };
    let today = now.date_naive();
    let today_summary = if analytics_state == AnalyticsState::Ready {
        Some(build_window_summary(&usage_events, today, today, None, None))
    } else {
        None
    };
    let last7d_summary = if analytics_state == AnalyticsState::Ready {
        Some(build_window_summary(
            &usage_events,
            today - Duration::days(6),
            today,
            Some(today - Duration::days(13)),
            Some(today - Duration::days(7)),
        ))
    } else {
        None
    };
    let last30d_summary = if analytics_state == AnalyticsState::Ready {
        Some(build_window_summary(
            &usage_events,
            today - Duration::days(29),
            today,
            Some(today - Duration::days(59)),
            Some(today - Duration::days(30)),
        ))
    } else {
        None
    };
    let lifetime_summary = if analytics_state == AnalyticsState::Ready {
        let start = usage_events
            .iter()
            .map(|event| event.occurred_at.with_timezone(&Local).date_naive())
            .min()
            .unwrap_or(today);
        Some(build_window_summary(&usage_events, start, today, None, None))
    } else {
        None
    };
    let periodic_breakdowns = if analytics_state == AnalyticsState::Ready {
        Some(build_periodic_breakdowns(&usage_events, now))
    } else {
        None
    };
    let session_costs = build_session_costs(reports);
    let mut sessions = report.sessions.iter().collect::<Vec<_>>();
    sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));

    Some(SourceDetailSnapshot {
        source_id: report.status.id.clone(),
        source_name: report.status.name.clone(),
        status: report.status.clone(),
        analytics_state,
        calculation_mix: report_calculation_mix(report),
        today_summary,
        last7d_summary,
        last30d_summary,
        lifetime_summary,
        periodic_breakdowns,
        week,
        daily_history,
        sessions: sessions
            .into_iter()
            .map(|record| attach_session_cost(&record.summary, &session_costs))
            .collect(),
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

pub fn set_scan_detail_hook(
    hook: Option<Arc<dyn Fn(String, String) + Send + Sync>>,
) {
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

#[derive(Default)]
struct DayAccumulator {
    tokens: u64,
    cost_usd: f64,
    session_ids: HashSet<String>,
}

#[derive(Default)]
struct SummaryAccumulator {
    tokens: u64,
    exact_tokens: u64,
    priced_cost_usd: f64,
    sessions: HashSet<String>,
    priced_sessions: HashSet<String>,
    active_days: HashSet<NaiveDate>,
    day_totals: HashMap<NaiveDate, DayAccumulator>,
}

fn report_analytics_state(report: &SourceReport) -> AnalyticsState {
    if !report.usage_events.is_empty() {
        AnalyticsState::Ready
    } else if !report.sessions.is_empty() || report.status.session_count.unwrap_or(0) > 0 {
        AnalyticsState::SessionOnly
    } else {
        AnalyticsState::Unavailable
    }
}

fn pricing_coverage(total_sessions: usize, priced_sessions: usize) -> PricingCoverage {
    if total_sessions == 0 || total_sessions == priced_sessions {
        PricingCoverage::Actual
    } else if priced_sessions == 0 {
        PricingCoverage::Pending
    } else {
        PricingCoverage::Partial
    }
}

fn cost_for_coverage(
    total_sessions: usize,
    priced_sessions: usize,
    priced_cost_usd: f64,
) -> Option<f64> {
    if total_sessions == 0 {
        Some(0.0)
    } else if priced_sessions == 0 {
        None
    } else {
        Some(priced_cost_usd)
    }
}

fn aggregate_summary(
    usage_events: &[&connectors::UsageEvent],
    start: NaiveDate,
    end: NaiveDate,
) -> SummaryAccumulator {
    let mut accumulator = SummaryAccumulator::default();

    for event in usage_events {
        let local_day = event.occurred_at.with_timezone(&Local).date_naive();
        if local_day < start || local_day > end {
            continue;
        }

        accumulator.tokens += event.total_tokens;
        if event.calculation_method == CalculationMethod::Native {
            accumulator.exact_tokens += event.total_tokens;
        }

        accumulator.sessions.insert(event.session_id.clone());
        accumulator.active_days.insert(local_day);

        let day_entry = accumulator.day_totals.entry(local_day).or_default();
        day_entry.tokens += event.total_tokens;
        day_entry.session_ids.insert(event.session_id.clone());

        if let Some(cost_usd) = event.estimated_cost_usd() {
            accumulator.priced_cost_usd += cost_usd;
            accumulator.priced_sessions.insert(event.session_id.clone());
            day_entry.cost_usd += cost_usd;
        }
    }

    accumulator
}

fn summary_from_accumulator(
    accumulator: SummaryAccumulator,
    previous: Option<SummaryAccumulator>,
) -> UsageWindowSummary {
    let sessions = accumulator.sessions.len() as u32;
    let priced_sessions = accumulator.priced_sessions.len() as u32;
    let pending_pricing_sessions = sessions.saturating_sub(priced_sessions);
    let coverage = pricing_coverage(accumulator.sessions.len(), accumulator.priced_sessions.len());
    let active_days = accumulator.active_days.len() as u32;
    let avg_per_active_day = if active_days == 0 {
        0.0
    } else {
        accumulator.tokens as f64 / active_days as f64
    };
    let exact_share = if accumulator.tokens == 0 {
        0.0
    } else {
        accumulator.exact_tokens as f64 / accumulator.tokens as f64
    };
    let peak_day = accumulator
        .day_totals
        .iter()
        .max_by_key(|(day, totals)| (totals.tokens, **day))
        .map(|(day, totals)| PeakUsagePoint {
            date: day.format("%Y-%m-%d").to_string(),
            total_tokens: totals.tokens,
            total_cost_usd: if totals.session_ids.is_empty() {
                Some(0.0)
            } else if totals.cost_usd > 0.0 {
                Some(totals.cost_usd)
            } else {
                None
            },
            session_count: totals.session_ids.len() as u32,
        });
    let delta_vs_previous_period = previous.map(|previous| {
        let tokens_delta = accumulator.tokens as i64 - previous.tokens as i64;
        let tokens_percent_change = if previous.tokens == 0 {
            None
        } else {
            Some(tokens_delta as f64 / previous.tokens as f64)
        };

        WindowDelta {
            tokens_delta,
            tokens_percent_change,
        }
    });

    UsageWindowSummary {
        tokens: accumulator.tokens,
        cost_usd: cost_for_coverage(
            accumulator.sessions.len(),
            accumulator.priced_sessions.len(),
            accumulator.priced_cost_usd,
        ),
        sessions,
        priced_sessions,
        pending_pricing_sessions,
        active_days,
        avg_per_active_day,
        exact_share,
        pricing_coverage: coverage,
        peak_day,
        delta_vs_previous_period,
    }
}

fn build_window_summary(
    usage_events: &[&connectors::UsageEvent],
    start: NaiveDate,
    end: NaiveDate,
    previous_start: Option<NaiveDate>,
    previous_end: Option<NaiveDate>,
) -> UsageWindowSummary {
    let current = aggregate_summary(usage_events, start, end);
    let previous = match (previous_start, previous_end) {
        (Some(previous_start), Some(previous_end)) => {
            Some(aggregate_summary(usage_events, previous_start, previous_end))
        }
        _ => None,
    };

    summary_from_accumulator(current, previous)
}

fn start_of_week(day: NaiveDate) -> NaiveDate {
    let days_from_monday = day.weekday().num_days_from_monday() as i64;
    day - Duration::days(days_from_monday)
}

fn month_start(day: NaiveDate) -> NaiveDate {
    day.with_day(1).expect("valid first day of month")
}

fn shift_month(month_start_day: NaiveDate, delta_months: i32) -> NaiveDate {
    let total_months = month_start_day.year() * 12 + month_start_day.month0() as i32 + delta_months;
    let year = total_months.div_euclid(12);
    let month0 = total_months.rem_euclid(12) as u32;
    NaiveDate::from_ymd_opt(year, month0 + 1, 1).expect("valid shifted month")
}

fn build_periodic_breakdowns(
    usage_events: &[&connectors::UsageEvent],
    now: chrono::DateTime<Local>,
) -> PeriodicBreakdowns {
    let today = now.date_naive();
    let current_week_start = start_of_week(today);
    let weekly = (0..8)
        .rev()
        .map(|offset| {
            let start = current_week_start - Duration::weeks(offset as i64);
            let end = std::cmp::min(start + Duration::days(6), today);
            let summary = build_window_summary(usage_events, start, end, None, None);

            PeriodicBreakdownRow {
                label: format!("{} – {}", start.format("%Y-%m-%d"), end.format("%Y-%m-%d")),
                start_date: start.format("%Y-%m-%d").to_string(),
                end_date: end.format("%Y-%m-%d").to_string(),
                tokens: summary.tokens,
                cost_usd: summary.cost_usd,
                sessions: summary.sessions,
                priced_sessions: summary.priced_sessions,
                pending_pricing_sessions: summary.pending_pricing_sessions,
                active_days: summary.active_days,
                pricing_coverage: summary.pricing_coverage,
            }
        })
        .collect();

    let current_month_start = month_start(today);
    let monthly = (0..6)
        .rev()
        .map(|offset| {
            let start = shift_month(current_month_start, -(offset as i32));
            let next_month_start = shift_month(start, 1);
            let end = std::cmp::min(next_month_start - Duration::days(1), today);
            let summary = build_window_summary(usage_events, start, end, None, None);

            PeriodicBreakdownRow {
                label: format!("{} – {}", start.format("%Y-%m-%d"), end.format("%Y-%m-%d")),
                start_date: start.format("%Y-%m-%d").to_string(),
                end_date: end.format("%Y-%m-%d").to_string(),
                tokens: summary.tokens,
                cost_usd: summary.cost_usd,
                sessions: summary.sessions,
                priced_sessions: summary.priced_sessions,
                pending_pricing_sessions: summary.pending_pricing_sessions,
                active_days: summary.active_days,
                pricing_coverage: summary.pricing_coverage,
            }
        })
        .collect();

    PeriodicBreakdowns { weekly, monthly }
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
    usage_events: &[&connectors::UsageEvent],
    now: chrono::DateTime<Local>,
    day_count: usize,
) -> Vec<DailyUsagePoint> {
    let mut totals = HashMap::<String, (u64, u64, f64)>::new();
    for event in usage_events {
        let local_time = event.occurred_at.with_timezone(&Local);
        let key = local_time.date_naive().format("%Y-%m-%d").to_string();
        let entry = totals.entry(key).or_insert((0, 0, 0.0));
        entry.0 += event.total_tokens;
        if event.calculation_method == CalculationMethod::Native {
            entry.1 += event.total_tokens;
        }
        entry.2 += event.estimated_cost_usd().unwrap_or(0.0);
    }

    (0..day_count)
        .map(|offset| now.date_naive() - Duration::days((day_count - 1 - offset) as i64))
        .map(|day| {
            let key = day.format("%Y-%m-%d").to_string();
            let (total_tokens, exact_tokens, total_cost_usd) =
                totals.get(&key).copied().unwrap_or((0, 0, 0.0));
            let exact_share = if total_tokens == 0 {
                0.0
            } else {
                exact_tokens as f64 / total_tokens as f64
            };

            DailyUsagePoint {
                date: key,
                total_tokens,
                total_cost_usd,
                exact_share,
                active_sources: count_active_sources_for_day(usage_events, day),
                session_count: count_sessions_for_day(usage_events, day),
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
                entry.2 += event.estimated_cost_usd().unwrap_or(0.0);
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
            let analytics_state = report_analytics_state(report);
            let (today_tokens, yesterday_tokens, today_cost_usd, today_sessions, _) = usage_by_source
                .remove(&report.status.id)
                .unwrap_or((0, 0, 0.0, HashSet::new(), HashSet::new()));
            let calculation_mix = report_calculation_mix(report);
            let priced_sessions = report
                .usage_events
                .iter()
                .filter(|event| event.occurred_at.with_timezone(&Local).date_naive() == today)
                .filter_map(|event| {
                    event.estimated_cost_usd().map(|_| event.session_id.clone())
                })
                .collect::<HashSet<_>>();
            let row_pricing_coverage = pricing_coverage(today_sessions.len(), priced_sessions.len());

            SourceUsage {
                source_id: report.status.id.clone(),
                source: source_names
                    .get(&report.status.id)
                    .cloned()
                    .unwrap_or_else(|| report.status.name.clone()),
                analytics_state,
                tokens: if analytics_state == AnalyticsState::Ready {
                    Some(today_tokens)
                } else {
                    None
                },
                cost_usd: if analytics_state == AnalyticsState::Ready {
                    cost_for_coverage(today_sessions.len(), priced_sessions.len(), today_cost_usd)
                } else {
                    None
                },
                sessions: if analytics_state == AnalyticsState::Ready {
                    Some(today_sessions.len() as u32)
                } else {
                    None
                },
                trend: if analytics_state == AnalyticsState::Ready {
                    Some(
                        if today_tokens > yesterday_tokens + (yesterday_tokens / 20).max(1) {
                            "up"
                        } else if yesterday_tokens > today_tokens + (today_tokens / 20).max(1) {
                            "down"
                        } else {
                            "flat"
                        }
                        .into(),
                    )
                } else {
                    None
                },
                pricing_coverage: if analytics_state == AnalyticsState::Ready {
                    Some(row_pricing_coverage)
                } else {
                    None
                },
                calculation_mix,
            }
        })
        .collect()
}

fn build_recent_sessions(reports: &[SourceReport]) -> Vec<SessionSummary> {
    let session_costs = build_session_costs(reports);
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
    let session_costs = build_session_costs(reports);
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

fn count_active_sources_for_day(
    usage_events: &[&connectors::UsageEvent],
    day: chrono::NaiveDate,
) -> u16 {
    usage_events
        .iter()
        .filter(|event| event.occurred_at.with_timezone(&Local).date_naive() == day)
        .map(|event| event.source_id)
        .collect::<HashSet<_>>()
        .len() as u16
}

fn count_sessions_for_day(usage_events: &[&connectors::UsageEvent], day: chrono::NaiveDate) -> u32 {
    usage_events
        .iter()
        .filter(|event| event.occurred_at.with_timezone(&Local).date_naive() == day)
        .map(|event| event.session_id.clone())
        .collect::<HashSet<_>>()
        .len() as u32
}

fn build_session_costs(reports: &[SourceReport]) -> HashMap<String, f64> {
    let mut costs = HashMap::<String, f64>::new();
    for report in reports {
        for event in &report.usage_events {
            let Some(cost_usd) = event.estimated_cost_usd() else {
                continue;
            };
            *costs
                .entry(session_cost_key(event.source_id, &event.session_id))
                .or_insert(0.0) += cost_usd;
        }
    }
    costs
}

fn attach_session_cost(
    summary: &SessionSummary,
    session_costs: &HashMap<String, f64>,
) -> SessionSummary {
    let mut summary = summary.clone();
    if let Some(cost_usd) = session_costs.get(&session_cost_key(&summary.source_id, &summary.id)) {
        summary.cost_usd = *cost_usd;
    }
    summary
}

fn session_cost_key(source_id: &str, session_id: &str) -> String {
    format!("{source_id}::{session_id}")
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
                    calculation_method: CalculationMethod::Native,
                    status: "indexed".into(),
                },
            }],
        };

        let snapshot = build_dashboard_snapshot_from_reports(vec![report], now);

        approx_eq(snapshot.total_cost_today, expected_cost);
        approx_eq(snapshot.sources[0].cost_usd.expect("source cost"), expected_cost);
        approx_eq(snapshot.sessions[0].cost_usd, expected_cost);
        approx_eq(snapshot.week[6].total_cost_usd, expected_cost);
        approx_eq(snapshot.daily_history.last().expect("daily point").total_cost_usd, expected_cost);
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
                    calculation_method: CalculationMethod::Estimated,
                    status: "indexed".into(),
                },
            }],
        };

        let snapshot = build_dashboard_snapshot_from_reports(vec![report], now);

        assert_eq!(snapshot.total_tokens_today, 8_000);
        approx_eq(snapshot.total_cost_today, 0.0);
        assert_eq!(snapshot.sources[0].cost_usd, None);
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
                    calculation_method: CalculationMethod::Native,
                    status: "indexed".into(),
                },
            }],
        };

        let snapshot =
            build_source_snapshot_from_reports(&[report], now, "codex").expect("source snapshot");

        assert_eq!(snapshot.source_id, "codex");
        approx_eq(
            snapshot
                .today_summary
                .as_ref()
                .and_then(|summary| summary.cost_usd)
                .expect("today summary cost"),
            expected_cost,
        );
        approx_eq(snapshot.week[6].total_cost_usd, expected_cost);
        approx_eq(snapshot.sessions[0].cost_usd, expected_cost);
    }

    #[test]
    fn source_rows_distinguish_ready_session_only_and_unavailable() {
        let now = Local
            .with_ymd_and_hms(2026, 3, 24, 12, 0, 0)
            .single()
            .expect("local datetime");
        let ready_report = report_with_usage("codex", "Codex", now.with_timezone(&Utc));
        let session_only_report = report_with_sessions_only("cursor", "Cursor");
        let unavailable_report = report_without_usage_or_sessions("antigravity", "Antigravity");

        let snapshot = build_dashboard_snapshot_from_reports(
            vec![ready_report, session_only_report, unavailable_report],
            now,
        );
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(
            source_row_json(&json, "codex").get("analyticsState").and_then(|value| value.as_str()),
            Some("ready")
        );
        assert_eq!(
            source_row_json(&json, "cursor").get("analyticsState").and_then(|value| value.as_str()),
            Some("session_only")
        );
        assert_eq!(
            source_row_json(&json, "antigravity")
                .get("analyticsState")
                .and_then(|value| value.as_str()),
            Some("unavailable")
        );
    }

    #[test]
    fn session_only_rows_keep_quantitative_metrics_null() {
        let now = Local
            .with_ymd_and_hms(2026, 3, 24, 12, 0, 0)
            .single()
            .expect("local datetime");
        let snapshot = build_dashboard_snapshot_from_reports(
            vec![report_with_sessions_only("cursor", "Cursor")],
            now,
        );
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
        let row = source_row_json(&json, "cursor");

        assert!(row.get("tokens").is_some_and(serde_json::Value::is_null));
        assert!(row.get("costUsd").is_some_and(serde_json::Value::is_null));
        assert!(row.get("sessions").is_some_and(serde_json::Value::is_null));
        assert!(row.get("trend").is_some_and(serde_json::Value::is_null));
        assert!(row
            .get("pricingCoverage")
            .is_some_and(serde_json::Value::is_null));
    }

    #[test]
    fn source_detail_uses_null_summaries_when_analytics_are_pending() {
        let now = Local
            .with_ymd_and_hms(2026, 3, 24, 12, 0, 0)
            .single()
            .expect("local datetime");
        let snapshot = build_source_snapshot_from_reports(
            &[report_with_sessions_only("cursor", "Cursor")],
            now,
            "cursor",
        )
        .expect("source snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert!(json.get("todaySummary").is_some_and(serde_json::Value::is_null));
        assert!(json.get("last7dSummary").is_some_and(serde_json::Value::is_null));
        assert!(json.get("last30dSummary").is_some_and(serde_json::Value::is_null));
        assert!(json.get("lifetimeSummary").is_some_and(serde_json::Value::is_null));
    }

    #[test]
    fn source_detail_preserves_zero_for_ready_but_idle_windows() {
        let now = Local
            .with_ymd_and_hms(2026, 3, 24, 12, 0, 0)
            .single()
            .expect("local datetime");
        let occurred_at = (now - Duration::days(20)).with_timezone(&Utc);
        let snapshot = build_source_snapshot_from_reports(
            &[report_with_usage("codex", "Codex", occurred_at)],
            now,
            "codex",
        )
        .expect("source snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(
            json.get("last7dSummary")
                .and_then(|value| value.get("tokens"))
                .and_then(|value| value.as_u64()),
            Some(0)
        );
    }

    #[test]
    fn source_rows_expose_row_level_pricing_coverage() {
        let now = Local
            .with_ymd_and_hms(2026, 3, 24, 12, 0, 0)
            .single()
            .expect("local datetime");
        let snapshot = build_dashboard_snapshot_from_reports(
            vec![report_with_usage("codex", "Codex", now.with_timezone(&Utc))],
            now,
        );
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(
            source_row_json(&json, "codex")
                .get("pricingCoverage")
                .and_then(|value| value.as_str()),
            Some("actual")
        );
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

    fn report_with_usage(id: &str, name: &str, occurred_at: chrono::DateTime<Utc>) -> SourceReport {
        SourceReport {
            status: ready_status(id, name),
            usage_events: vec![UsageEvent {
                source_id: match id {
                    "codex" => "codex",
                    "cursor" => "cursor",
                    "antigravity" => "antigravity",
                    other => panic!("unsupported source id: {other}"),
                },
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
                session_id: format!("{id}-session"),
            }],
            sessions: vec![SessionRecord {
                updated_at: occurred_at,
                summary: SessionSummary {
                    id: format!("{id}-session"),
                    source_id: id.into(),
                    title: "Session".into(),
                    preview: "Preview".into(),
                    source: name.into(),
                    workspace: "burned".into(),
                    model: "gpt-5.4".into(),
                    started_at: "Mar 24 12:00".into(),
                    total_tokens: 1_750,
                    cost_usd: 0.0,
                    calculation_method: CalculationMethod::Native,
                    status: "indexed".into(),
                },
            }],
        }
    }

    fn report_with_sessions_only(id: &str, name: &str) -> SourceReport {
        let updated_at = Utc
            .with_ymd_and_hms(2026, 3, 24, 16, 0, 0)
            .single()
            .expect("utc datetime");
        SourceReport {
            status: ready_status(id, name),
            usage_events: Vec::new(),
            sessions: vec![SessionRecord {
                updated_at,
                summary: SessionSummary {
                    id: format!("{id}-session"),
                    source_id: id.into(),
                    title: "Session".into(),
                    preview: "Preview".into(),
                    source: name.into(),
                    workspace: "burned".into(),
                    model: "unknown".into(),
                    started_at: "Mar 24 12:00".into(),
                    total_tokens: 0,
                    cost_usd: 0.0,
                    calculation_method: CalculationMethod::Estimated,
                    status: "indexed".into(),
                },
            }],
        }
    }

    fn report_without_usage_or_sessions(id: &str, name: &str) -> SourceReport {
        let mut status = ready_status(id, name);
        status.state = SourceState::Partial;
        status.session_count = None;

        SourceReport {
            status,
            usage_events: Vec::new(),
            sessions: Vec::new(),
        }
    }

    fn source_row_json<'a>(snapshot_json: &'a serde_json::Value, source_id: &str) -> &'a serde_json::Value {
        snapshot_json
            .get("sources")
            .and_then(|value| value.as_array())
            .and_then(|rows| {
                rows.iter().find(|row| {
                    row.get("sourceId")
                        .and_then(|value| value.as_str())
                        == Some(source_id)
                })
            })
            .expect("source row")
    }

    fn approx_eq(left: f64, right: f64) {
        let delta = (left - right).abs();
        assert!(delta < 1e-9, "left={left}, right={right}, delta={delta}");
    }
}
