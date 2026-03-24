mod connectors;
mod models;
mod settings;

use std::collections::{HashMap, HashSet};

use chrono::{Duration, Local, Timelike};
use serde_json::Result as JsonResult;

use connectors::{collect_all, collect_all_with_progress, SourceReport};
pub use models::DashboardSnapshot;
use models::{
    CalculationMethod, DailyUsagePoint, SessionGroup, SessionSummary, SourceStatus, SourceUsage,
};
pub use settings::AppSettings;

#[tauri::command]
fn get_dashboard_snapshot() -> DashboardSnapshot {
    build_dashboard_snapshot()
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
        total_cost_today: 0.0,
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

pub fn build_dashboard_snapshot_json() -> JsonResult<String> {
    serde_json::to_string(&build_dashboard_snapshot())
}

pub fn build_dashboard_snapshot_json_with_progress<F>(on_progress: F) -> JsonResult<String>
where
    F: FnMut(usize, usize, &str),
{
    serde_json::to_string(&build_dashboard_snapshot_with_progress(on_progress))
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
    let mut totals = HashMap::<String, (u64, u64)>::new();
    for event in usage_events {
        let local_time = event.occurred_at.with_timezone(&Local);
        let key = local_time.date_naive().format("%Y-%m-%d").to_string();
        let entry = totals.entry(key).or_insert((0, 0));
        entry.0 += event.total_tokens;
        if event.calculation_method == CalculationMethod::Native {
            entry.1 += event.total_tokens;
        }
    }

    (0..7)
        .map(|offset| now.date_naive() - Duration::days((6 - offset) as i64))
        .map(|day| {
            let key = day.format("%Y-%m-%d").to_string();
            let (total_tokens, exact_tokens) = totals.get(&key).copied().unwrap_or((0, 0));
            let exact_share = if total_tokens == 0 {
                0.0
            } else {
                exact_tokens as f64 / total_tokens as f64
            };

            DailyUsagePoint {
                date: day.format("%Y-%m-%d").to_string(),
                total_tokens,
                total_cost_usd: 0.0,
                exact_share,
                active_sources: count_active_sources_for_day(usage_events, day),
                session_count: count_sessions_for_day(usage_events, day),
            }
        })
        .collect()
}

fn build_daily_history(
    usage_events: &[&connectors::UsageEvent],
    now: chrono::DateTime<Local>,
    day_count: usize,
) -> Vec<DailyUsagePoint> {
    let mut totals = HashMap::<String, (u64, u64)>::new();
    for event in usage_events {
        let local_time = event.occurred_at.with_timezone(&Local);
        let key = local_time.date_naive().format("%Y-%m-%d").to_string();
        let entry = totals.entry(key).or_insert((0, 0));
        entry.0 += event.total_tokens;
        if event.calculation_method == CalculationMethod::Native {
            entry.1 += event.total_tokens;
        }
    }

    (0..day_count)
        .map(|offset| now.date_naive() - Duration::days((day_count - 1 - offset) as i64))
        .map(|day| {
            let key = day.format("%Y-%m-%d").to_string();
            let (total_tokens, exact_tokens) = totals.get(&key).copied().unwrap_or((0, 0));
            let exact_share = if total_tokens == 0 {
                0.0
            } else {
                exact_tokens as f64 / total_tokens as f64
            };

            DailyUsagePoint {
                date: key,
                total_tokens,
                total_cost_usd: 0.0,
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
        HashMap::<String, (u64, u64, HashSet<String>, HashSet<String>)>::new();

    for report in reports {
        for event in &report.usage_events {
            let local_day = event.occurred_at.with_timezone(&Local).date_naive();
            let entry = usage_by_source
                .entry(event.source_id.to_string())
                .or_insert((0, 0, HashSet::new(), HashSet::new()));
            if local_day == today {
                entry.0 += event.total_tokens;
                entry.2.insert(event.session_id.clone());
            } else if local_day == yesterday {
                entry.1 += event.total_tokens;
                entry.3.insert(event.session_id.clone());
            }
        }
    }

    reports
        .iter()
        .filter(|report| !matches!(report.status.state, models::SourceState::Missing))
        .map(|report| {
            let (today_tokens, yesterday_tokens, today_sessions, _) = usage_by_source
                .remove(&report.status.id)
                .unwrap_or((0, 0, HashSet::new(), HashSet::new()));
            let trend = if today_tokens > yesterday_tokens + (yesterday_tokens / 20).max(1) {
                "up"
            } else if yesterday_tokens > today_tokens + (today_tokens / 20).max(1) {
                "down"
            } else {
                "flat"
            };

            let calculation_mix = if report.usage_events.is_empty() {
                "estimated".into()
            } else {
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
            };

            SourceUsage {
                source: source_names
                    .get(&report.status.id)
                    .cloned()
                    .unwrap_or_else(|| report.status.name.clone()),
                tokens: today_tokens,
                cost_usd: 0.0,
                sessions: today_sessions.len() as u32,
                trend: trend.into(),
                calculation_mix,
            }
        })
        .collect()
}

fn build_recent_sessions(reports: &[SourceReport]) -> Vec<SessionSummary> {
    let mut sessions = reports
        .iter()
        .flat_map(|report| report.sessions.iter())
        .collect::<Vec<_>>();
    sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    sessions
        .into_iter()
        .take(8)
        .map(|record| record.summary.clone())
        .collect()
}

fn build_session_groups(reports: &[SourceReport]) -> Vec<SessionGroup> {
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
                    .map(|record| record.summary.clone())
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
            get_app_settings,
            set_cherry_backup_dir,
            clear_cherry_backup_dir
        ])
        .run(tauri::generate_context!())
        .expect("error while running Burned");
}
