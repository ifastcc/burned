mod connectors;
mod models;
mod pricing;
mod settings;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Duration, Local, NaiveDate, Timelike, Utc};
use chrono_tz::Tz;
use serde_json::Result as JsonResult;

use connectors::{collect_all, collect_all_with_progress, SourceReport};
pub use models::DashboardSnapshot;
use models::{
    CalculationMethod, DailyUsagePoint, SessionGroup, SessionSummary, SourceDetailSnapshot,
    SourceStatus, SourceUsage,
};
pub use settings::AppSettings;

#[tauri::command]
fn get_dashboard_snapshot(time_zone: Option<String>) -> DashboardSnapshot {
    build_dashboard_snapshot(time_zone.as_deref())
}

#[tauri::command]
fn get_source_snapshot(
    source_id: String,
    time_zone: Option<String>,
) -> Result<SourceDetailSnapshot, String> {
    build_source_snapshot(&source_id, time_zone.as_deref())
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

#[derive(Clone, Copy, Debug)]
enum SnapshotTimeZone {
    Named(Tz),
    SystemLocal,
}

impl SnapshotTimeZone {
    fn resolve(requested_time_zone: Option<&str>) -> Self {
        requested_time_zone
            .and_then(|value| value.parse::<Tz>().ok())
            .map(Self::Named)
            .unwrap_or(Self::SystemLocal)
    }

    fn today(self, now: DateTime<Utc>) -> NaiveDate {
        match self {
            SnapshotTimeZone::Named(time_zone) => now.with_timezone(&time_zone).date_naive(),
            SnapshotTimeZone::SystemLocal => now.with_timezone(&Local).date_naive(),
        }
    }

    fn local_day(self, at: DateTime<Utc>) -> NaiveDate {
        match self {
            SnapshotTimeZone::Named(time_zone) => at.with_timezone(&time_zone).date_naive(),
            SnapshotTimeZone::SystemLocal => at.with_timezone(&Local).date_naive(),
        }
    }

    fn headline_date(self, now: DateTime<Utc>) -> String {
        match self {
            SnapshotTimeZone::Named(time_zone) => now.with_timezone(&time_zone).format("%B %-d, %Y").to_string(),
            SnapshotTimeZone::SystemLocal => now.with_timezone(&Local).format("%B %-d, %Y").to_string(),
        }
    }

    fn elapsed_hours(self, now: DateTime<Utc>) -> f64 {
        let (hour, minute) = match self {
            SnapshotTimeZone::Named(time_zone) => {
                let local = now.with_timezone(&time_zone);
                (local.hour(), local.minute())
            }
            SnapshotTimeZone::SystemLocal => {
                let local = now.with_timezone(&Local);
                (local.hour(), local.minute())
            }
        };

        ((hour as f64) + (minute as f64 / 60.0)).max(1.0)
    }
}

pub fn build_dashboard_snapshot(time_zone: Option<&str>) -> DashboardSnapshot {
    let now = Utc::now();
    let snapshot_time_zone = SnapshotTimeZone::resolve(time_zone);
    let reports = collect_all();
    build_dashboard_snapshot_from_reports(reports, now, snapshot_time_zone)
}

pub fn build_source_snapshot(
    source_id: &str,
    time_zone: Option<&str>,
) -> Result<SourceDetailSnapshot, String> {
    let now = Utc::now();
    let snapshot_time_zone = SnapshotTimeZone::resolve(time_zone);
    let reports = collect_all();
    build_source_snapshot_from_reports(&reports, now, snapshot_time_zone, source_id)
        .ok_or_else(|| format!("Source `{source_id}` was not found"))
}

pub fn build_dashboard_snapshot_with_progress<F>(
    on_progress: F,
    time_zone: Option<&str>,
) -> DashboardSnapshot
where
    F: FnMut(usize, usize, &str),
{
    let now = Utc::now();
    let snapshot_time_zone = SnapshotTimeZone::resolve(time_zone);
    let reports = collect_all_with_progress(on_progress);
    build_dashboard_snapshot_from_reports(reports, now, snapshot_time_zone)
}

fn build_dashboard_snapshot_from_reports(
    reports: Vec<SourceReport>,
    now: DateTime<Utc>,
    snapshot_time_zone: SnapshotTimeZone,
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
    let today = snapshot_time_zone.today(now);

    let total_tokens_today = usage_events
        .iter()
        .filter(|event| snapshot_time_zone.local_day(event.occurred_at) == today)
        .map(|event| event.total_tokens)
        .sum::<u64>();
    let total_cost_today = usage_events
        .iter()
        .filter(|event| snapshot_time_zone.local_day(event.occurred_at) == today)
        .map(|event| event.estimated_cost_usd().unwrap_or(0.0))
        .sum::<f64>();

    let total_native_today = usage_events
        .iter()
        .filter(|event| {
            snapshot_time_zone.local_day(event.occurred_at) == today
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
        .filter(|event| snapshot_time_zone.local_day(event.occurred_at) == today)
        .map(|event| event.source_id)
        .collect::<HashSet<_>>()
        .len() as u16;

    let elapsed_hours = snapshot_time_zone.elapsed_hours(now);
    let burn_rate_per_hour = (total_tokens_today as f64 / elapsed_hours).round() as u64;

    let week = build_weekly_usage(&usage_events, now, snapshot_time_zone);
    let daily_history = build_daily_history(&usage_events, now, 180, snapshot_time_zone);
    let sources = build_source_usage(&reports, &source_names, now, snapshot_time_zone);
    let sessions = build_recent_sessions(&reports);
    let session_groups = build_session_groups(&reports);

    DashboardSnapshot {
        headline_date: snapshot_time_zone.headline_date(now),
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
    now: DateTime<Utc>,
    snapshot_time_zone: SnapshotTimeZone,
    source_id: &str,
) -> Option<SourceDetailSnapshot> {
    let report = reports.iter().find(|report| report.status.id == source_id)?;
    let usage_events = report.usage_events.iter().collect::<Vec<_>>();
    let week = build_weekly_usage(&usage_events, now, snapshot_time_zone);
    let daily_history = build_daily_history(&usage_events, now, 30, snapshot_time_zone);
    let today = week.last().cloned().unwrap_or(DailyUsagePoint {
        date: snapshot_time_zone.today(now).format("%Y-%m-%d").to_string(),
        total_tokens: 0,
        total_cost_usd: 0.0,
        exact_share: 0.0,
        active_sources: 0,
        session_count: 0,
    });
    let session_costs = build_session_costs(reports);
    let mut sessions = report.sessions.iter().collect::<Vec<_>>();
    sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));

    Some(SourceDetailSnapshot {
        source_id: report.status.id.clone(),
        source_name: report.status.name.clone(),
        status: report.status.clone(),
        calculation_mix: report_calculation_mix(report),
        today_tokens: today.total_tokens,
        today_cost_usd: today.total_cost_usd,
        week,
        daily_history,
        sessions: sessions
            .into_iter()
            .map(|record| attach_session_cost(&record.summary, &session_costs))
            .collect(),
    })
}

pub fn build_dashboard_snapshot_json(time_zone: Option<&str>) -> JsonResult<String> {
    serde_json::to_string(&build_dashboard_snapshot(time_zone))
}

pub fn build_source_snapshot_json(
    source_id: &str,
    time_zone: Option<&str>,
) -> Result<String, String> {
    build_source_snapshot(source_id, time_zone)
        .and_then(|snapshot| serde_json::to_string(&snapshot).map_err(|error| error.to_string()))
}

pub fn build_dashboard_snapshot_json_with_progress<F>(
    on_progress: F,
    time_zone: Option<&str>,
) -> JsonResult<String>
where
    F: FnMut(usize, usize, &str),
{
    serde_json::to_string(&build_dashboard_snapshot_with_progress(on_progress, time_zone))
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

fn build_weekly_usage(
    usage_events: &[&connectors::UsageEvent],
    now: DateTime<Utc>,
    snapshot_time_zone: SnapshotTimeZone,
) -> Vec<DailyUsagePoint> {
    build_usage_window(usage_events, now, 7, snapshot_time_zone)
}

fn build_daily_history(
    usage_events: &[&connectors::UsageEvent],
    now: DateTime<Utc>,
    day_count: usize,
    snapshot_time_zone: SnapshotTimeZone,
) -> Vec<DailyUsagePoint> {
    build_usage_window(usage_events, now, day_count, snapshot_time_zone)
}

fn build_usage_window(
    usage_events: &[&connectors::UsageEvent],
    now: DateTime<Utc>,
    day_count: usize,
    snapshot_time_zone: SnapshotTimeZone,
) -> Vec<DailyUsagePoint> {
    let mut totals = HashMap::<String, (u64, u64, f64)>::new();
    for event in usage_events {
        let key = snapshot_time_zone
            .local_day(event.occurred_at)
            .format("%Y-%m-%d")
            .to_string();
        let entry = totals.entry(key).or_insert((0, 0, 0.0));
        entry.0 += event.total_tokens;
        if event.calculation_method == CalculationMethod::Native {
            entry.1 += event.total_tokens;
        }
        entry.2 += event.estimated_cost_usd().unwrap_or(0.0);
    }

    (0..day_count)
        .map(|offset| snapshot_time_zone.today(now) - Duration::days((day_count - 1 - offset) as i64))
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
                active_sources: count_active_sources_for_day(usage_events, day, snapshot_time_zone),
                session_count: count_sessions_for_day(usage_events, day, snapshot_time_zone),
            }
        })
        .collect()
}

