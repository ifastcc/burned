use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Local, TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use walkdir::WalkDir;

use crate::connectors::{
    report_scan_detail, SessionRecord, SourceConnector, SourceReport, UsageEvent,
};
use crate::models::{CalculationMethod, PricingCoverage, SessionSummary, SourceState, SourceStatus};
use crate::pricing::TokenBreakdown;

const SOURCE_ID: &str = "codex";
const SOURCE_NAME: &str = "Codex";

pub struct CodexConnector;

#[derive(Clone, Copy, Default, Eq, PartialEq)]
struct RawUsage {
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    reasoning_output_tokens: u64,
    total_tokens: u64,
}

impl RawUsage {
    fn token_breakdown(&self) -> TokenBreakdown {
        // Codex session JSONL follows the OpenAI usage shape where cached input
        // and reasoning tokens are detail buckets within the main input/output totals.
        let cached_input_tokens = self.cached_input_tokens.min(self.input_tokens);
        let input_tokens = self.input_tokens.saturating_sub(cached_input_tokens);
        let output_tokens = self.output_tokens;
        let classified_tokens = input_tokens
            .saturating_add(cached_input_tokens)
            .saturating_add(output_tokens);

        TokenBreakdown {
            input_tokens,
            cache_creation_input_tokens: 0,
            cached_input_tokens,
            output_tokens,
            other_tokens: self.total_tokens.saturating_sub(classified_tokens),
        }
    }
}

impl SourceConnector for CodexConnector {
    fn collect(&self) -> SourceReport {
        collect_codex().unwrap_or_else(|error| SourceReport {
            status: SourceStatus {
                id: SOURCE_ID.into(),
                name: SOURCE_NAME.into(),
                state: SourceState::Partial,
                capabilities: vec![
                    "local-sqlite".into(),
                    "native-tokens".into(),
                    "session-jsonl".into(),
                    "log-ingestion".into(),
                ],
                note: format!("Codex detected, but ingestion failed: {error}"),
                local_path: codex_home().map(display_path),
                session_count: None,
                last_seen_at: None,
            },
            usage_events: Vec::new(),
            sessions: Vec::new(),
        })
    }
}

fn collect_codex() -> Result<SourceReport> {
    let Some(home) = codex_home() else {
        return Ok(missing_report());
    };

    if !home.exists() {
        return Ok(missing_report());
    }

    let latest_state_db = latest_matching_file(&home, "state_", ".sqlite");
    let log_dbs = matching_files(&home, "logs_", ".sqlite");
    let sessions_root = home.join("sessions");

    let mut status = SourceStatus {
        id: SOURCE_ID.into(),
        name: SOURCE_NAME.into(),
        state: SourceState::Partial,
        capabilities: vec![
            "local-sqlite".into(),
            "native-tokens".into(),
            "session-jsonl".into(),
            "log-ingestion".into(),
            "app-server-events".into(),
        ],
        note: "Codex home found, but no usable session store was detected.".into(),
        local_path: Some(display_path(home.clone())),
        session_count: None,
        last_seen_at: latest_state_db
            .as_ref()
            .and_then(|path| format_mtime(path).ok()),
    };

    let mut sessions = Vec::new();
    let mut usage_events = Vec::new();

    if let Some(state_db) = latest_state_db.as_ref() {
        let connection = open_read_only(state_db)?;
        status.session_count = query_count(&connection, "select count(*) from threads").ok();
        status.last_seen_at = query_latest_thread_update(&connection)
            .ok()
            .flatten()
            .map(format_timestamp);
        sessions = query_recent_sessions(&connection)?;
    }

    if sessions_root.exists() {
        usage_events = query_usage_events_from_session_files(&sessions_root)?;
    } else if !log_dbs.is_empty() {
        usage_events = query_usage_events(&log_dbs)?;
    }

    if !sessions.is_empty() || !usage_events.is_empty() {
        status.state = SourceState::Ready;
        status.note =
            "Native session totals and per-turn usage events are active from Codex session JSONL. App-server live updates can be added next."
                .into();
    } else if latest_state_db.is_some() {
        status.note =
            "Session metadata exists, but no recent usage events were found in Codex session JSONL."
                .into();
    }

    Ok(SourceReport {
        status,
        usage_events,
        sessions,
    })
}

