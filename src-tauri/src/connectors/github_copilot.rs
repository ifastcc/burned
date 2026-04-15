use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use chrono::{DateTime, Local, TimeZone, Utc};
use serde::Deserialize;
use serde_json::{Map, Value};
use walkdir::WalkDir;

use crate::connectors::{
    report_scan_detail, SessionRecord, SourceConnector, SourceReport, UsageEvent,
};
use crate::models::{
    CalculationMethod, PricingCoverage, SessionSummary, SourceState, SourceStatus,
};
use crate::pricing::TokenBreakdown;

const SOURCE_ID: &str = "github_copilot";
const SOURCE_NAME: &str = "GitHub Copilot";

pub struct GitHubCopilotConnector;

#[derive(Clone)]
struct CopilotInstallation {
    workspace_storage_root: PathBuf,
    global_storage_root: PathBuf,
}

#[derive(Default, Deserialize)]
struct WorkspaceDescriptor {
    folder: Option<String>,
    configuration: Option<String>,
}

#[derive(Deserialize)]
struct JsonlRecord {
    kind: u8,
    #[serde(default)]
    k: Vec<Value>,
    v: Value,
}

struct ParsedAgentSession {
    usage_events: Vec<UsageEvent>,
    session: Option<SessionRecord>,
}

impl SourceConnector for GitHubCopilotConnector {
    fn collect(&self) -> SourceReport {
        collect_github_copilot().unwrap_or_else(|error| SourceReport {
            status: SourceStatus {
                id: SOURCE_ID.into(),
                name: SOURCE_NAME.into(),
                state: SourceState::Partial,
                capabilities: vec![
                    "local-json".into(),
                    "local-jsonl".into(),
                    "workspace-storage".into(),
                    "workspace-json".into(),
                    "agent-session-state".into(),
                    "session-content".into(),
                    "request-models".into(),
                    "native-usage".into(),
                    "premium-requests".into(),
                ],
                note: format!("GitHub Copilot detected, but ingestion failed: {error}"),
                local_path: discovered_installations()
                    .into_iter()
                    .map(|installation| display_path(installation.workspace_storage_root))
                    .next(),
                session_count: None,
                last_seen_at: None,
            },
            usage_events: Vec::new(),
            sessions: Vec::new(),
        })
    }
}

