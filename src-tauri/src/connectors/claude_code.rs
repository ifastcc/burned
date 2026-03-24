use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use chrono::{DateTime, Local, Utc};
use serde_json::Value;
use walkdir::WalkDir;

use crate::connectors::{
    report_scan_detail, SessionRecord, SourceConnector, SourceReport, UsageEvent,
};
use crate::models::{
    CalculationMethod, PricingCoverage, SessionSummary, SourceState, SourceStatus,
};
use crate::pricing::TokenBreakdown;

const SOURCE_ID: &str = "claude_code";
const SOURCE_NAME: &str = "Claude Code";

pub struct ClaudeCodeConnector;

#[derive(Default)]
struct SessionAccumulator {
    session_id: String,
    cwd: String,
    model: String,
    started_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
    total_tokens: u64,
    title: Option<String>,
    preview: Option<String>,
    has_non_meta_user_message: bool,
}

impl SourceConnector for ClaudeCodeConnector {
    fn collect(&self) -> SourceReport {
        collect_claude().unwrap_or_else(|error| SourceReport {
            status: SourceStatus {
                id: SOURCE_ID.into(),
                name: SOURCE_NAME.into(),
                state: SourceState::Partial,
                capabilities: vec![
                    "local-jsonl".into(),
                    "native-tokens".into(),
                    "cli-json".into(),
                    "analytics-api".into(),
                ],
                note: format!("Claude Code detected, but ingestion failed: {error}"),
                local_path: claude_root().map(display_path),
                session_count: None,
                last_seen_at: None,
            },
            usage_events: Vec::new(),
            sessions: Vec::new(),
        })
    }
}

fn collect_claude() -> Result<SourceReport> {
    let Some(root) = claude_root() else {
        return Ok(missing_report());
    };

    if !root.exists() {
        return Ok(missing_report());
    }

    let projects_root = root.join("projects");
    let all_files = session_files(&projects_root);
    let session_count = all_files.len() as u32;
    let parse_targets = recent_files(&all_files, 180, Duration::from_secs(180 * 24 * 60 * 60));

    let mut usage_events = Vec::new();
    let mut sessions = Vec::new();

    let parse_target_total = parse_targets.len();
    for (index, path) in parse_targets.into_iter().enumerate() {
        if parse_target_total > 0
            && (index == 0 || index + 1 == parse_target_total || (index + 1) % 25 == 0)
        {
            report_scan_detail(
                SOURCE_NAME,
                format!("Project logs {}/{}", index + 1, parse_target_total),
            );
        }
        let report = parse_session_file(&path)?;
        usage_events.extend(report.usage_events);
        if let Some(session) = report.session {
            sessions.push(session);
        }
    }

    sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    sessions.truncate(12);

    let latest_seen = all_files
        .iter()
        .filter_map(|path| format_mtime(path).ok())
        .max();

    let state = if usage_events.is_empty() {
        SourceState::Partial
    } else {
        SourceState::Ready
    };

    let note = if usage_events.is_empty() {
        "Local project session files were found, but no assistant usage events were parsed yet."
            .into()
    } else {
        "Local project session JSONL files expose assistant usage. CLI JSON and Anthropic analytics can be layered on later."
            .into()
    };

    Ok(SourceReport {
        status: SourceStatus {
            id: SOURCE_ID.into(),
            name: SOURCE_NAME.into(),
            state,
            capabilities: vec![
                "local-jsonl".into(),
                "native-tokens".into(),
                "cli-json".into(),
                "analytics-api".into(),
            ],
            note,
            local_path: Some(display_path(root)),
            session_count: Some(session_count),
            last_seen_at: latest_seen,
        },
        usage_events,
        sessions,
    })
}

struct ParsedSessionFile {
    usage_events: Vec<UsageEvent>,
    session: Option<SessionRecord>,
}