fn missing_report() -> SourceReport {
    SourceReport {
        status: SourceStatus {
            id: SOURCE_ID.into(),
            name: SOURCE_NAME.into(),
            state: SourceState::Missing,
            capabilities: vec![
                "local-sqlite".into(),
                "native-tokens".into(),
                "session-jsonl".into(),
                "log-ingestion".into(),
            ],
            note: "No Codex home directory was found on this machine.".into(),
            local_path: codex_home().map(display_path),
            session_count: None,
            last_seen_at: None,
        },
        usage_events: Vec::new(),
        sessions: Vec::new(),
    }
}

fn query_recent_sessions(connection: &Connection) -> Result<Vec<SessionRecord>> {
    let mut statement = connection.prepare(
        "select id, created_at, updated_at, tokens_used, coalesce(model, ''), coalesce(cwd, ''), \
         coalesce(title, ''), coalesce(first_user_message, '') \
         from threads order by updated_at desc limit 12",
    )?;

    let rows = statement.query_map([], |row| {
        let id: String = row.get(0)?;
        let created_at: i64 = row.get(1)?;
        let updated_at: i64 = row.get(2)?;
        let total_tokens: i64 = row.get(3)?;
        let model: String = row.get(4)?;
        let cwd: String = row.get(5)?;
        let title: String = row.get(6)?;
        let first_user_message: String = row.get(7)?;

        Ok((
            id,
            created_at,
            updated_at,
            total_tokens,
            model,
            cwd,
            title,
            first_user_message,
        ))
    })?;

    let mut sessions = Vec::new();
    for row in rows {
        let (id, created_at, updated_at, total_tokens, model, cwd, title, first_user_message) =
            row?;
        let started_at = epoch_to_utc(created_at).unwrap_or_else(Utc::now);
        let updated_at_dt = epoch_to_utc(updated_at).unwrap_or(started_at);
        let started_local = started_at.with_timezone(&Local);
        let session_title = choose_title(&title, &first_user_message);
        let preview = make_preview(&first_user_message);

        sessions.push(SessionRecord {
            updated_at: updated_at_dt,
            summary: SessionSummary {
                id,
                source_id: SOURCE_ID.into(),
                title: session_title,
                preview,
                source: SOURCE_NAME.into(),
                workspace: workspace_name(&cwd),
                model: fallback_model(&model),
                started_at: started_local.format("%b %-d %H:%M").to_string(),
                total_tokens: total_tokens.max(0) as u64,
                cost_usd: 0.0,
                pricing_coverage: PricingCoverage::Pending,
                long_context: None,
                calculation_method: CalculationMethod::Native,
                status: "indexed".into(),
            },
        });
    }

    Ok(sessions)
}

fn query_usage_events(log_dbs: &[PathBuf]) -> Result<Vec<UsageEvent>> {
    let cutoff = Utc::now().timestamp() - 180 * 24 * 60 * 60;
    let mut seen = HashSet::new();
    let mut events = Vec::new();

    for log_db in log_dbs {
        let connection = open_read_only(log_db)?;
        let mut statement = connection.prepare(
            "select ts, feedback_log_body from logs \
             where ts >= ?1 \
             and feedback_log_body like '%response.completed%' \
             and feedback_log_body like '%input_token_count=%'",
        )?;

        let rows = statement.query_map([cutoff], |row| {
            let ts: i64 = row.get(0)?;
            let body: String = row.get(1)?;
            Ok((ts, body))
        })?;

        for row in rows {
            let (ts, body) = row?;
            if let Some(event) = parse_usage_event(ts, &body) {
                let fingerprint = format!(
                    "{}|{}|{}",
                    event.session_id,
                    event.occurred_at.timestamp_millis(),
                    event.total_tokens
                );
                if seen.insert(fingerprint) {
                    events.push(event);
                }
            }
        }
    }

    Ok(events)
}