fn collect_github_copilot() -> Result<SourceReport> {
    let installations = discovered_installations();
    let install_detected = installations
        .iter()
        .any(|installation| installation.global_storage_root.exists());
    let chat_session_files = installations
        .iter()
        .flat_map(|installation| session_files(&installation.workspace_storage_root))
        .collect::<Vec<_>>();
    let agent_session_root = copilot_session_state_root();
    let agent_state_files = agent_session_files(&agent_session_root);

    if !install_detected && chat_session_files.is_empty() && agent_state_files.is_empty() {
        return Ok(missing_report());
    }

    let parse_targets = recent_files(
        &chat_session_files,
        180,
        Duration::from_secs(180 * 24 * 60 * 60),
    );
    let parse_target_total = parse_targets.len();
    let mut sessions = Vec::new();
    let mut usage_events = Vec::new();

    for (index, path) in parse_targets.into_iter().enumerate() {
        if parse_target_total > 0
            && (index == 0 || index + 1 == parse_target_total || (index + 1) % 25 == 0)
        {
            report_scan_detail(
                SOURCE_NAME,
                format!("Chat sessions {}/{}", index + 1, parse_target_total),
            );
        }

        if let Some(session) = parse_session_file(&path)? {
            sessions.push(session);
        }
    }

    let agent_parse_targets = recent_files(
        &agent_state_files,
        180,
        Duration::from_secs(180 * 24 * 60 * 60),
    );
    let agent_parse_target_total = agent_parse_targets.len();

    for (index, path) in agent_parse_targets.into_iter().enumerate() {
        if agent_parse_target_total > 0
            && (index == 0
                || index + 1 == agent_parse_target_total
                || (index + 1) % 25 == 0)
        {
            report_scan_detail(
                SOURCE_NAME,
                format!("Agent sessions {}/{}", index + 1, agent_parse_target_total),
            );
        }

        let parsed = parse_agent_session_file(&path)?;
        usage_events.extend(parsed.usage_events);
        if let Some(session) = parsed.session {
            sessions.push(session);
        }
    }

    sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    sessions = dedupe_sessions(sessions);
    sessions.truncate(12);

    let mut all_activity_files = chat_session_files.clone();
    all_activity_files.extend(agent_state_files.clone());
    let last_seen_at = all_activity_files
        .iter()
        .filter_map(|path| format_mtime(path).ok())
        .max();
    let local_path = installations
        .iter()
        .find(|installation| installation.workspace_storage_root.exists())
        .map(|installation| display_path(installation.workspace_storage_root.clone()))
        .or_else(|| {
            if agent_session_root.exists() {
                Some(display_path(agent_session_root.clone()))
            } else {
                None
            }
        });

    let note = if chat_session_files.is_empty() && agent_state_files.is_empty() {
        "GitHub Copilot is installed, but no local VS Code or agent session files were found yet."
            .into()
    } else if !usage_events.is_empty() {
        "VS Code chatSessions expose session metadata, and Copilot agent session-state adds native session totals, model usage, and premium request counts."
            .into()
    } else if chat_session_files.is_empty() {
        "Copilot agent session-state was found, but no completed sessions with native usage totals were parsed yet."
            .into()
    } else if sessions.is_empty() {
        "GitHub Copilot chatSessions were found, but no usable user prompts were parsed yet."
            .into()
    } else {
        "VS Code chatSessions expose session titles, workspace context, and model IDs. Native token totals are only available for Copilot agent sessions under ~/.copilot/session-state."
            .into()
    };

    let state = if usage_events.is_empty() {
        SourceState::Partial
    } else {
        SourceState::Ready
    };

    Ok(SourceReport {
        status: SourceStatus {
            id: SOURCE_ID.into(),
            name: SOURCE_NAME.into(),
            state,
            capabilities: vec![
                "local-json".into(),
                "local-jsonl".into(),
                "workspace-storage".into(),
                "workspace-json".into(),
                "agent-session-state".into(),
                "session-content".into(),
                "request-models".into(),
                "native-usage".into(),
                "premium-requests".into(),
            ],
            note,
            local_path,
            session_count: Some((chat_session_files.len() + agent_state_files.len()) as u32),
            last_seen_at,
        },
        usage_events,
        sessions,
    })
}

fn discovered_installations() -> Vec<CopilotInstallation> {
    let Some(config_dir) = dirs::config_dir() else {
        return Vec::new();
    };

    [
        config_dir.join("Code").join("User"),
        config_dir.join("Code - Insiders").join("User"),
    ]
    .into_iter()
    .map(|user_root| CopilotInstallation {
        workspace_storage_root: user_root.join("workspaceStorage"),
        global_storage_root: user_root.join("globalStorage").join("github.copilot-chat"),
    })
    .collect()
}

fn missing_report() -> SourceReport {
    SourceReport {
        status: SourceStatus {
            id: SOURCE_ID.into(),
            name: SOURCE_NAME.into(),
            state: SourceState::Missing,
            capabilities: vec![
                "local-json".into(),
                "local-jsonl".into(),
                "workspace-storage".into(),
                "workspace-json".into(),
                "agent-session-state".into(),
                "session-content".into(),
                "request-models".into(),
                "native-usage".into(),
                "premium-requests".into(),
            ],
            note: "No GitHub Copilot VS Code storage or Copilot agent session-state was found on this machine.".into(),
            local_path: discovered_installations()
                .into_iter()
                .map(|installation| display_path(installation.workspace_storage_root))
                .next(),
            session_count: None,
            last_seen_at: None,
        },
        usage_events: Vec::new(),
        sessions: Vec::new(),
    }
}