fn build_source_usage(
    reports: &[SourceReport],
    source_names: &HashMap<String, String>,
    now: DateTime<Utc>,
    snapshot_time_zone: SnapshotTimeZone,
) -> Vec<SourceUsage> {
    let today = snapshot_time_zone.today(now);
    let yesterday = today - Duration::days(1);
    let mut usage_by_source =
        HashMap::<String, (u64, u64, f64, HashSet<String>, HashSet<String>)>::new();

    for report in reports {
        for event in &report.usage_events {
            let local_day = snapshot_time_zone.local_day(event.occurred_at);
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
            let (today_tokens, yesterday_tokens, today_cost_usd, today_sessions, _) = usage_by_source
                .remove(&report.status.id)
                .unwrap_or((0, 0, 0.0, HashSet::new(), HashSet::new()));
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
    snapshot_time_zone: SnapshotTimeZone,
) -> u16 {
    usage_events
        .iter()
        .filter(|event| snapshot_time_zone.local_day(event.occurred_at) == day)
        .map(|event| event.source_id)
        .collect::<HashSet<_>>()
        .len() as u16
}

fn count_sessions_for_day(
    usage_events: &[&connectors::UsageEvent],
    day: chrono::NaiveDate,
    snapshot_time_zone: SnapshotTimeZone,
) -> u32 {
    usage_events
        .iter()
        .filter(|event| snapshot_time_zone.local_day(event.occurred_at) == day)
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
    use chrono::TimeZone;

    use crate::connectors::{SessionRecord, SourceReport, UsageEvent};
    use crate::models::{SourceState, SourceStatus};
    use crate::pricing::TokenBreakdown;

    #[test]
    fn dashboard_snapshot_rolls_up_estimated_costs_across_views() {
        let now = Utc
            .with_ymd_and_hms(2026, 3, 24, 16, 0, 0)
            .single()
            .expect("utc datetime");
        let occurred_at = now;
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

        let snapshot = build_dashboard_snapshot_from_reports(
            vec![report],
            now,
            SnapshotTimeZone::Named("Asia/Shanghai".parse::<Tz>().expect("time zone")),
        );

        approx_eq(snapshot.total_cost_today, expected_cost);
        approx_eq(snapshot.sources[0].cost_usd, expected_cost);
        approx_eq(snapshot.sessions[0].cost_usd, expected_cost);
        approx_eq(snapshot.week[6].total_cost_usd, expected_cost);
        approx_eq(snapshot.daily_history.last().expect("daily point").total_cost_usd, expected_cost);
    }

    #[test]
    fn unsupported_models_keep_cost_pending() {
        let now = Utc
            .with_ymd_and_hms(2026, 3, 24, 16, 0, 0)
            .single()
            .expect("utc datetime");
        let occurred_at = now;
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

        let snapshot = build_dashboard_snapshot_from_reports(
            vec![report],
            now,
            SnapshotTimeZone::Named("Asia/Shanghai".parse::<Tz>().expect("time zone")),
        );

        assert_eq!(snapshot.total_tokens_today, 8_000);
        approx_eq(snapshot.total_cost_today, 0.0);
        approx_eq(snapshot.sources[0].cost_usd, 0.0);
        approx_eq(snapshot.sessions[0].cost_usd, 0.0);
    }

    #[test]
    fn source_detail_snapshot_rolls_up_source_history_and_costs() {
        let now = Utc
            .with_ymd_and_hms(2026, 3, 24, 16, 0, 0)
            .single()
            .expect("utc datetime");
        let occurred_at = now;
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

        let snapshot = build_source_snapshot_from_reports(
            &[report],
            now,
            SnapshotTimeZone::Named("Asia/Shanghai".parse::<Tz>().expect("time zone")),
            "codex",
        )
        .expect("source snapshot");

        assert_eq!(snapshot.source_id, "codex");
        approx_eq(snapshot.today_cost_usd, expected_cost);
        approx_eq(snapshot.week[6].total_cost_usd, expected_cost);
        approx_eq(snapshot.sessions[0].cost_usd, expected_cost);
    }

    #[test]
    fn dashboard_snapshot_uses_requested_time_zone_for_day_boundaries() {
        let now = Utc
            .with_ymd_and_hms(2026, 3, 24, 16, 30, 0)
            .single()
            .expect("utc datetime");
        let occurred_at = Utc
            .with_ymd_and_hms(2026, 3, 24, 16, 10, 0)
            .single()
            .expect("utc datetime");
        let report = SourceReport {
            status: ready_status("codex", "Codex"),
            usage_events: vec![UsageEvent {
                source_id: "codex",
                occurred_at,
                model: "gpt-5.4".into(),
                token_breakdown: TokenBreakdown {
                    input_tokens: 1_000,
                    output_tokens: 500,
                    ..TokenBreakdown::default()
                },
                total_tokens: 1_500,
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
                    started_at: "Mar 25 00:10".into(),
                    total_tokens: 1_500,
                    cost_usd: 0.0,
                    calculation_method: CalculationMethod::Native,
                    status: "indexed".into(),
                },
            }],
        };

        let snapshot = build_dashboard_snapshot_from_reports(
            vec![report],
            now,
            SnapshotTimeZone::Named("Asia/Shanghai".parse::<Tz>().expect("time zone")),
        );

        assert_eq!(snapshot.total_tokens_today, 1_500);
        assert_eq!(snapshot.week.last().expect("daily point").date, "2026-03-25");
        assert_eq!(snapshot.week.last().expect("daily point").total_tokens, 1_500);
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

    fn approx_eq(left: f64, right: f64) {
        let delta = (left - right).abs();
        assert!(delta < 1e-9, "left={left}, right={right}, delta={delta}");
    }
}