fn query_usage_events_from_session_files(sessions_root: &Path) -> Result<Vec<UsageEvent>> {
    let cutoff = Utc::now() - chrono::Duration::days(180);
    let mut events = Vec::new();
    let session_files = WalkDir::new(sessions_root)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("jsonl"))
        .map(|entry| entry.into_path())
        .collect::<Vec<_>>();
    let total_files = session_files.len();

    for (index, path) in session_files.iter().enumerate() {
        if total_files > 0
            && (index == 0 || index + 1 == total_files || (index + 1) % 50 == 0)
        {
            report_scan_detail(
                SOURCE_NAME,
                format!("Session files {}/{}", index + 1, total_files),
            );
        }
        let contents = fs::read_to_string(path)
            .with_context(|| format!("read Codex session file {}", path.display()))?;
        let fallback_session_id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("unknown");

        events.extend(
            parse_session_usage_events(&contents, fallback_session_id)
                .into_iter()
                .filter(|event| event.occurred_at >= cutoff),
        );
    }

    events.sort_by(|left, right| left.occurred_at.cmp(&right.occurred_at));
    Ok(events)
}

fn parse_usage_event(ts: i64, body: &str) -> Option<UsageEvent> {
    let input = extract_u64(body, "input_token_count=").unwrap_or(0);
    let output = extract_u64(body, "output_token_count=").unwrap_or(0);
    let cached = extract_u64(body, "cached_token_count=").unwrap_or(0);
    let reasoning = extract_u64(body, "reasoning_token_count=").unwrap_or(0);
    // SQLite logs expose the full token total in `tool_token_count`, despite
    // the field name. Cached/reasoning counts are detail buckets within the
    // main input/output totals, just like the session JSONL path.
    let total = extract_u64(body, "tool_token_count=")
        .unwrap_or_else(|| input.saturating_add(output));
    let token_breakdown = RawUsage {
        input_tokens: input,
        cached_input_tokens: cached,
        output_tokens: output,
        reasoning_output_tokens: reasoning,
        total_tokens: total,
    }
    .token_breakdown();
    let total_tokens = token_breakdown.total_tokens();
    if total_tokens == 0 {
        return None;
    }

    let session_id = extract_token(body, "conversation.id=")?.to_string();
    let occurred_at = extract_token(body, "event.timestamp=")
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .or_else(|| epoch_to_utc(ts))?;
    let model = extract_token(body, "model=").unwrap_or("unknown");

    Some(UsageEvent {
        source_id: SOURCE_ID,
        occurred_at,
        model: model.to_string(),
        token_breakdown,
        total_tokens,
        calculation_method: CalculationMethod::Native,
        session_id,
    })
}