fn parse_session_file(path: &Path) -> Result<ParsedSessionFile> {
    let file = File::open(path).with_context(|| format!("open session file {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut usage_events = Vec::new();
    let mut sessions: HashMap<String, SessionAccumulator> = HashMap::new();

    for line in reader.lines() {
        let line = line?;
        let value: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => continue,
        };

        let session_id = value
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .map(str::to_owned)
            });
        let timestamp = value
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
            .map(|timestamp| timestamp.with_timezone(&Utc));
        let Some(session_id) = session_id else {
            continue;
        };
        let accumulator =
            sessions
                .entry(session_id.clone())
                .or_insert_with(|| SessionAccumulator {
                    session_id: session_id.clone(),
                    cwd: value
                        .get("cwd")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    model: value
                        .get("message")
                        .and_then(|message| message.get("model"))
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_string(),
                    ..SessionAccumulator::default()
                });

        if accumulator.cwd.is_empty() {
            accumulator.cwd = value
                .get("cwd")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
        }
        if accumulator.model == "unknown" {
            accumulator.model = value
                .get("message")
                .and_then(|message| message.get("model"))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
        }

        let content_text = extract_message_text(&value);
        let entry_type = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if entry_type == "user" && !content_text.is_empty() && !looks_meta_command(&content_text) {
            accumulator.has_non_meta_user_message = true;
            if accumulator.title.is_none() {
                accumulator.title = Some(truncate(&content_text, 72));
            }
            if accumulator.preview.is_none() {
                accumulator.preview = Some(truncate(&content_text, 180));
            }
        } else if entry_type == "assistant"
            && !content_text.is_empty()
            && accumulator.preview.is_none()
        {
            accumulator.preview = Some(truncate(&content_text, 180));
        }

        let Some(timestamp) = timestamp else {
            continue;
        };

        accumulator.cwd = if accumulator.cwd.is_empty() {
            value
                .get("cwd")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        } else {
            accumulator.cwd.clone()
        };
        accumulator.model = if accumulator.model == "unknown" {
            value
                .get("message")
                .and_then(|message| message.get("model"))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string()
        } else {
            accumulator.model.clone()
        };
        accumulator.started_at = Some(
            accumulator
                .started_at
                .map_or(timestamp, |current| current.min(timestamp)),
        );
        accumulator.updated_at = Some(
            accumulator
                .updated_at
                .map_or(timestamp, |current| current.max(timestamp)),
        );

        if entry_type == "assistant" {
            let usage = value
                .get("message")
                .and_then(|message| message.get("usage"));
            let input_tokens = usage
                .and_then(|usage| usage.get("input_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let cache_creation_tokens = usage
                .and_then(|usage| usage.get("cache_creation_input_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let cache_read_tokens = usage
                .and_then(|usage| usage.get("cache_read_input_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let output_tokens = usage
                .and_then(|usage| usage.get("output_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0);

            let token_breakdown = TokenBreakdown {
                input_tokens,
                cache_creation_input_tokens: cache_creation_tokens,
                cached_input_tokens: cache_read_tokens,
                output_tokens,
                other_tokens: 0,
            };
            let total_tokens = token_breakdown.total_tokens();
            if total_tokens > 0 && accumulator.has_non_meta_user_message {
                let event_model = value
                    .get("message")
                    .and_then(|message| message.get("model"))
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| accumulator.model.clone());
                usage_events.push(UsageEvent {
                    source_id: SOURCE_ID,
                    occurred_at: timestamp,
                    model: event_model,
                    token_breakdown,
                    total_tokens,
                    calculation_method: CalculationMethod::Native,
                    session_id: session_id.clone(),
                    explicit_cost_usd: None,
                });
                accumulator.total_tokens += total_tokens;
            }
        }
    }

    let session = sessions
        .into_values()
        .max_by_key(|session| session.updated_at)
        .and_then(to_session_record);

    Ok(ParsedSessionFile {
        usage_events,
        session,
    })
}

fn to_session_record(session: SessionAccumulator) -> Option<SessionRecord> {
    if !session.has_non_meta_user_message {
        return None;
    }

    let started_at = session.started_at?;
    let updated_at = session.updated_at.unwrap_or(started_at);

    Some(SessionRecord {
        updated_at,
        summary: SessionSummary {
            id: session.session_id,
            source_id: SOURCE_ID.into(),
            title: session
                .title
                .unwrap_or_else(|| "Untitled Claude session".into()),
            preview: session
                .preview
                .unwrap_or_else(|| "No preview available.".into()),
            source: SOURCE_NAME.into(),
            workspace: workspace_name(&session.cwd),
            model: session.model,
            started_at: started_at
                .with_timezone(&Local)
                .format("%b %-d %H:%M")
                .to_string(),
            total_tokens: session.total_tokens,
            cost_usd: 0.0,
            priced_sessions: 0,
            pending_pricing_sessions: 0,
            pricing_coverage: PricingCoverage::Pending,
            pricing_state: "pending".into(),
            calculation_method: CalculationMethod::Native,
            status: "indexed".into(),
        },
    })
}

fn session_files(projects_root: &Path) -> Vec<PathBuf> {
    if !projects_root.exists() {
        return Vec::new();
    }

    WalkDir::new(projects_root)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("jsonl"))
        .filter(|path| !path.to_string_lossy().contains("/subagents/"))
        .collect()
}

fn recent_files(files: &[PathBuf], top_n: usize, max_age: Duration) -> Vec<PathBuf> {
    let cutoff = SystemTime::now()
        .checked_sub(max_age)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut with_mtime = files
        .iter()
        .filter_map(|path| {
            fs::metadata(path)
                .and_then(|meta| meta.modified())
                .ok()
                .map(|mtime| (path.clone(), mtime))
        })
        .collect::<Vec<_>>();

    with_mtime.sort_by(|left, right| right.1.cmp(&left.1));

    let mut selected = Vec::new();
    for (index, (path, mtime)) in with_mtime.into_iter().enumerate() {
        if index < top_n || mtime >= cutoff {
            selected.push(path);
        }
    }

    selected
}

fn missing_report() -> SourceReport {
    SourceReport {
        status: SourceStatus {
            id: SOURCE_ID.into(),
            name: SOURCE_NAME.into(),
            state: SourceState::Missing,
            capabilities: vec![
                "local-jsonl".into(),
                "native-tokens".into(),
                "cli-json".into(),
                "analytics-api".into(),
            ],
            note: "No Claude Code home directory was found on this machine.".into(),
            local_path: claude_root().map(display_path),
            session_count: None,
            last_seen_at: None,
        },
        usage_events: Vec::new(),
        sessions: Vec::new(),
    }
}

fn claude_root() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".claude"))
}

fn workspace_name(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| "unknown".into())
}

fn format_mtime(path: &Path) -> Result<String> {
    let modified = fs::metadata(path)?.modified()?;
    let modified: DateTime<Local> = modified.into();
    Ok(modified.format("%Y-%m-%d %H:%M").to_string())
}

fn display_path(path: PathBuf) -> String {
    path.display().to_string()
}

fn extract_message_text(value: &Value) -> String {
    let Some(message) = value.get("message") else {
        return String::new();
    };

    if let Some(content) = message.get("content").and_then(Value::as_str) {
        return normalize_text(content);
    }

    message
        .get("content")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .map(normalize_text)
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

fn looks_meta_command(text: &str) -> bool {
    let trimmed = text.trim();
    let normalized = trimmed.to_ascii_lowercase();
    trimmed.contains("<local-command-caveat>")
        || trimmed.starts_with("<command-name>")
        || trimmed.starts_with("<command-message>")
        || trimmed.contains("<command-name>")
        || trimmed.starts_with("<local-command-stdout>")
        || normalized.starts_with("the user just ran /")
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
    use anyhow::Result;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn write_temp_session(contents: impl AsRef<[u8]>) -> Result<PathBuf> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("burned-claude-{unique}.jsonl"));
        fs::write(&path, contents)?;
        Ok(path)
    }

    #[test]
    fn parse_session_file_excludes_meta_command_only_sessions() -> Result<()> {
        let path = write_temp_session(
            r#"{"type":"user","timestamp":"2026-03-23T14:30:22.695Z","sessionId":"meta-session","cwd":"/Users/kbaicai","message":{"role":"user","content":"<command-message>insights</command-message>\n<command-name>/insights</command-name>"}}"#
                .to_owned()
                + "\n"
                + r#"{"type":"user","timestamp":"2026-03-23T14:30:22.695Z","sessionId":"meta-session","cwd":"/Users/kbaicai","message":{"role":"user","content":"The user just ran /insights to generate a usage report."}}"#
                + "\n"
                + r#"{"type":"assistant","timestamp":"2026-03-23T14:30:27.587Z","sessionId":"meta-session","cwd":"/Users/kbaicai","message":{"role":"assistant","model":"claude-opus-4-6","content":[{"type":"text","text":"report ready"}],"usage":{"input_tokens":3,"cache_creation_input_tokens":23579,"cache_read_input_tokens":10252,"output_tokens":2}}}"#,
        )?;
        let parsed = parse_session_file(&path)?;
        let _ = fs::remove_file(&path);

        assert!(parsed.usage_events.is_empty());
        assert!(parsed.session.is_none());
        Ok(())
    }

    #[test]
    fn parse_session_file_keeps_regular_user_sessions() -> Result<()> {
        let path = write_temp_session(
            r#"{"type":"user","timestamp":"2026-03-23T14:30:22.695Z","sessionId":"real-session","cwd":"/Users/kbaicai/project","message":{"role":"user","content":"帮我修一个 bug"}}"#
                .to_owned()
                + "\n"
                + r#"{"type":"assistant","timestamp":"2026-03-23T14:30:27.587Z","sessionId":"real-session","cwd":"/Users/kbaicai/project","message":{"role":"assistant","model":"claude-opus-4-6","content":[{"type":"text","text":"好的"}],"usage":{"input_tokens":10,"cache_creation_input_tokens":0,"cache_read_input_tokens":20,"output_tokens":5}}}"#,
        )?;
        let parsed = parse_session_file(&path)?;
        let _ = fs::remove_file(&path);

        assert_eq!(parsed.usage_events.len(), 1);
        assert_eq!(parsed.usage_events[0].total_tokens, 35);
        assert_eq!(
            parsed.session.expect("session").summary.title,
            "帮我修一个 bug"
        );
        Ok(())
    }
}