fn session_files(workspace_storage_root: &Path) -> Vec<PathBuf> {
    if !workspace_storage_root.exists() {
        return Vec::new();
    }

    WalkDir::new(workspace_storage_root)
        .min_depth(3)
        .max_depth(3)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| entry.path().parent().and_then(Path::file_name).and_then(|name| name.to_str()) == Some("chatSessions"))
        .map(|entry| entry.into_path())
        .filter(|path| {
            matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("json") | Some("jsonl")
            )
        })
        .collect()
}

fn copilot_session_state_root() -> PathBuf {
    dirs::home_dir()
        .map(|home| home.join(".copilot").join("session-state"))
        .unwrap_or_else(|| PathBuf::from(".copilot/session-state"))
}

fn agent_session_files(session_state_root: &Path) -> Vec<PathBuf> {
    if !session_state_root.exists() {
        return Vec::new();
    }

    WalkDir::new(session_state_root)
        .min_depth(2)
        .max_depth(2)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| path.file_name().and_then(|name| name.to_str()) == Some("events.jsonl"))
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

fn parse_session_file(path: &Path) -> Result<Option<SessionRecord>> {
    let Some(snapshot) = read_session_snapshot(path)? else {
        return Ok(None);
    };

    let requests = snapshot
        .get("requests")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let first_user_message = requests
        .iter()
        .find_map(extract_request_text)
        .unwrap_or_default();

    if requests.is_empty() || first_user_message.is_empty() {
        return Ok(None);
    }

    let session_id = snapshot
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "unknown".into());

    let created_at = snapshot
        .get("creationDate")
        .and_then(Value::as_i64)
        .and_then(epoch_millis_to_utc)
        .unwrap_or_else(Utc::now);
    let updated_at = snapshot
        .get("lastMessageDate")
        .and_then(Value::as_i64)
        .and_then(epoch_millis_to_utc)
        .or_else(|| {
            requests
                .iter()
                .rev()
                .filter_map(|request| request.get("timestamp").and_then(Value::as_i64))
                .find_map(epoch_millis_to_utc)
        })
        .unwrap_or(created_at);
    let custom_title = snapshot
        .get("customTitle")
        .and_then(Value::as_str)
        .unwrap_or_default();

    Ok(Some(SessionRecord {
        updated_at,
        summary: SessionSummary {
            id: session_id,
            source_id: SOURCE_ID.into(),
            title: choose_title(custom_title, &first_user_message),
            preview: make_preview(&first_user_message),
            source: SOURCE_NAME.into(),
            workspace: workspace_name_for_session_file(path),
            model: choose_model(&snapshot, &requests),
            started_at: created_at
                .with_timezone(&Local)
                .format("%b %-d %H:%M")
                .to_string(),
            total_tokens: 0,
            cost_usd: 0.0,
            pricing_coverage: PricingCoverage::Pending,
            long_context: None,
            calculation_method: CalculationMethod::Derived,
            status: "indexed".into(),
        },
    }))
}