fn parse_session_usage_events(contents: &str, fallback_session_id: &str) -> Vec<UsageEvent> {
    let mut events = Vec::new();
    let mut session_id = fallback_session_id.to_string();
    let mut canonical_session_id: Option<String> = None;
    let mut session_model = String::from("unknown");
    let mut previous_totals: Option<RawUsage> = None;
    let mut previous_last_usage: Option<(DateTime<Utc>, RawUsage)> = None;

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let is_session_meta_line = trimmed.contains(r#""type":"session_meta""#);
        let is_turn_context_line = trimmed.contains(r#""type":"turn_context""#);
        let is_token_count_line =
            trimmed.contains(r#""type":"event_msg""#) && trimmed.contains(r#""token_count""#);
        if !is_session_meta_line && !is_turn_context_line && !is_token_count_line {
            continue;
        }

        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };

        let entry_type = value.get("type").and_then(Value::as_str).unwrap_or_default();
        if entry_type == "session_meta" {
            if let Some(meta_id) = value
                .get("payload")
                .and_then(|payload| payload.get("id"))
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            {
                let canonical = canonical_session_id
                    .get_or_insert_with(|| meta_id.to_string())
                    .clone();
                session_id = canonical;
            }
            continue;
        }

        if entry_type == "turn_context" {
            if let Some(model) = value
                .get("payload")
                .and_then(|payload| payload.get("model"))
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            {
                session_model = model.to_string();
            }
            continue;
        }

        if entry_type != "event_msg" {
            continue;
        }

        let payload = value.get("payload");
        if payload
            .and_then(|payload| payload.get("type"))
            .and_then(Value::as_str)
            != Some("token_count")
        {
            continue;
        }

        let Some(occurred_at) = value
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
            .map(|timestamp| timestamp.with_timezone(&Utc))
        else {
            continue;
        };

        let info = payload.and_then(|payload| payload.get("info"));
        let last_usage = info
            .and_then(|info| info.get("last_token_usage"))
            .and_then(normalize_raw_usage);
        let total_usage = info
            .and_then(|info| info.get("total_token_usage"))
            .and_then(normalize_raw_usage);

        let raw_usage = if let Some(total_usage) = total_usage {
            if previous_totals == Some(total_usage) {
                None
            } else {
                let delta = subtract_raw_usage(total_usage, previous_totals.as_ref());
                previous_totals = Some(total_usage);
                Some(delta)
            }
        } else if let Some(last_usage) = last_usage {
            if previous_last_usage == Some((occurred_at, last_usage)) {
                None
            } else {
                previous_last_usage = Some((occurred_at, last_usage));
                Some(last_usage)
            }
        } else {
            None
        };

        let Some(raw_usage) = raw_usage else {
            continue;
        };

        let token_breakdown = raw_usage.token_breakdown();
        let total_tokens = token_breakdown.total_tokens();
        if total_tokens == 0 {
            continue;
        }

        events.push(UsageEvent {
            source_id: SOURCE_ID,
            occurred_at,
            model: session_model.clone(),
            token_breakdown,
            total_tokens,
            calculation_method: CalculationMethod::Native,
            session_id: session_id.clone(),
        });
    }

    events
}

fn normalize_raw_usage(value: &Value) -> Option<RawUsage> {
    let record = value.as_object()?;
    let input_tokens = record.get("input_tokens").and_then(number_from_value)?;
    let cached_input_tokens = record
        .get("cached_input_tokens")
        .and_then(number_from_value)
        .or_else(|| record.get("cache_read_input_tokens").and_then(number_from_value))
        .unwrap_or(0);
    let output_tokens = record
        .get("output_tokens")
        .and_then(number_from_value)
        .unwrap_or(0);
    let reasoning_output_tokens = record
        .get("reasoning_output_tokens")
        .and_then(number_from_value)
        .unwrap_or(0);
    let total_tokens = record
        .get("total_tokens")
        .and_then(number_from_value)
        .unwrap_or_else(|| {
            input_tokens
                .saturating_add(cached_input_tokens)
                .saturating_add(output_tokens)
                .saturating_add(reasoning_output_tokens)
        });

    Some(RawUsage {
        input_tokens,
        cached_input_tokens,
        output_tokens,
        reasoning_output_tokens,
        total_tokens,
    })
}

fn subtract_raw_usage(current: RawUsage, previous: Option<&RawUsage>) -> RawUsage {
    let previous = previous.copied().unwrap_or_default();

    RawUsage {
        input_tokens: current.input_tokens.saturating_sub(previous.input_tokens),
        cached_input_tokens: current
            .cached_input_tokens
            .saturating_sub(previous.cached_input_tokens),
        output_tokens: current.output_tokens.saturating_sub(previous.output_tokens),
        reasoning_output_tokens: current
            .reasoning_output_tokens
            .saturating_sub(previous.reasoning_output_tokens),
        total_tokens: current.total_tokens.saturating_sub(previous.total_tokens),
    }
}

fn number_from_value(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        value.as_i64()
            .and_then(|number| if number >= 0 { Some(number as u64) } else { None })
    })
}

fn query_latest_thread_update(connection: &Connection) -> Result<Option<DateTime<Utc>>> {
    let timestamp = connection
        .query_row("select max(updated_at) from threads", [], |row| {
            row.get::<_, Option<i64>>(0)
        })
        .context("query latest thread update")?;

    Ok(timestamp.and_then(epoch_to_utc))
}

