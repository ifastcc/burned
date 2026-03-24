use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Local, TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::connectors::{SessionRecord, SourceConnector, SourceReport, UsageEvent};
use crate::models::{
    CalculationMethod, PricingCoverage, SessionSummary, SourceState, SourceStatus,
};
use crate::pricing::TokenBreakdown;

const SOURCE_ID: &str = "cursor";
const SOURCE_NAME: &str = "Cursor";

pub struct CursorConnector;

impl SourceConnector for CursorConnector {
    fn collect(&self) -> SourceReport {
        collect_cursor().unwrap_or_else(|error| SourceReport {
            status: SourceStatus {
                id: SOURCE_ID.into(),
                name: SOURCE_NAME.into(),
                state: SourceState::Partial,
                capabilities: vec![
                    "local-sqlite".into(),
                    "session-index".into(),
                    "admin-api-tokens".into(),
                ],
                note: format!("Cursor detected, but discovery failed: {error}"),
                local_path: cursor_root().map(display_path),
                session_count: None,
                last_seen_at: None,
            },
            usage_events: Vec::new(),
            sessions: Vec::new(),
        })
    }
}

fn collect_cursor() -> Result<SourceReport> {
    let Some(root) = cursor_root() else {
        return Ok(missing_report());
    };

    let global_state = root.join("User").join("globalStorage").join("state.vscdb");
    let global_backup = root
        .join("User")
        .join("globalStorage")
        .join("state.vscdb.backup");
    let workspace_storage = root.join("User").join("workspaceStorage");
    let global_store = if global_backup.exists() {
        global_backup
    } else {
        global_state.clone()
    };
    if !global_store.exists() {
        return Ok(missing_report());
    }

    let connection = Connection::open_with_flags(&global_store, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let session_count = connection
        .query_row(
            "select count(*) from cursorDiskKV where key like 'composerData:%'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .ok()
        .map(|count| count.max(0) as u32);

    let workspace_count = fs::read_dir(&workspace_storage)
        .ok()
        .map(|entries| entries.filter_map(std::result::Result::ok).count() as u32);
    let workspace_map = composer_workspace_map(&workspace_storage);
    let (sessions, usage_events) = query_sessions(&connection, &workspace_map)?;

    Ok(SourceReport {
        status: SourceStatus {
            id: SOURCE_ID.into(),
            name: SOURCE_NAME.into(),
            state: SourceState::Partial,
            capabilities: vec![
                "local-sqlite".into(),
                "session-index".into(),
                "admin-api-tokens".into(),
            ],
            note: "Local SQLite history is available. Session titles and previews are parsed from composer data; native token usage should come from the team Admin API."
                .into(),
            local_path: Some(display_path(root)),
            session_count: session_count.or(workspace_count),
            last_seen_at: format_mtime(&global_store).ok(),
        },
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
                "session-index".into(),
                "admin-api-tokens".into(),
            ],
            note: "No Cursor workspace store was found on this machine.".into(),
            local_path: cursor_root().map(display_path),
            session_count: None,
            last_seen_at: None,
        },
        usage_events: Vec::new(),
        sessions: Vec::new(),
    }
}

fn cursor_root() -> Option<PathBuf> {
    dirs::home_dir().map(|home| {
        home.join("Library")
            .join("Application Support")
            .join("Cursor")
    })
}

fn format_mtime(path: &Path) -> Result<String> {
    let modified = fs::metadata(path)?.modified()?;
    let modified: DateTime<Local> = modified.into();
    Ok(modified.format("%Y-%m-%d %H:%M").to_string())
}

fn display_path(path: PathBuf) -> String {
    path.display().to_string()
}

fn query_sessions(
    connection: &Connection,
    workspace_map: &std::collections::HashMap<String, String>,
) -> Result<(Vec<SessionRecord>, Vec<UsageEvent>)> {
    let mut statement = connection.prepare(
        "select key, value from cursorDiskKV where key like 'composerData:%' and value <> ''",
    )?;
    let rows = statement.query_map([], |row| {
        let key: String = row.get(0)?;
        let value: String = row.get(1)?;
        Ok((key, value))
    })?;

    let mut sessions = Vec::new();
    let mut usage_events = Vec::new();
    for row in rows {
        let (key, raw_json) = row?;
        let composer_id = key.trim_start_matches("composerData:").to_string();
        let value: Value = match serde_json::from_str(&raw_json) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let workspace = workspace_map.get(&composer_id).cloned();
        if let Some((session, usage_event)) = parse_cursor_session(&composer_id, &value, workspace)
        {
            sessions.push(session);
            if let Some(usage_event) = usage_event {
                usage_events.push(usage_event);
            }
        }
    }

    sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    sessions.truncate(12);
    Ok((sessions, usage_events))
}

fn parse_cursor_session(
    composer_id: &str,
    value: &Value,
    workspace: Option<String>,
) -> Option<(SessionRecord, Option<UsageEvent>)> {
    let created_at = value
        .get("createdAt")
        .and_then(Value::as_i64)
        .and_then(epoch_millis_to_utc)
        .unwrap_or_else(Utc::now);
    let updated_at = value
        .get("lastUpdatedAt")
        .and_then(Value::as_i64)
        .and_then(epoch_millis_to_utc)
        .unwrap_or(created_at);
    let title = value
        .get("name")
        .and_then(Value::as_str)
        .map(normalize_text)
        .filter(|text| !text.is_empty())
        .or_else(|| {
            value
                .get("text")
                .and_then(Value::as_str)
                .map(normalize_text)
                .filter(|text| !text.is_empty())
        })
        .unwrap_or_else(|| "Untitled Cursor session".into());
    let preview = value
        .get("latestConversationSummary")
        .and_then(|summary| summary.get("summary"))
        .and_then(|summary| summary.get("summary"))
        .and_then(Value::as_str)
        .map(normalize_text)
        .filter(|text| !text.is_empty())
        .or_else(|| {
            value
                .get("text")
                .and_then(Value::as_str)
                .map(normalize_text)
                .filter(|text| !text.is_empty())
        })
        .unwrap_or_else(|| "No preview available.".into());
    let total_tokens = parse_cursor_session_token_total(value).unwrap_or(0);
    let parsed_cost_usd = value.get("usageData").and_then(parse_usage_data_cost_usd);
    let cost_usd = parsed_cost_usd.unwrap_or(0.0);
    let is_priced = parsed_cost_usd.is_some();

    let session = SessionRecord {
        updated_at,
        summary: SessionSummary {
            id: composer_id.to_string(),
            source_id: SOURCE_ID.into(),
            title: truncate(&title, 72),
            preview: truncate(&preview, 180),
            source: SOURCE_NAME.into(),
            workspace: workspace.unwrap_or_else(|| "unknown".into()),
            model: "unknown".into(),
            started_at: created_at
                .with_timezone(&Local)
                .format("%b %-d %H:%M")
                .to_string(),
            total_tokens,
            cost_usd,
            priced_sessions: if is_priced { 1 } else { 0 },
            pending_pricing_sessions: if is_priced { 0 } else { 1 },
            pricing_coverage: if is_priced {
                PricingCoverage::Actual
            } else {
                PricingCoverage::Pending
            },
            pricing_state: if is_priced {
                "actual".into()
            } else {
                "pending".into()
            },
            calculation_method: if is_priced {
                CalculationMethod::Native
            } else {
                CalculationMethod::Estimated
            },
            status: "indexed".into(),
        },
    };
    let usage_event = is_priced.then(|| {
        build_cursor_pricing_event(
            composer_id,
            updated_at,
            total_tokens,
            cost_usd,
            value
                .get("usageData")
                .and_then(Value::as_object)
                .and_then(|usage_data| {
                    (usage_data.len() == 1)
                        .then(|| usage_data.keys().next().map(|key| key.to_string()))
                        .flatten()
                }),
        )
    });

    Some((session, usage_event))
}

fn parse_usage_data_cost_usd(value: &Value) -> Option<f64> {
    let usage_data = value.as_object()?;
    if usage_data.is_empty() {
        return None;
    }

    let mut total_cost_in_cents = 0.0;
    for entry in usage_data.values() {
        let cost_in_cents = entry.get("costInCents")?.as_f64()?;
        if !cost_in_cents.is_finite() || cost_in_cents < 0.0 {
            return None;
        }
        total_cost_in_cents += cost_in_cents;
    }

    Some(total_cost_in_cents / 100.0)
}

fn parse_cursor_session_token_total(value: &Value) -> Option<u64> {
    parse_direct_token_total(value)
        .or_else(|| value.get("usage").and_then(parse_direct_token_total))
        .or_else(|| value.get("metrics").and_then(parse_direct_token_total))
        .or_else(|| {
            let usage_data = value.get("usageData")?.as_object()?;
            let mut total = 0u64;
            let mut found_any = false;
            for entry in usage_data.values() {
                let entry_total = parse_direct_token_total(entry)?;
                found_any = true;
                total = total.saturating_add(entry_total);
            }
            found_any.then_some(total)
        })
}

fn parse_direct_token_total(value: &Value) -> Option<u64> {
    ["tokenCount", "totalTokens", "total_tokens"]
        .iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
}

fn build_cursor_pricing_event(
    session_id: &str,
    occurred_at: DateTime<Utc>,
    total_tokens: u64,
    real_cost_usd: f64,
    model: Option<String>,
) -> UsageEvent {
    UsageEvent {
        source_id: SOURCE_ID,
        occurred_at,
        model: model.unwrap_or_else(|| "unknown".into()),
        token_breakdown: TokenBreakdown {
            input_tokens: 0,
            cache_creation_input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 0,
            other_tokens: total_tokens,
        },
        total_tokens,
        calculation_method: CalculationMethod::Native,
        session_id: session_id.to_string(),
        explicit_cost_usd: Some(real_cost_usd),
    }
}

fn composer_workspace_map(workspace_storage: &Path) -> std::collections::HashMap<String, String> {
    let mut mapping = std::collections::HashMap::new();
    let entries = match fs::read_dir(workspace_storage) {
        Ok(entries) => entries,
        Err(_) => return mapping,
    };

    for entry in entries.filter_map(std::result::Result::ok) {
        let dir = entry.path();
        let db_path = dir.join("state.vscdb");
        let workspace_json = dir.join("workspace.json");
        if !db_path.exists() || !workspace_json.exists() {
            continue;
        }

        let workspace_name = fs::read_to_string(&workspace_json)
            .ok()
            .and_then(|content| serde_json::from_str::<Value>(&content).ok())
            .and_then(|json| {
                json.get("folder")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .map(|folder| folder.trim_start_matches("file://").to_string())
            .map(|folder| workspace_label(&folder))
            .unwrap_or_else(|| "unknown".into());

        let connection =
            match Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
                Ok(connection) => connection,
                Err(_) => continue,
            };

        let value = connection
            .query_row(
                "select value from ItemTable where key='composer.composerData'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok();

        let Some(value) = value else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<Value>(&value) else {
            continue;
        };
        let Some(all_composers) = json.get("allComposers").and_then(Value::as_array) else {
            continue;
        };

        for composer in all_composers {
            if let Some(composer_id) = composer.get("composerId").and_then(Value::as_str) {
                mapping.insert(composer_id.to_string(), workspace_name.clone());
            }
        }
    }

    mapping
}

fn workspace_label(folder: &str) -> String {
    Path::new(folder)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| "unknown".into())
}

fn epoch_millis_to_utc(timestamp_ms: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_millis_opt(timestamp_ms).single()
}

fn normalize_text(text: &str) -> String {
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
    use serde_json::json;

    #[test]
    fn parses_cursor_usage_data_into_real_session_cost() {
        let value = json!({
            "createdAt": 1_710_000_000_000i64,
            "lastUpdatedAt": 1_710_000_123_000i64,
            "name": "Real cost session",
            "usageData": {
                "gpt-4.1": { "costInCents": 125 },
                "claude-3.7": { "costInCents": 75 }
            }
        });

        let (session, usage_event) =
            parse_cursor_session("composer-1", &value, None).expect("session should parse");

        assert_eq!(session.summary.cost_usd, 2.0);
        assert_eq!(session.summary.pricing_coverage, PricingCoverage::Actual);
        assert_eq!(session.summary.pricing_state, "actual");
        let usage_event = usage_event.expect("priced session should emit usage event");
        assert_eq!(usage_event.explicit_cost_usd, Some(2.0));
        assert_eq!(usage_event.session_id, "composer-1");
    }

    #[test]
    fn cursor_session_stays_pending_when_usage_data_costs_are_invalid() {
        let malformed = json!({
            "createdAt": 1_710_000_000_000i64,
            "lastUpdatedAt": 1_710_000_123_000i64,
            "usageData": {
                "gpt-4.1": { "costInCents": 125 },
                "claude-3.7": { "costInCents": "oops" }
            }
        });
        let missing = json!({
            "createdAt": 1_710_000_000_000i64,
            "lastUpdatedAt": 1_710_000_123_000i64,
            "usageData": {
                "gpt-4.1": {}
            }
        });

        let (malformed_session, malformed_event) =
            parse_cursor_session("composer-1", &malformed, None).expect("session should parse");
        let (missing_session, missing_event) =
            parse_cursor_session("composer-2", &missing, None).expect("session should parse");

        assert_eq!(malformed_session.summary.cost_usd, 0.0);
        assert_eq!(
            malformed_session.summary.pricing_coverage,
            PricingCoverage::Pending
        );
        assert_eq!(malformed_session.summary.pricing_state, "pending");
        assert!(malformed_event.is_none());

        assert_eq!(missing_session.summary.cost_usd, 0.0);
        assert_eq!(
            missing_session.summary.pricing_coverage,
            PricingCoverage::Pending
        );
        assert_eq!(missing_session.summary.pricing_state, "pending");
        assert!(missing_event.is_none());
    }

    #[test]
    fn cursor_session_preserves_token_totals_when_token_count_is_present() {
        let value = json!({
            "createdAt": 1_710_000_000_000i64,
            "lastUpdatedAt": 1_710_000_123_000i64,
            "usageData": {
                "gpt-4.1": { "costInCents": 240 }
            },
            "tokenCount": 4321
        });

        let (session, usage_event) =
            parse_cursor_session("composer-1", &value, None).expect("session should parse");

        assert_eq!(session.summary.total_tokens, 4321);
        let usage_event = usage_event.expect("priced session should emit usage event");
        assert_eq!(usage_event.total_tokens, 4321);
    }
}