fn parse_agent_session_file(path: &Path) -> Result<ParsedAgentSession> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("read session file {}", path.display()))?;
    let mut session_id = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| "unknown".into());
    let mut started_at = None;
    let mut updated_at = None;
    let mut workspace_cwd = String::new();
    let mut selected_model = String::new();
    let mut current_model = String::new();
    let mut first_user_message = String::new();
    let mut usage_events = Vec::new();
    let mut session_status = "active".to_string();

    for line in contents.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(event_type) = value.get("type").and_then(Value::as_str) else {
            continue;
        };
        let data = value.get("data").unwrap_or(&Value::Null);

        match event_type {
            "session.start" => {
                if let Some(id) = data.get("sessionId").and_then(Value::as_str) {
                    session_id = id.to_string();
                }
                started_at = data
                    .get("startTime")
                    .and_then(Value::as_str)
                    .and_then(parse_rfc3339_utc)
                    .or(started_at);
                selected_model = data
                    .get("selectedModel")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or(selected_model);
                workspace_cwd = data
                    .get("context")
                    .and_then(|context| context.get("cwd"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or(workspace_cwd);
            }
            "user.message" => {
                if first_user_message.is_empty() {
                    first_user_message = data
                        .get("content")
                        .and_then(Value::as_str)
                        .map(normalize_text)
                        .unwrap_or_default();
                }
            }
            "session.shutdown" => {
                session_status = "completed".into();
                updated_at = value
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .and_then(parse_rfc3339_utc)
                    .or(updated_at);
                current_model = data
                    .get("currentModel")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or(current_model);

                if let Some(model_metrics) = data.get("modelMetrics").and_then(Value::as_object) {
                    let occurred_at = updated_at.unwrap_or_else(Utc::now);
                    for (model, metrics) in model_metrics {
                        if let Some(event) =
                            usage_event_from_model_metrics(&session_id, model, occurred_at, metrics)
                        {
                            usage_events.push(event);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let started_at = started_at.unwrap_or_else(Utc::now);
    let updated_at = updated_at.unwrap_or(started_at);
    let total_tokens = usage_events.iter().map(|event| event.total_tokens).sum::<u64>();
    let summary_model = if !current_model.is_empty() {
        current_model
    } else if !selected_model.is_empty() {
        selected_model
    } else if let Some(model) = usage_events.first().map(|event| event.model.clone()) {
        model
    } else {
        "unknown".into()
    };

    let session = if first_user_message.is_empty() && usage_events.is_empty() {
        None
    } else {
        Some(SessionRecord {
            updated_at,
            summary: SessionSummary {
                id: session_id.clone(),
                source_id: SOURCE_ID.into(),
                title: choose_title("", &first_user_message),
                preview: make_preview(&first_user_message),
                source: SOURCE_NAME.into(),
                workspace: workspace_name_from_cwd(&workspace_cwd),
                model: sanitize_model(&summary_model),
                started_at: started_at
                    .with_timezone(&Local)
                    .format("%b %-d %H:%M")
                    .to_string(),
                total_tokens,
                cost_usd: 0.0,
                pricing_coverage: PricingCoverage::Pending,
                long_context: None,
                calculation_method: if total_tokens > 0 {
                    CalculationMethod::Native
                } else {
                    CalculationMethod::Derived
                },
                status: session_status,
            },
        })
    };

    Ok(ParsedAgentSession {
        usage_events,
        session,
    })
}

fn read_session_snapshot(path: &Path) -> Result<Option<Value>> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("read session file {}", path.display()))?;

    match path.extension().and_then(|ext| ext.to_str()) {
        Some("json") => Ok(serde_json::from_str(&contents).ok()),
        Some("jsonl") => Ok(parse_jsonl_snapshot(&contents)),
        _ => Ok(None),
    }
}

fn parse_jsonl_snapshot(contents: &str) -> Option<Value> {
    let mut snapshot = None;

    for line in contents.lines() {
        let Ok(record) = serde_json::from_str::<JsonlRecord>(line) else {
            continue;
        };

        match record.kind {
            0 => snapshot = Some(record.v),
            1 | 2 => {
                if let Some(current) = snapshot.as_mut() {
                    apply_patch(current, &record.k, record.v);
                }
            }
            _ => {}
        }
    }

    snapshot
}

fn apply_patch(target: &mut Value, path: &[Value], value: Value) {
    if path.is_empty() {
        *target = value;
        return;
    }

    let mut current = target;

    for segment in &path[..path.len().saturating_sub(1)] {
        match segment {
            Value::String(key) => {
                if !current.is_object() {
                    *current = Value::Object(Map::new());
                }
                let object = current.as_object_mut().expect("object after initialization");
                current = object.entry(key.clone()).or_insert(Value::Null);
            }
            Value::Number(index) => {
                let Some(index) = index.as_u64().map(|index| index as usize) else {
                    return;
                };
                if !current.is_array() {
                    *current = Value::Array(Vec::new());
                }
                let array = current.as_array_mut().expect("array after initialization");
                while array.len() <= index {
                    array.push(Value::Null);
                }
                current = &mut array[index];
            }
            _ => return,
        }
    }

    match path.last() {
        Some(Value::String(key)) => {
            if !current.is_object() {
                *current = Value::Object(Map::new());
            }
            current
                .as_object_mut()
                .expect("object after initialization")
                .insert(key.clone(), value);
        }
        Some(Value::Number(index)) => {
            let Some(index) = index.as_u64().map(|index| index as usize) else {
                return;
            };
            if !current.is_array() {
                *current = Value::Array(Vec::new());
            }
            let array = current.as_array_mut().expect("array after initialization");
            while array.len() <= index {
                array.push(Value::Null);
            }
            array[index] = value;
        }
        _ => {}
    }
}

fn extract_request_text(request: &Value) -> Option<String> {
    let message = request.get("message")?;

    if let Some(text) = message.get("text").and_then(Value::as_str) {
        let normalized = normalize_text(text);
        if !normalized.is_empty() {
            return Some(normalized);
        }
    }

    let parts = message.get("parts").and_then(Value::as_array)?;
    let normalized = parts
        .iter()
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .map(normalize_text)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn choose_model(snapshot: &Value, requests: &[Value]) -> String {
    requests
        .iter()
        .rev()
        .find_map(|request| {
            request
                .get("resolvedModel")
                .and_then(Value::as_str)
                .or_else(|| request.get("modelId").and_then(Value::as_str))
        })
        .or_else(|| {
            snapshot
                .get("selectedModel")
                .and_then(|selected_model| selected_model.get("identifier"))
                .and_then(Value::as_str)
        })
        .map(sanitize_model)
        .filter(|model| !model.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

fn usage_event_from_model_metrics(
    session_id: &str,
    model: &str,
    occurred_at: DateTime<Utc>,
    metrics: &Value,
) -> Option<UsageEvent> {
    let usage = metrics.get("usage")?;
    let input_tokens = usage.get("inputTokens").and_then(Value::as_u64).unwrap_or(0);
    let output_tokens = usage.get("outputTokens").and_then(Value::as_u64).unwrap_or(0);
    let cached_input_tokens = usage
        .get("cacheReadTokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cache_creation_input_tokens = usage
        .get("cacheWriteTokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total_tokens = input_tokens
        .saturating_add(cached_input_tokens)
        .saturating_add(cache_creation_input_tokens)
        .saturating_add(output_tokens);

    if total_tokens == 0 {
        return None;
    }

    Some(UsageEvent {
        source_id: SOURCE_ID,
        occurred_at,
        model: sanitize_model(model),
        token_breakdown: TokenBreakdown {
            input_tokens,
            cache_creation_input_tokens,
            cached_input_tokens,
            output_tokens,
            other_tokens: 0,
        },
        total_tokens,
        calculation_method: CalculationMethod::Native,
        session_id: session_id.to_string(),
    })
}

fn sanitize_model(model: &str) -> String {
    model
        .strip_prefix("copilot/")
        .unwrap_or(model)
        .trim()
        .to_string()
}

fn parse_rfc3339_utc(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|timestamp| timestamp.with_timezone(&Utc))
}

fn workspace_name_for_session_file(path: &Path) -> String {
    let Some(storage_root) = path.parent().and_then(Path::parent) else {
        return "VS Code".into();
    };
    let workspace_json = storage_root.join("workspace.json");
    let contents = fs::read_to_string(workspace_json).ok();
    let descriptor = contents
        .as_deref()
        .and_then(|contents| serde_json::from_str::<WorkspaceDescriptor>(contents).ok());

    descriptor
        .and_then(|descriptor| descriptor.folder.or(descriptor.configuration))
        .and_then(|raw| workspace_name_from_location(&raw))
        .unwrap_or_else(|| "VS Code".into())
}

fn workspace_name_from_cwd(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| "VS Code".into())
}

fn workspace_name_from_location(raw: &str) -> Option<String> {
    let path = if let Some(path) = raw.strip_prefix("file://") {
        decode_percent_escapes(path)
    } else {
        raw.to_string()
    };

    Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

fn decode_percent_escapes(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(left), Some(right)) = (hex_value(bytes[index + 1]), hex_value(bytes[index + 2])) {
                decoded.push(left * 16 + right);
                index += 3;
                continue;
            }
        }

        decoded.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn epoch_millis_to_utc(value: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_millis_opt(value).single()
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
    let cleaned_title = normalize_text(title);
    if !cleaned_title.is_empty() {
        return truncate(&cleaned_title, 72);
    }

    let cleaned_fallback = normalize_text(fallback);
    if cleaned_fallback.is_empty() {
        "Untitled GitHub Copilot session".into()
    } else {
        truncate(&cleaned_fallback, 72)
    }
}

fn make_preview(text: &str) -> String {
    let cleaned = normalize_text(text);
    if cleaned.is_empty() {
        "No preview available.".into()
    } else {
        truncate(&cleaned, 180)
    }
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

fn dedupe_sessions(mut sessions: Vec<SessionRecord>) -> Vec<SessionRecord> {
    sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    let mut seen_ids = HashSet::new();
    let mut seen_fingerprints = HashSet::new();
    let mut deduped = Vec::new();

    for session in sessions {
        if !seen_ids.insert(session.summary.id.clone()) {
            continue;
        }

        let fingerprint = format!(
            "{}|{}|{}|{}",
            normalize_text(&session.summary.preview),
            session.summary.workspace,
            session.summary.model,
            session.summary.started_at
        );

        if !seen_fingerprints.insert(fingerprint) {
            continue;
        }

        deduped.push(session);
    }

    deduped
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("burned-{label}-{}-{nanos}", std::process::id()))
    }

    fn write_workspace_storage(
        label: &str,
        workspace_json: &str,
        file_name: &str,
        contents: &str,
    ) -> Result<PathBuf> {
        let root = unique_temp_dir(label);
        let storage_root = root.join("workspaceStorage").join("hash");
        let sessions_root = storage_root.join("chatSessions");
        fs::create_dir_all(&sessions_root)?;
        fs::write(storage_root.join("workspace.json"), workspace_json)?;
        let path = sessions_root.join(file_name);
        fs::write(&path, contents)?;
        Ok(path)
    }

    fn write_agent_session(label: &str, contents: &str) -> Result<PathBuf> {
        let root = unique_temp_dir(label);
        let session_root = root.join("session-state").join("session-id");
        fs::create_dir_all(&session_root)?;
        let path = session_root.join("events.jsonl");
        fs::write(&path, contents)?;
        Ok(path)
    }

    #[test]
    fn parse_session_file_reads_json_snapshot() -> Result<()> {
        let path = write_workspace_storage(
            "copilot-json",
            r#"{"folder":"file:///Users/kbaicai/Documents/codex_workspace/product"}"#,
            "session.json",
            r#"{
  "sessionId": "copilot-session-1",
  "creationDate": 1775886246000,
  "lastMessageDate": 1775886846000,
  "customTitle": "Review the new connector",
  "requests": [
    {
      "modelId": "copilot/gpt-4o-mini",
      "message": {
        "text": "Review the new connector implementation"
      }
    }
  ]
}"#,
        )?;

        let parsed = parse_session_file(&path)?.expect("session");
        fs::remove_dir_all(path.parent().and_then(Path::parent).and_then(Path::parent).expect("temp root")).ok();

        assert_eq!(parsed.summary.id, "copilot-session-1");
        assert_eq!(parsed.summary.title, "Review the new connector");
        assert_eq!(parsed.summary.preview, "Review the new connector implementation");
        assert_eq!(parsed.summary.workspace, "product");
        assert_eq!(parsed.summary.model, "gpt-4o-mini");
        assert_eq!(parsed.summary.total_tokens, 0);
        assert_eq!(parsed.summary.calculation_method, CalculationMethod::Derived);
        Ok(())
    }

    #[test]
    fn parse_session_file_applies_jsonl_patches() -> Result<()> {
        let path = write_workspace_storage(
            "copilot-jsonl",
            r#"{"folder":"file:///Users/kbaicai/Documents/work/%E9%82%AE%E8%AE%A2%E9%98%85/query_scenario_discovery"}"#,
            "session.jsonl",
            r#"{"kind":0,"v":{"sessionId":"copilot-session-2","creationDate":1775918735000,"lastMessageDate":1775918735000,"requests":[],"selectedModel":{"identifier":"copilot/auto"}}}
{"kind":1,"k":["customTitle"],"v":"强化学习与智能体的深度探讨"}
{"kind":1,"k":["lastMessageDate"],"v":1775919022000}
{"kind":2,"k":["requests"],"v":[{"modelId":"copilot/claude-opus-4.6","resolvedModel":"claude-opus-4-6","message":{"text":"帮我定一下标题和 slogan"}}]}"#,
        )?;

        let parsed = parse_session_file(&path)?.expect("session");
        fs::remove_dir_all(path.parent().and_then(Path::parent).and_then(Path::parent).expect("temp root")).ok();

        assert_eq!(parsed.summary.id, "copilot-session-2");
        assert_eq!(parsed.summary.title, "强化学习与智能体的深度探讨");
        assert_eq!(parsed.summary.preview, "帮我定一下标题和 slogan");
        assert_eq!(parsed.summary.workspace, "query_scenario_discovery");
        assert_eq!(parsed.summary.model, "claude-opus-4-6");
        Ok(())
    }

    #[test]
    fn parse_agent_session_file_reads_native_usage_totals() -> Result<()> {
        let path = write_agent_session(
            "copilot-agent",
            r#"{"type":"session.start","data":{"sessionId":"agent-session-1","startTime":"2026-04-13T01:52:55.020Z","selectedModel":"claude-sonnet-4.6","context":{"cwd":"/Users/kbaicai/Documents/codex_workspace/product"}}}
{"type":"user.message","data":{"content":"帮我做一个 accounting visualization"}}
{"type":"session.shutdown","data":{"currentModel":"claude-sonnet-4.6","modelMetrics":{"claude-sonnet-4.6":{"requests":{"count":5,"cost":1},"usage":{"inputTokens":129755,"outputTokens":16496,"cacheReadTokens":91485,"cacheWriteTokens":0}}}},"timestamp":"2026-04-13T01:56:37.254Z"}"#,
        )?;

        let parsed = parse_agent_session_file(&path)?;
        fs::remove_dir_all(
            path.parent()
                .and_then(Path::parent)
                .and_then(Path::parent)
                .expect("temp root"),
        )
        .ok();

        assert_eq!(parsed.usage_events.len(), 1);
        assert_eq!(parsed.usage_events[0].model, "claude-sonnet-4.6");
        assert_eq!(parsed.usage_events[0].token_breakdown.input_tokens, 129755);
        assert_eq!(
            parsed.usage_events[0].token_breakdown.cached_input_tokens,
            91485
        );
        assert_eq!(parsed.usage_events[0].token_breakdown.output_tokens, 16496);
        assert_eq!(parsed.usage_events[0].total_tokens, 237736);

        let session = parsed.session.expect("session");
        assert_eq!(session.summary.id, "agent-session-1");
        assert_eq!(session.summary.workspace, "product");
        assert_eq!(session.summary.model, "claude-sonnet-4.6");
        assert_eq!(session.summary.total_tokens, 237736);
        assert_eq!(session.summary.calculation_method, CalculationMethod::Native);
        assert_eq!(session.summary.status, "completed");
        Ok(())
    }
}