fn query_count(connection: &Connection, sql: &str) -> Result<u32> {
    let count = connection.query_row(sql, [], |row| row.get::<_, i64>(0))?;
    Ok(count.max(0) as u32)
}

fn open_read_only(path: &Path) -> Result<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open sqlite database {}", path.display()))
}

fn codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".codex")))
}

fn latest_matching_file(dir: &Path, prefix: &str, suffix: &str) -> Option<PathBuf> {
    matching_files(dir, prefix, suffix)
        .into_iter()
        .max_by_key(|path| fs::metadata(path).and_then(|meta| meta.modified()).ok())
}

fn matching_files(dir: &Path, prefix: &str, suffix: &str) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .into_iter()
        .flat_map(|entries| entries.filter_map(std::result::Result::ok))
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with(prefix) && name.ends_with(suffix))
                .unwrap_or(false)
        })
        .collect()
}

fn workspace_name(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| "unknown".into())
}

fn fallback_model(model: &str) -> String {
    if model.is_empty() {
        "unknown".into()
    } else {
        model.into()
    }
}

fn extract_u64(haystack: &str, needle: &str) -> Option<u64> {
    extract_token(haystack, needle)?.parse().ok()
}

fn extract_token<'a>(haystack: &'a str, needle: &str) -> Option<&'a str> {
    let start = haystack.find(needle)? + needle.len();
    let tail = &haystack[start..];
    let end = tail
        .find(|character: char| {
            character.is_whitespace() || matches!(character, '}' | ',' | '"' | ']')
        })
        .unwrap_or(tail.len());
    Some(&tail[..end])
}

fn epoch_to_utc(seconds: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_opt(seconds, 0).single()
}

fn format_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

fn format_mtime(path: &Path) -> Result<String> {
    let modified = fs::metadata(path)?.modified()?;
    let modified: DateTime<Local> = modified.into();
    Ok(modified.format("%Y-%m-%d %H:%M").to_string())
}

fn display_path(path: PathBuf) -> String {
    path.display().to_string()
}

fn choose_title(title: &str, fallback: &str) -> String {
    let cleaned_title = clean_text(title);
    if !cleaned_title.is_empty() {
        return truncate(&cleaned_title, 72);
    }

    let cleaned_fallback = clean_text(fallback);
    if cleaned_fallback.is_empty() {
        "Untitled Codex session".into()
    } else {
        truncate(&cleaned_fallback, 72)
    }
}

fn make_preview(text: &str) -> String {
    let cleaned = clean_text(text);
    if cleaned.is_empty() {
        "No preview available.".into()
    } else {
        truncate(&cleaned, 180)
    }
}

