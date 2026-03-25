use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Local, TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::connectors::{SessionRecord, SourceConnector, SourceReport};
use crate::models::{
    CalculationMethod, PricingCoverage, SessionSummary, SourceState, SourceStatus,
};

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
    collect_cursor_at(&root)
}

fn collect_cursor_at(root: &Path) -> Result<SourceReport> {
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
    let sessions = query_sessions(&connection, &workspace_map)?;

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
            note: cursor_status_note().into(),
            local_path: Some(display_path(root)),
            session_count: session_count.or(workspace_count),
            last_seen_at: format_mtime(&global_store).ok(),
        },
        usage_events: Vec::new(),
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

fn display_path(path: impl AsRef<Path>) -> String {
    path.as_ref().display().to_string()
}

fn query_sessions(
    connection: &Connection,
    workspace_map: &std::collections::HashMap<String, String>,
) -> Result<Vec<SessionRecord>> {
    let mut statement = connection.prepare(
        "select key, value from cursorDiskKV where key like 'composerData:%' and value <> ''",
    )?;
    let rows = statement.query_map([], |row| {
        let key: String = row.get(0)?;
        let value: String = row.get(1)?;
        Ok((key, value))
    })?;

    let mut sessions = Vec::new();
    for row in rows {
        let (key, raw_json) = row?;
        let composer_id = key.trim_start_matches("composerData:").to_string();
        let value: Value = match serde_json::from_str(&raw_json) {
            Ok(value) => value,
            Err(_) => continue,
        };

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

        sessions.push(SessionRecord {
            updated_at,
            summary: SessionSummary {
                id: composer_id.clone(),
                source_id: SOURCE_ID.into(),
                title: truncate(&title, 72),
                preview: truncate(&preview, 180),
                source: SOURCE_NAME.into(),
                workspace: workspace_map
                    .get(&composer_id)
                    .cloned()
                    .unwrap_or_else(|| "unknown".into()),
                model: cursor_model_label(&value),
                started_at: created_at
                    .with_timezone(&Local)
                    .format("%b %-d %H:%M")
                    .to_string(),
                total_tokens: cursor_total_tokens(&value),
                cost_usd: cursor_cost_usd(&value),
                pricing_coverage: cursor_pricing_coverage(&value),
                calculation_method: CalculationMethod::Derived,
                status: "indexed".into(),
            },
        });
    }

    sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    sessions.truncate(12);
    Ok(sessions)
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

fn cursor_status_note() -> &'static str {
    "Cursor session metadata is indexed locally. Day-level analytics still require native/admin usage ingestion."
}

fn cursor_total_tokens(value: &Value) -> u64 {
    value
        .get("tokenCount")
        .and_then(|token_count| match token_count {
            Value::Number(number) => number
                .as_u64()
                .or_else(|| number.as_i64().map(|value| value.max(0) as u64)),
            Value::String(raw) => raw.trim().parse::<u64>().ok(),
            _ => None,
        })
        .filter(|token_count| *token_count > 0)
        .unwrap_or(0)
}

fn cursor_cost_usd(value: &Value) -> f64 {
    let Some(usage_data) = value.get("usageData").and_then(Value::as_object) else {
        return 0.0;
    };

    let total_cents: i64 = usage_data
        .values()
        .filter_map(|entry| entry.get("costInCents"))
        .filter_map(|value| match value {
            Value::Number(number) => number.as_i64(),
            Value::String(raw) => raw.trim().parse::<i64>().ok(),
            _ => None,
        })
        .filter(|cost_in_cents| *cost_in_cents > 0)
        .sum();

    (total_cents as f64) / 100.0
}

fn cursor_pricing_coverage(value: &Value) -> Option<PricingCoverage> {
    let usage_data = value.get("usageData").and_then(Value::as_object)?;
    if usage_data.is_empty() {
        return Some(PricingCoverage::Pending);
    }

    let priced_entries = usage_data
        .values()
        .filter_map(|entry| entry.get("costInCents"))
        .filter_map(|value| match value {
            Value::Number(number) => number.as_i64(),
            Value::String(raw) => raw.trim().parse::<i64>().ok(),
            _ => None,
        })
        .filter(|cost_in_cents| *cost_in_cents > 0)
        .count();

    Some(match priced_entries {
        0 => PricingCoverage::Pending,
        count if count == usage_data.len() => PricingCoverage::Actual,
        _ => PricingCoverage::Partial,
    })
}

fn cursor_model_label(value: &Value) -> String {
    let Some(usage_data) = value.get("usageData").and_then(Value::as_object) else {
        return "unknown".into();
    };
    if usage_data.is_empty() {
        return "unknown".into();
    }
    if usage_data.len() > 1 {
        return "mixed".into();
    }

    let labels = usage_data
        .keys()
        .filter_map(|key| normalize_model_label(key))
        .collect::<Vec<_>>();

    match labels.as_slice() {
        [] => "unknown".into(),
        [single] => single.clone(),
        _ => "mixed".into(),
    }
}

