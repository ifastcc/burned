use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Local, Utc};

use crate::connectors::{SessionRecord, SourceConnector, SourceReport};
use crate::models::{CalculationMethod, SessionSummary, SourceState, SourceStatus};

const SOURCE_ID: &str = "antigravity";
const SOURCE_NAME: &str = "Antigravity";

pub struct AntigravityConnector;

impl SourceConnector for AntigravityConnector {
    fn collect(&self) -> SourceReport {
        collect_antigravity().unwrap_or_else(|error| SourceReport {
            status: SourceStatus {
                id: SOURCE_ID.into(),
                name: SOURCE_NAME.into(),
                state: SourceState::Partial,
                capabilities: vec![
                    "raw-artifacts".into(),
                    "workspace-storage".into(),
                    "logs".into(),
                ],
                note: format!("Antigravity detected, but discovery failed: {error}"),
                local_path: antigravity_primary_path().map(display_path),
                session_count: None,
                last_seen_at: None,
            },
            usage_events: Vec::new(),
            sessions: Vec::new(),
        })
    }
}

fn collect_antigravity() -> Result<SourceReport> {
    let primary = antigravity_primary_path();
    let secondary = antigravity_secondary_path();
    let root = primary
        .clone()
        .filter(|path| path.exists())
        .or_else(|| secondary.clone().filter(|path| path.exists()));

    let Some(root) = root else {
        return Ok(missing_report());
    };

    let logs_root = antigravity_logs_path();
    let conversation_dir = dirs::home_dir().map(|home| {
        home.join(".gemini")
            .join("antigravity")
            .join("conversations")
    });
    let session_count = conversation_dir
        .filter(|path| path.exists())
        .and_then(|path| fs::read_dir(path).ok())
        .map(|entries| entries.filter_map(std::result::Result::ok).count() as u32);
    let sessions = antigravity_sessions();

    Ok(SourceReport {
        status: SourceStatus {
            id: SOURCE_ID.into(),
            name: SOURCE_NAME.into(),
            state: SourceState::Partial,
            capabilities: vec![
                "raw-artifacts".into(),
                "workspace-storage".into(),
                "logs".into(),
            ],
            note: "Workspace storage, logs, and raw artifacts are present. Session schema and token fields are still unverified."
                .into(),
            local_path: Some(display_path(root)),
            session_count,
            last_seen_at: logs_root.as_ref().and_then(|path| format_mtime(path).ok()),
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
                "raw-artifacts".into(),
                "workspace-storage".into(),
                "logs".into(),
            ],
            note: "No Antigravity local storage was found on this machine.".into(),
            local_path: antigravity_primary_path()
                .or_else(antigravity_secondary_path)
                .map(display_path),
            session_count: None,
            last_seen_at: None,
        },
        usage_events: Vec::new(),
        sessions: Vec::new(),
    }
}

fn antigravity_primary_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".gemini").join("antigravity"))
}

fn antigravity_secondary_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| {
        home.join("Library")
            .join("Application Support")
            .join("Antigravity")
    })
}

fn antigravity_logs_path() -> Option<PathBuf> {
    antigravity_secondary_path().map(|path| path.join("logs"))
}

fn format_mtime(path: &Path) -> Result<String> {
    let modified = fs::metadata(path)?.modified()?;
    let modified: DateTime<Local> = modified.into();
    Ok(modified.format("%Y-%m-%d %H:%M").to_string())
}

fn display_path(path: PathBuf) -> String {
    path.display().to_string()
}

fn antigravity_sessions() -> Vec<SessionRecord> {
    let Some(brain_root) = antigravity_primary_path().map(|path| path.join("brain")) else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(&brain_root) else {
        return Vec::new();
    };

    let mut sessions = Vec::new();
    for entry in entries.filter_map(std::result::Result::ok) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let id = match path.file_name().and_then(|name| name.to_str()) {
            Some(id) => id.to_string(),
            None => continue,
        };
        let task_path = path.join("task.md");
        let metadata_path = path.join("task.md.metadata.json");
        if !task_path.exists() {
            continue;
        }

        let task_content = fs::read_to_string(&task_path).unwrap_or_default();
        let title = extract_task_title(&task_content);
        let preview = extract_task_preview(&task_content, &metadata_path);
        let updated_at = metadata_path
            .exists()
            .then(|| fs::read_to_string(&metadata_path).ok())
            .flatten()
            .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
            .and_then(|json| {
                json.get("updatedAt")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .and_then(|timestamp| DateTime::parse_from_rfc3339(&timestamp).ok())
            .map(|timestamp| timestamp.with_timezone(&Utc))
            .or_else(|| {
                fs::metadata(&task_path)
                    .ok()
                    .and_then(|meta| meta.modified().ok())
                    .map(Into::into)
            })
            .unwrap_or_else(Utc::now);

        sessions.push(SessionRecord {
            updated_at,
            summary: SessionSummary {
                id,
                source_id: SOURCE_ID.into(),
                title,
                preview,
                source: SOURCE_NAME.into(),
                workspace: "brain".into(),
                model: "unknown".into(),
                started_at: updated_at
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
    sessions
}

fn extract_task_title(task_content: &str) -> String {
    task_content
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with('#'))
        .map(|line| line.trim_start_matches('#').trim().to_string())
        .filter(|line| !line.is_empty())
        .map(|line| truncate(&line, 72))
        .unwrap_or_else(|| "Untitled Antigravity task".into())
}

fn extract_task_preview(task_content: &str, metadata_path: &Path) -> String {
    let metadata_summary = fs::read_to_string(metadata_path)
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .and_then(|json| {
            json.get("summary")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .map(|summary| normalize_text(&summary))
        .filter(|summary| !summary.is_empty());

    metadata_summary
        .map(|summary| truncate(&summary, 180))
        .or_else(|| {
            task_content
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty() && !line.starts_with('#'))
                .map(normalize_text)
                .filter(|line| !line.is_empty())
                .map(|line| truncate(&line, 180))
        })
        .unwrap_or_else(|| "No preview available.".into())
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