fn clean_text(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let truncated = text
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>();
        format!("{truncated}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_last_token_usage_from_session_jsonl() {
        let contents = r#"{"timestamp":"2026-03-23T06:25:22.394Z","type":"session_meta","payload":{"id":"thread-123"}}
{"timestamp":"2026-03-23T06:41:09.961Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1200,"cached_input_tokens":200,"output_tokens":500,"reasoning_output_tokens":50,"total_tokens":1700}}}}"#;

        let events = parse_session_usage_events(contents, "fallback-id");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].session_id, "thread-123");
        assert_eq!(events[0].total_tokens, 1700);
        assert_eq!(events[0].token_breakdown.input_tokens, 1000);
        assert_eq!(events[0].token_breakdown.cached_input_tokens, 200);
        assert_eq!(events[0].token_breakdown.output_tokens, 500);
        assert_eq!(events[0].token_breakdown.other_tokens, 0);
        assert_eq!(
            events[0].occurred_at,
            DateTime::parse_from_rfc3339("2026-03-23T06:41:09.961Z")
                .unwrap()
                .with_timezone(&Utc)
        );
    }

    #[test]
    fn derives_usage_delta_from_total_token_usage() {
        let contents = r#"{"timestamp":"2026-03-23T06:25:22.394Z","type":"session_meta","payload":{"id":"thread-456"}}
{"timestamp":"2026-03-23T06:41:09.961Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1200,"cached_input_tokens":200,"output_tokens":500,"reasoning_output_tokens":50,"total_tokens":1700}}}}
{"timestamp":"2026-03-23T06:42:09.961Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":2000,"cached_input_tokens":300,"output_tokens":800,"reasoning_output_tokens":120,"total_tokens":2800}}}}"#;

        let events = parse_session_usage_events(contents, "fallback-id");

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].total_tokens, 1700);
        assert_eq!(events[1].session_id, "thread-456");
        assert_eq!(events[1].total_tokens, 1100);
        assert_eq!(events[1].token_breakdown.input_tokens, 700);
        assert_eq!(events[1].token_breakdown.cached_input_tokens, 100);
        assert_eq!(events[1].token_breakdown.output_tokens, 300);
        assert_eq!(events[1].token_breakdown.other_tokens, 0);
    }

    #[test]
    fn captures_turn_context_model_for_session_usage_events() {
        let contents = r#"{"timestamp":"2026-03-24T03:00:34.000Z","type":"session_meta","payload":{"id":"thread-789"}}
{"timestamp":"2026-03-24T03:00:35.000Z","type":"turn_context","payload":{"model":"gpt-5.4"}}
{"timestamp":"2026-03-24T03:01:09.961Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1200,"cached_input_tokens":200,"output_tokens":500,"reasoning_output_tokens":50,"total_tokens":1700}}}}"#;

        let events = parse_session_usage_events(contents, "fallback-id");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].model, "gpt-5.4");
        assert!(events[0].estimated_cost_usd().is_some());
    }

    #[test]
    fn parses_sqlite_log_usage_without_double_counting_detail_buckets() {
        let body = "conversation.id=thread-log \
event.timestamp=2026-03-23T06:41:09.961Z \
input_token_count=1200 \
cached_token_count=200 \
output_token_count=500 \
reasoning_token_count=50 \
tool_token_count=1700";

        let event = parse_usage_event(0, body).expect("log usage event");

        assert_eq!(event.session_id, "thread-log");
        assert_eq!(event.total_tokens, 1700);
        assert_eq!(event.token_breakdown.input_tokens, 1000);
        assert_eq!(event.token_breakdown.cached_input_tokens, 200);
        assert_eq!(event.token_breakdown.output_tokens, 500);
        assert_eq!(event.token_breakdown.other_tokens, 0);
    }

    #[test]
    fn keeps_first_session_meta_as_canonical_session_id() {
        let contents = r#"{"timestamp":"2026-03-24T03:00:34.000Z","type":"session_meta","payload":{"id":"thread-parent"}}
{"timestamp":"2026-03-24T03:00:35.000Z","type":"session_meta","payload":{"id":"thread-child"}}
{"timestamp":"2026-03-24T03:01:09.961Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1200,"cached_input_tokens":200,"output_tokens":500,"reasoning_output_tokens":50,"total_tokens":1700}}}}"#;

        let events = parse_session_usage_events(contents, "fallback-id");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].session_id, "thread-parent");
    }

    #[test]
    fn ignores_duplicate_total_usage_snapshots() {
        let contents = r#"{"timestamp":"2026-03-23T06:25:22.394Z","type":"session_meta","payload":{"id":"thread-456"}}
{"timestamp":"2026-03-23T06:41:09.961Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1200,"cached_input_tokens":200,"output_tokens":500,"reasoning_output_tokens":50,"total_tokens":1700}}}}
{"timestamp":"2026-03-23T06:42:09.961Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1200,"cached_input_tokens":200,"output_tokens":500,"reasoning_output_tokens":50,"total_tokens":1700}}}}"#;

        let events = parse_session_usage_events(contents, "fallback-id");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].total_tokens, 1700);
    }

    #[test]
    fn extracts_model_from_sqlite_log_usage() {
        let body = "turn{thread.id=thread-log model=gpt-5.4-mini}:run_turn event.timestamp=2026-03-23T06:41:09.961Z conversation.id=thread-log input_token_count=1200 cached_token_count=200 output_token_count=500 reasoning_token_count=50 tool_token_count=1700";

        let event = parse_usage_event(0, body).expect("log usage event");

        assert_eq!(event.model, "gpt-5.4-mini");
    }
}
