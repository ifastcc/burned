use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Local, TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::connectors::{SessionRecord, SourceConnector, SourceReport};
use crate::models::{CalculationMethod, SessionSummary, SourceState, SourceStatus};

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
            note: "Local SQLite history is available. Session titles and previews are parsed from composer data; native token usage should come from the team Admin API."
                .into(),
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

fn display_path(path: PathBuf) -> String {
    path.display().to_string()
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
                model: "unknown".into(),
                started_at: created_at
                    .with_timezone(&Local)
                    .format("%b %-d %H:%M")
                    .to_string(),
                total_tokens: 0,
                cost_usd: 0.0,
                calculation_method: CalculationMethod::Estimated,
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