fn normalize_model_label(label: &str) -> Option<String> {
    let normalized = normalize_text(label);
    (!normalized.is_empty()).then_some(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct HomeGuard {
        previous_home: Option<String>,
    }

    impl HomeGuard {
        fn set(temp_home: &Path) -> Self {
            let previous_home = env::var("HOME").ok();
            env::set_var("HOME", temp_home);
            Self { previous_home }
        }
    }

    impl Drop for HomeGuard {
        fn drop(&mut self) {
            if let Some(previous_home) = self.previous_home.take() {
                env::set_var("HOME", previous_home);
            } else {
                env::remove_var("HOME");
            }
        }
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        env::temp_dir().join(format!("{prefix}-{stamp}-{}", std::process::id()))
    }

    fn create_cursor_db(path: &Path, records: &[(&str, &str)]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create sqlite parent");
        }
        let connection = Connection::open(path).expect("open sqlite");
        connection
            .execute(
                "create table if not exists cursorDiskKV (key text primary key, value text)",
                [],
            )
            .expect("create cursorDiskKV");
        for (key, value) in records {
            connection
                .execute(
                    "insert or replace into cursorDiskKV (key, value) values (?1, ?2)",
                    [key, value],
                )
                .expect("insert record");
        }
    }

    fn create_workspace_mapping(root: &Path, composer_ids: &[&str], workspace_name: &str) {
        let workspace_dir = root
            .join("User")
            .join("workspaceStorage")
            .join("workspace-1");
        fs::create_dir_all(&workspace_dir).expect("create workspace dir");

        let workspace_json = serde_json::json!({
            "folder": format!("file:///Users/test/{workspace_name}"),
        });
        fs::write(
            workspace_dir.join("workspace.json"),
            serde_json::to_string(&workspace_json).expect("serialize workspace json"),
        )
        .expect("write workspace json");

        let connection =
            Connection::open(workspace_dir.join("state.vscdb")).expect("open workspace sqlite");
        connection
            .execute(
                "create table if not exists ItemTable (key text primary key, value text)",
                [],
            )
            .expect("create ItemTable");

        let all_composers = composer_ids
            .iter()
            .map(|composer_id| serde_json::json!({ "composerId": composer_id }))
            .collect::<Vec<_>>();
        let composer_value = serde_json::json!({ "allComposers": all_composers });
        connection
            .execute(
                "insert or replace into ItemTable (key, value) values ('composer.composerData', ?1)",
                [serde_json::to_string(&composer_value).expect("serialize composer value")],
            )
            .expect("insert composer data");
    }

    #[test]
    fn collect_cursor_reports_parsed_session_metadata_and_keeps_ordering() {
        let temp_home = unique_temp_dir("cursor-home");
        let _guard = HomeGuard::set(&temp_home);

        let cursor_root = temp_home
            .join("Library")
            .join("Application Support")
            .join("Cursor");
        let global_storage = cursor_root.join("User").join("globalStorage");
        let state_db = global_storage.join("state.vscdb");
        let backup_db = global_storage.join("state.vscdb.backup");

        create_cursor_db(
            &state_db,
            &[(
                "composerData:ignored",
                "{\"tokenCount\":1,\"usageData\":{\"unused\":{\"costInCents\":1}}}",
            )],
        );

        let newer = serde_json::json!({
            "createdAt": 1700000000000i64,
            "lastUpdatedAt": 1700003600000i64,
            "name": "  New\nSession  ",
            "latestConversationSummary": {
                "summary": {
                    "summary": "First line\nSecond line"
                }
            },
            "tokenCount": 42,
            "usageData": {
                "claude-3.7-sonnet-thinking": {
                    "costInCents": 8
                }
            }
        });
        let mixed = serde_json::json!({
            "createdAt": 1699990000000i64,
            "lastUpdatedAt": 1699993600000i64,
            "text": "  fallback text  ",
            "tokenCount": 0,
            "usageData": {
                "gpt-4o": {
                    "costInCents": 11
                },
                "claude-3.5-sonnet": {
                    "costInCents": 9
                }
            }
        });
        let unknown = serde_json::json!({
            "createdAt": 1699980000000i64,
            "lastUpdatedAt": 1699983600000i64,
            "usageData": {}
        });
        create_cursor_db(
            &backup_db,
            &[
                ("composerData:new", &serde_json::to_string(&newer).unwrap()),
                ("composerData:mixed", &serde_json::to_string(&mixed).unwrap()),
                ("composerData:unknown", &serde_json::to_string(&unknown).unwrap()),
            ],
        );
        create_workspace_mapping(&cursor_root, &["new", "mixed"], "workspace-one");

        let report = collect_cursor().expect("collect cursor");

        assert!(report.usage_events.is_empty());
        assert_eq!(
            report.status.note,
            "Cursor session metadata is indexed locally. Day-level analytics still require native/admin usage ingestion."
        );
        assert_eq!(report.sessions.len(), 3);

        let first = &report.sessions[0].summary;
        assert_eq!(first.id, "new");
        assert_eq!(first.title, "New Session");
        assert_eq!(first.preview, "First line Second line");
        assert_eq!(first.workspace, "workspace-one");
        assert_eq!(first.model, "claude-3.7-sonnet-thinking");
        assert_eq!(first.total_tokens, 42);
        assert!((first.cost_usd - 0.08).abs() < f64::EPSILON);
        assert_eq!(first.calculation_method, CalculationMethod::Derived);

        let second = &report.sessions[1].summary;
        assert_eq!(second.id, "mixed");
        assert_eq!(second.model, "mixed");
        assert_eq!(second.total_tokens, 0);
        assert!((second.cost_usd - 0.20).abs() < f64::EPSILON);
        assert_eq!(second.calculation_method, CalculationMethod::Derived);

        let third = &report.sessions[2].summary;
        assert_eq!(third.id, "unknown");
        assert_eq!(third.model, "unknown");
        assert_eq!(third.total_tokens, 0);
        assert_eq!(third.cost_usd, 0.0);
        assert_eq!(third.calculation_method, CalculationMethod::Derived);
    }

    #[test]
    fn cursor_model_label_treats_multiple_usage_keys_as_mixed() {
        let value = serde_json::json!({
            "usageData": {
                "   ": { "costInCents": 3 },
                "gpt-4o": { "costInCents": 7 }
            }
        });

        assert_eq!(cursor_model_label(&value), "mixed");
    }
}
