use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Local, Utc};
use rusqlite::{params, Connection, OpenFlags};
use serde::Deserialize;
use serde_json::Value;
use zip::ZipArchive;

use crate::connectors::{
    report_scan_detail, SessionRecord, SourceConnector, SourceReport, UsageEvent,
};
use crate::models::{
    CalculationMethod, PricingCoverage, SessionRole, SessionSummary, SourceState, SourceStatus,
};
use crate::pricing::TokenBreakdown;
use crate::settings::{default_cherry_backup_dir, load_app_settings, CherryStudioSettings};

const SOURCE_ID: &str = "cherry_studio";
const SOURCE_NAME: &str = "Cherry Studio";
const HISTORY_TOPIC_LIMIT: usize = 48;
const TRANSCRIPT_ENRICH_LIMIT: usize = 8;

pub struct CherryStudioConnector;

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiServerProfile {
    #[serde(rename = "baseURL", alias = "baseUrl")]
    base_url: Option<String>,
    api_key: Option<String>,
    enabled: Option<bool>,
    updated_at: Option<String>,
}

#[derive(Default, Deserialize)]
struct TopicListResponse {
    #[serde(default)]
    topics: Vec<HistoryTopic>,
    total: Option<u32>,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryTopic {
    topic_id: String,
    topic_name: Option<String>,
    assistant_name: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    first_message_at: Option<String>,
    last_message_at: Option<String>,
    preview: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptResponse {
    #[serde(default)]
    messages: Vec<TranscriptMessage>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptMessage {
    role: String,
    model_id: Option<String>,
    annotations: Option<Value>,
}

#[derive(Default)]
struct AgentSessionAccumulator {
    id: String,
    title: Option<String>,
    preview: Option<String>,
    agent_name: Option<String>,
    model: Option<String>,
    created_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
    total_tokens: u64,
}

#[derive(Default)]
struct BackupImport {
    archive_path: PathBuf,
    sessions: Vec<SessionRecord>,
    usage_events: Vec<UsageEvent>,
    topic_count: u32,
    last_seen_at: Option<String>,
}

#[derive(Clone, Default)]
struct BackupTopicMeta {
    title: Option<String>,
    assistant_name: Option<String>,
    created_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
}

#[derive(Default)]
struct BackupTopicAccumulator {
    id: String,
    title: Option<String>,
    preview: Option<String>,
    assistant_name: Option<String>,
    model: Option<String>,
    created_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
    total_tokens: u64,
}

#[derive(Default)]
struct UsageMetrics {
    model: Option<String>,
    token_breakdown: TokenBreakdown,
}

impl UsageMetrics {
    fn total_tokens(&self) -> u64 {
        self.token_breakdown.total_tokens()
    }
}

impl SourceConnector for CherryStudioConnector {
    fn collect(&self) -> SourceReport {
        let settings = load_app_settings().unwrap_or_default();
        let cherry_settings = &settings.cherry_studio;
        collect_cherry_studio().unwrap_or_else(|error| SourceReport {
            status: SourceStatus {
                id: SOURCE_ID.into(),
                name: SOURCE_NAME.into(),
                state: SourceState::Partial,
                capabilities: default_capabilities(),
                note: format!("Cherry Studio was detected, but ingestion failed: {error}"),
                local_path: preferred_root()
                    .map(display_path)
                    .or_else(|| configured_backup_dir(cherry_settings)),
                session_count: None,
                last_seen_at: None,
            },
            usage_events: Vec::new(),
            sessions: Vec::new(),
        })
    }
}

fn collect_cherry_studio() -> Result<SourceReport> {
    let settings = load_app_settings().unwrap_or_default();
    let cherry_settings = &settings.cherry_studio;
    let backup_count = backup_zip_count(cherry_settings);
    let backup_import = latest_legacy_backup_import(cherry_settings).ok().flatten();
    let root = preferred_root();

    if root.is_none() && backup_import.is_none() {
        return Ok(missing_report(cherry_settings));
    }

    let data_dir = root.as_ref().map(|root| root.join("Data"));
    let api_profile_path = data_dir
        .as_ref()
        .map(|data_dir| data_dir.join("api-server.json"));
    let profile = api_profile_path
        .as_ref()
        .and_then(|api_profile_path| load_api_profile(api_profile_path).ok());

    let history_result = profile
        .as_ref()
        .filter(|profile| profile.enabled.unwrap_or(false))
        .map(fetch_history_sessions)
        .transpose();

    let (history_sessions, history_total, history_ok, history_error) = match history_result {
        Ok(Some((sessions, total))) => (sessions, Some(total), true, None),
        Ok(None) => (Vec::new(), None, false, None),
        Err(error) => (Vec::new(), None, false, Some(error.to_string())),
    };

    let agent_db_path = data_dir.as_ref().map(|data_dir| data_dir.join("agents.db"));
    let (agent_sessions, usage_events, agent_session_count) =
        if let Some(agent_db_path) = agent_db_path.as_ref().filter(|path| path.exists()) {
            collect_agent_sessions(agent_db_path)?
        } else {
            (Vec::new(), Vec::new(), 0)
        };
    let mut sessions_by_id = HashMap::new();
    for session in history_sessions {
        sessions_by_id.insert(session.summary.id.clone(), session);
    }
    for session in agent_sessions {
        sessions_by_id.insert(session.summary.id.clone(), session);
    }

    let mut usage_events = usage_events;
    let mut backup_topic_count = 0;
    let mut backup_native_topics = 0;
    let backup_last_seen_at = backup_import
        .as_ref()
        .and_then(|import| import.last_seen_at.clone());
    let backup_archive_path = backup_import
        .as_ref()
        .map(|import| display_path(import.archive_path.clone()));

    if let Some(import) = backup_import.as_ref() {
        backup_topic_count = import.topic_count;
        let backup_applied =
            apply_backup_import(&mut sessions_by_id, &mut usage_events, import, history_ok);
        if backup_applied {
            backup_native_topics = import
                .sessions
                .iter()
                .filter(|session| session.summary.total_tokens > 0)
                .count() as u32;
        }
    }

    let mut sessions = sessions_by_id.into_values().collect::<Vec<_>>();
    sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    sessions.truncate(12);

    let ordinary_topics = if history_ok {
        history_total.unwrap_or(0)
    } else {
        history_total.unwrap_or(0).max(backup_topic_count)
    };
    let session_count = if ordinary_topics > 0 || agent_session_count > 0 {
        Some(ordinary_topics.saturating_add(agent_session_count))
    } else {
        None
    };

    let state = if !usage_events.is_empty() {
        SourceState::Ready
    } else {
        SourceState::Partial
    };

    let note = if history_ok {
        let topic_total = history_total.unwrap_or(ordinary_topics);
        if !usage_events.is_empty() {
            format!(
                "Local History API is live for {topic_total} ordinary topics. Session timing comes from live history timestamps, agent sessions are scanned from agents.db, and legacy backup zips stay on standby as fallback{}.",
                backup_note(backup_count)
            )
        } else {
            format!(
                "Local History API is live for {topic_total} ordinary topics. Session timing comes from live history timestamps; ordinary topic token usage still needs a fallback source, so legacy backup zips are kept on standby{}.",
                backup_note(backup_count)
            )
        }
    } else if backup_native_topics > 0 {
        format!(
            "The local History API is unavailable, so Burned fell back to the latest legacy backup zip and recovered {backup_native_topics} topic(s) with native token usage. Agent sessions are still scanned from agents.db{}.",
            backup_note(backup_count)
        )
    } else if let Some(error) = history_error {
        format!(
            "Cherry Studio data is present, but the local History API could not be read: {error}. Burned can still scan agent sessions from agents.db and look for compatible backup exports{}.",
            backup_note(backup_count)
        )
    } else if agent_session_count > 0 {
        format!(
            "agents.db is present and agent sessions were indexed. Ordinary Cherry topics need either the local History API or a compatible legacy backup zip{}.",
            backup_note(backup_count)
        )
    } else {
        format!(
            "Cherry Studio data directories were found, but no ordinary topics or agent sessions could be indexed yet{}.",
            backup_note(backup_count)
        )
    };

    let last_seen_at = profile
        .as_ref()
        .and_then(|profile| profile.updated_at.as_deref())
        .and_then(parse_rfc3339)
        .map(|timestamp| {
            timestamp
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .or_else(|| {
            api_profile_path
                .as_ref()
                .and_then(|path| format_mtime(path).ok())
        })
        .or_else(|| {
            agent_db_path
                .as_ref()
                .and_then(|path| format_mtime(path).ok())
        })
        .or(backup_last_seen_at)
        .or_else(|| data_dir.as_ref().and_then(|path| format_mtime(path).ok()));

    Ok(SourceReport {
        status: SourceStatus {
            id: SOURCE_ID.into(),
            name: SOURCE_NAME.into(),
            state,
            capabilities: default_capabilities(),
            note,
            local_path: root.clone().map(display_path).or(backup_archive_path),
            session_count,
            last_seen_at,
        },
        usage_events,
        sessions,
    })
}

fn fetch_history_sessions(profile: &ApiServerProfile) -> Result<(Vec<SessionRecord>, u32)> {
    let base_url = profile
        .base_url
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("api-server.json is missing baseURL"))?;
    let api_key = profile
        .api_key
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("api-server.json is missing apiKey"))?;

    let agent = history_agent();
    let topics_url = format!(
        "{}/history/topics?limit={}",
        base_url.trim_end_matches('/'),
        HISTORY_TOPIC_LIMIT
    );
    let response = agent
        .get(&topics_url)
        .set("X-API-Key", api_key)
        .call()
        .with_context(|| format!("request {topics_url}"))?;
    let payload: TopicListResponse = response
        .into_json()
        .context("parse Cherry Studio history topics response")?;
    report_scan_detail(
        SOURCE_NAME,
        format!("Loaded {} recent topics", payload.topics.len()),
    );

    let model_map = fetch_topic_model_map(&agent, base_url, api_key, &payload.topics);
    let mut sessions = payload
        .topics
        .iter()
        .filter_map(|topic| topic_to_session(topic, model_map.get(&topic.topic_id)))
        .collect::<Vec<_>>();

    sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    sessions.truncate(12);

    Ok((
        sessions,
        payload.total.unwrap_or(payload.topics.len() as u32),
    ))
}

fn fetch_topic_model_map(
    agent: &ureq::Agent,
    base_url: &str,
    api_key: &str,
    topics: &[HistoryTopic],
) -> HashMap<String, String> {
    let mut model_map = HashMap::new();
    let enrich_count = topics.len().min(TRANSCRIPT_ENRICH_LIMIT);

    for (index, topic) in topics.iter().take(TRANSCRIPT_ENRICH_LIMIT).enumerate() {
        report_scan_detail(
            SOURCE_NAME,
            format!("Transcript {}/{}", index + 1, enrich_count),
        );
        let transcript_url = format!(
            "{}/history/topics/{}/transcript?limit=20",
            base_url.trim_end_matches('/'),
            topic.topic_id
        );

        let response = match agent.get(&transcript_url).set("X-API-Key", api_key).call() {
            Ok(response) => response,
            Err(_) => continue,
        };
        let transcript: TranscriptResponse = match response.into_json() {
            Ok(transcript) => transcript,
            Err(_) => continue,
        };

        if let Some(model) = transcript_model(&transcript.messages) {
            model_map.insert(topic.topic_id.clone(), model);
        }
    }

    model_map
}

fn topic_to_session(topic: &HistoryTopic, model: Option<&String>) -> Option<SessionRecord> {
    let created_at = topic
        .first_message_at
        .as_deref()
        .and_then(parse_rfc3339)
        .or_else(|| topic.created_at.as_deref().and_then(parse_rfc3339))
        .or_else(|| topic.last_message_at.as_deref().and_then(parse_rfc3339))
        .or_else(|| topic.updated_at.as_deref().and_then(parse_rfc3339))?;
    let updated_at = topic
        .last_message_at
        .as_deref()
        .and_then(parse_rfc3339)
        .or_else(|| topic.updated_at.as_deref().and_then(parse_rfc3339))
        .or_else(|| topic.created_at.as_deref().and_then(parse_rfc3339))
        .unwrap_or(created_at);

    let title = topic
        .topic_name
        .as_deref()
        .map(normalize_text)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            topic
                .preview
                .as_deref()
                .map(normalize_text)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "Untitled Cherry topic".into());
    let preview = topic
        .preview
        .as_deref()
        .map(normalize_text)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "No preview available.".into());

    Some(SessionRecord {
        updated_at,
        summary: SessionSummary {
            id: topic.topic_id.clone(),
            source_id: SOURCE_ID.into(),
            title: truncate(&title, 72),
            preview: truncate(&preview, 180),
            source: SOURCE_NAME.into(),
            workspace: topic
                .assistant_name
                .as_deref()
                .map(normalize_text)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "history".into()),
            model: model.cloned().unwrap_or_else(|| "unknown".into()),
            started_at: created_at
                .with_timezone(&Local)
                .format("%b %-d %H:%M")
                .to_string(),
            total_tokens: 0,
            cost_usd: 0.0,
            priced_sessions: 0,
            pending_pricing_sessions: 0,
            pricing_coverage: PricingCoverage::Pending,
            pricing_state: "pending".into(),
            calculation_method: CalculationMethod::Estimated,
            status: "indexed".into(),
            parent_session_id: None,
            session_role: SessionRole::Primary,
            agent_label: None,
        },
    })
}

fn apply_backup_import(
    sessions_by_id: &mut HashMap<String, SessionRecord>,
    usage_events: &mut Vec<UsageEvent>,
    import: &BackupImport,
    history_ok: bool,
) -> bool {
    if history_ok {
        return false;
    }

    report_scan_detail(SOURCE_NAME, "Parsing fallback backup".to_string());
    usage_events.extend(import.usage_events.iter().cloned());

    for session in &import.sessions {
        let session_id = session.summary.id.clone();
        if let Some(existing) = sessions_by_id.get_mut(&session_id) {
            merge_session_record(existing, session.clone());
        } else {
            sessions_by_id.insert(session_id, session.clone());
        }
    }

    true
}

fn collect_agent_sessions(db_path: &Path) -> Result<(Vec<SessionRecord>, Vec<UsageEvent>, u32)> {
    report_scan_detail(SOURCE_NAME, "Scanning agent sessions".to_string());
    let connection = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open {}", db_path.display()))?;
    let agent_count = connection
        .query_row("select count(*) from sessions", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap_or(0)
        .max(0) as u32;

    let mut statement = connection.prepare(
        "select s.id, s.name, s.description, s.model, s.created_at, s.updated_at, coalesce(a.name, '') \
         from sessions s left join agents a on a.id = s.agent_id order by s.updated_at desc limit 24",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;

    let mut sessions = Vec::new();
    let mut usage_events = Vec::new();

    for row in rows {
        let (session_id, name, description, model, created_at_raw, updated_at_raw, agent_name) =
            row?;
        let mut accumulator = AgentSessionAccumulator {
            id: session_id.clone(),
            title: Some(normalize_text(&name)),
            preview: description
                .as_deref()
                .map(normalize_text)
                .filter(|value| !value.is_empty()),
            agent_name: if agent_name.is_empty() {
                None
            } else {
                Some(agent_name)
            },
            model: Some(model),
            created_at: parse_rfc3339(&created_at_raw),
            updated_at: parse_rfc3339(&updated_at_raw),
            total_tokens: 0,
        };

        let mut message_statement = connection.prepare(
            "select role, content, metadata, created_at from session_messages where session_id = ? order by created_at asc",
        )?;
        let message_rows = message_statement.query_map(params![session_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;

        for message_row in message_rows {
            let (role, content, metadata, created_at_text) = message_row?;
            let timestamp = parse_rfc3339(&created_at_text);
            let content_value = serde_json::from_str::<Value>(&content).ok();
            let metadata_value = metadata
                .as_deref()
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok());

            if accumulator.preview.is_none() {
                let preview = content_value
                    .as_ref()
                    .and_then(extract_message_preview)
                    .filter(|value| !looks_meta_command(value))
                    .or_else(|| metadata_value.as_ref().and_then(extract_message_preview));
                if let Some(preview) = preview {
                    accumulator.preview = Some(preview);
                }
            }

            if accumulator.title.is_none() {
                let title = content_value
                    .as_ref()
                    .and_then(extract_message_preview)
                    .filter(|value| !looks_meta_command(value))
                    .map(|value| truncate(&value, 72));
                accumulator.title = title;
            }

            if accumulator.model.as_deref() == Some("unknown") || accumulator.model.is_none() {
                accumulator.model = content_value
                    .as_ref()
                    .and_then(extract_model)
                    .or_else(|| metadata_value.as_ref().and_then(extract_model))
                    .or_else(|| Some("unknown".into()));
            }

            let usage_metrics = content_value
                .as_ref()
                .and_then(extract_usage_metrics)
                .or_else(|| metadata_value.as_ref().and_then(extract_usage_metrics));
            let total_tokens = usage_metrics
                .as_ref()
                .map(UsageMetrics::total_tokens)
                .unwrap_or(0);

            if total_tokens > 0 {
                accumulator.total_tokens += total_tokens;
                if let Some(timestamp) = timestamp {
                    let usage_metrics = usage_metrics.unwrap_or_default();
                    usage_events.push(UsageEvent {
                        source_id: SOURCE_ID,
                        occurred_at: timestamp,
                        model: usage_metrics
                            .model
                            .or_else(|| accumulator.model.clone())
                            .unwrap_or_else(|| "unknown".into()),
                        token_breakdown: usage_metrics.token_breakdown,
                        total_tokens,
                        calculation_method: CalculationMethod::Native,
                        session_id: format!("agent:{session_id}"),
                        explicit_cost_usd: None,
                    });
                }
            }

            if role == "user" && accumulator.title.as_deref().unwrap_or_default().is_empty() {
                accumulator.title = content_value.as_ref().and_then(extract_message_preview);
            }
        }

        if let Some(record) = agent_accumulator_to_session(accumulator) {
            sessions.push(record);
        }
    }

    sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    sessions.truncate(12);

    Ok((sessions, usage_events, agent_count))
}

fn agent_accumulator_to_session(accumulator: AgentSessionAccumulator) -> Option<SessionRecord> {
    let created_at = accumulator.created_at.or(accumulator.updated_at)?;
    let updated_at = accumulator.updated_at.unwrap_or(created_at);
    let total_tokens = accumulator.total_tokens;
    let calculation_method = if total_tokens > 0 {
        CalculationMethod::Native
    } else {
        CalculationMethod::Estimated
    };

    Some(SessionRecord {
        updated_at,
        summary: SessionSummary {
            id: format!("agent:{}", accumulator.id),
            source_id: SOURCE_ID.into(),
            title: accumulator
                .title
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "Untitled Cherry agent session".into()),
            preview: accumulator
                .preview
                .map(|value| truncate(&value, 180))
                .unwrap_or_else(|| "No preview available.".into()),
            source: SOURCE_NAME.into(),
            workspace: accumulator.agent_name.unwrap_or_else(|| "agents".into()),
            model: accumulator.model.unwrap_or_else(|| "unknown".into()),
            started_at: created_at
                .with_timezone(&Local)
                .format("%b %-d %H:%M")
                .to_string(),
            total_tokens,
            cost_usd: 0.0,
            priced_sessions: 0,
            pending_pricing_sessions: 0,
            pricing_coverage: PricingCoverage::Pending,
            pricing_state: "pending".into(),
            calculation_method,
            status: "indexed".into(),
            parent_session_id: None,
            session_role: SessionRole::Primary,
            agent_label: None,
        },
    })
}

fn merge_session_record(existing: &mut SessionRecord, incoming: SessionRecord) {
    existing.updated_at = existing.updated_at.max(incoming.updated_at);

    if existing.summary.title.starts_with("Untitled")
        && !incoming.summary.title.starts_with("Untitled")
    {
        existing.summary.title = incoming.summary.title.clone();
    }

    if existing.summary.preview == "No preview available."
        && incoming.summary.preview != "No preview available."
    {
        existing.summary.preview = incoming.summary.preview.clone();
    }

    if (existing.summary.model == "unknown" || existing.summary.model.is_empty())
        && incoming.summary.model != "unknown"
        && !incoming.summary.model.is_empty()
    {
        existing.summary.model = incoming.summary.model.clone();
    }

    if existing.summary.workspace == "history"
        && incoming.summary.workspace != "history"
        && !incoming.summary.workspace.is_empty()
    {
        existing.summary.workspace = incoming.summary.workspace.clone();
    }

    if incoming.summary.total_tokens > existing.summary.total_tokens {
        existing.summary.total_tokens = incoming.summary.total_tokens;
        existing.summary.calculation_method = incoming.summary.calculation_method;
    }
}

fn transcript_model(messages: &[TranscriptMessage]) -> Option<String> {
    messages
        .iter()
        .find(|message| {
            message.role == "assistant"
                && message
                    .annotations
                    .as_ref()
                    .and_then(|annotations| annotations.get("isPreferredResponse"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        })
        .and_then(|message| message.model_id.clone())
        .or_else(|| {
            messages
                .iter()
                .find(|message| message.role == "assistant")
                .and_then(|message| message.model_id.clone())
        })
}

fn history_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(1200))
        .timeout_read(Duration::from_millis(2500))
        .timeout_write(Duration::from_millis(1200))
        .build()
}

fn load_api_profile(path: &Path) -> Result<ApiServerProfile> {
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))
}

fn preferred_root() -> Option<PathBuf> {
    cherry_roots().into_iter().find(|root| root.exists())
}

fn cherry_roots() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };

    vec![
        home.join("Library")
            .join("Application Support")
            .join("CherryStudio"),
        home.join("Library")
            .join("Application Support")
            .join("CherryStudioDev"),
    ]
}

fn missing_report(cherry_settings: &CherryStudioSettings) -> SourceReport {
    SourceReport {
        status: SourceStatus {
            id: SOURCE_ID.into(),
            name: SOURCE_NAME.into(),
            state: SourceState::Missing,
            capabilities: default_capabilities(),
            note: "No Cherry Studio local profile was found on this machine.".into(),
            local_path: preferred_root()
                .map(display_path)
                .or_else(|| configured_backup_dir(cherry_settings)),
            session_count: None,
            last_seen_at: None,
        },
        usage_events: Vec::new(),
        sessions: Vec::new(),
    }
}

fn default_capabilities() -> Vec<String> {
    vec![
        "history-api".into(),
        "history-transcript".into(),
        "local-agents-db".into(),
        "legacy-backup-json".into(),
        "zip-backups".into(),
    ]
}

fn backup_zip_count(cherry_settings: &CherryStudioSettings) -> usize {
    backup_archives(cherry_settings).len()
}

fn backup_note(count: usize) -> String {
    if count == 0 {
        String::new()
    } else {
        format!("; {count} Cherry backup zip(s) were also detected")
    }
}

fn backup_archives(cherry_settings: &CherryStudioSettings) -> Vec<PathBuf> {
    let mut archives = Vec::new();

    for backup_root in backup_roots(cherry_settings) {
        let Ok(entries) = fs::read_dir(backup_root) else {
            continue;
        };

        archives.extend(
            entries
                .filter_map(std::result::Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension()
                        .and_then(|extension| extension.to_str())
                        .map(|extension| extension.eq_ignore_ascii_case("zip"))
                        .unwrap_or(false)
                }),
        );
    }

    archives.sort_by(|left, right| {
        let left_mtime = fs::metadata(left).and_then(|meta| meta.modified()).ok();
        let right_mtime = fs::metadata(right).and_then(|meta| meta.modified()).ok();
        right_mtime.cmp(&left_mtime)
    });
    archives.dedup();

    archives
}

fn backup_roots(cherry_settings: &CherryStudioSettings) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(preferred) = cherry_settings.preferred_backup_dir.as_ref() {
        roots.push(PathBuf::from(preferred));
    }

    for known in &cherry_settings.known_backup_dirs {
        roots.push(PathBuf::from(known));
    }

    if let Some(default_root) = default_cherry_backup_dir() {
        roots.push(default_root);
    }

    let mut deduped = Vec::new();
    for root in roots {
        if !deduped.iter().any(|existing: &PathBuf| existing == &root) {
            deduped.push(root);
        }
    }

    deduped
}

fn configured_backup_dir(cherry_settings: &CherryStudioSettings) -> Option<String> {
    cherry_settings.preferred_backup_dir.clone()
}

fn latest_legacy_backup_import(
    cherry_settings: &CherryStudioSettings,
) -> Result<Option<BackupImport>> {
    for archive in backup_archives(cherry_settings) {
        match import_legacy_backup_archive(&archive) {
            Ok(import) => return Ok(Some(import)),
            Err(_) => continue,
        }
    }

    Ok(None)
}

fn import_legacy_backup_archive(path: &Path) -> Result<BackupImport> {
    let file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut archive =
        ZipArchive::new(file).with_context(|| format!("open zip {}", path.display()))?;
    let mut entry = archive
        .by_name("data.json")
        .with_context(|| format!("read legacy data.json from {}", path.display()))?;
    let mut raw = String::new();
    entry
        .read_to_string(&mut raw)
        .with_context(|| format!("read data.json from {}", path.display()))?;

    let data: Value = serde_json::from_str(&raw)
        .with_context(|| format!("parse data.json from {}", path.display()))?;
    let topic_meta = extract_backup_topic_meta(&data);
    let block_previews = extract_backup_block_previews(&data);
    let topic_values = extract_backup_table_values(&data, "topics");
    let archive_seen_at = format_mtime(path).ok();
    let archive_modified_at = fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .map(DateTime::<Utc>::from)
        .ok();

    let mut sessions = Vec::new();
    let mut usage_events = Vec::new();

    for topic_value in topic_values {
        if let Some((session, mut events)) =
            backup_topic_to_session(&topic_value, &topic_meta, &block_previews)
        {
            usage_events.append(&mut events);
            sessions.push(session);
        }
    }

    if let Some(archive_modified_at) = archive_modified_at {
        retain_plausible_backup_usage_events(&mut usage_events, archive_modified_at);
    }

    sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    sessions.truncate(12);

    Ok(BackupImport {
        archive_path: path.to_path_buf(),
        sessions,
        usage_events,
        topic_count: topic_meta
            .len()
            .max(extract_backup_table_values(&data, "topics").len()) as u32,
        last_seen_at: archive_seen_at,
    })
}

fn extract_backup_table_values(data: &Value, table_name: &str) -> Vec<Value> {
    if let Some(table) = data
        .get("indexedDB")
        .and_then(|value| value.get(table_name))
        .and_then(Value::as_array)
    {
        return table.clone();
    }

    if let Some(entries) = data.get("indexedDB").and_then(Value::as_array) {
        if table_name == "topics" {
            return entries
                .iter()
                .filter_map(|entry| {
                    let key = entry.get("key").and_then(Value::as_str)?;
                    if key.starts_with("topic:") {
                        entry.get("value").cloned()
                    } else {
                        None
                    }
                })
                .collect();
        }
    }

    Vec::new()
}

fn extract_backup_topic_meta(data: &Value) -> HashMap<String, BackupTopicMeta> {
    let Some(raw_persist) = data
        .get("localStorage")
        .and_then(|value| value.get("persist:cherry-studio"))
        .and_then(Value::as_str)
    else {
        return HashMap::new();
    };
    let Ok(persist) = serde_json::from_str::<Value>(raw_persist) else {
        return HashMap::new();
    };
    let Some(assistants_raw) = persist.get("assistants").and_then(Value::as_str) else {
        return HashMap::new();
    };
    let Ok(assistants_value) = serde_json::from_str::<Value>(assistants_raw) else {
        return HashMap::new();
    };

    let mut topic_meta = HashMap::new();
    if let Some(default_assistant) = assistants_value.get("defaultAssistant") {
        collect_backup_assistant_topics(default_assistant, &mut topic_meta);
    }
    if let Some(assistants) = assistants_value.get("assistants").and_then(Value::as_array) {
        for assistant in assistants {
            collect_backup_assistant_topics(assistant, &mut topic_meta);
        }
    }

    topic_meta
}

fn collect_backup_assistant_topics(
    assistant: &Value,
    topic_meta: &mut HashMap<String, BackupTopicMeta>,
) {
    let assistant_name = assistant
        .get("name")
        .and_then(Value::as_str)
        .map(normalize_text)
        .filter(|value| !value.is_empty());

    let Some(topics) = assistant.get("topics").and_then(Value::as_array) else {
        return;
    };

    for topic in topics {
        let Some(topic_id) = topic.get("id").and_then(Value::as_str) else {
            continue;
        };
        topic_meta.insert(
            topic_id.to_string(),
            BackupTopicMeta {
                title: topic
                    .get("name")
                    .and_then(Value::as_str)
                    .map(normalize_text)
                    .filter(|value| !value.is_empty()),
                assistant_name: assistant_name.clone(),
                created_at: topic
                    .get("createdAt")
                    .and_then(Value::as_str)
                    .and_then(parse_rfc3339),
                updated_at: topic
                    .get("updatedAt")
                    .and_then(Value::as_str)
                    .and_then(parse_rfc3339),
            },
        );
    }
}

fn extract_backup_block_previews(data: &Value) -> HashMap<String, String> {
    extract_backup_table_values(data, "message_blocks")
        .into_iter()
        .filter_map(|block| {
            let id = block.get("id").and_then(Value::as_str)?;
            let preview = extract_message_preview(&block)?;
            Some((id.to_string(), preview))
        })
        .collect()
}

fn backup_topic_to_session(
    topic_value: &Value,
    topic_meta: &HashMap<String, BackupTopicMeta>,
    block_previews: &HashMap<String, String>,
) -> Option<(SessionRecord, Vec<UsageEvent>)> {
    let topic_id = topic_value.get("id").and_then(Value::as_str)?.to_string();
    let mut accumulator = BackupTopicAccumulator {
        id: topic_id.clone(),
        ..Default::default()
    };
    let mut usage_events = Vec::new();

    if let Some(meta) = topic_meta.get(&topic_id) {
        accumulator.title = meta.title.clone();
        accumulator.assistant_name = meta.assistant_name.clone();
        accumulator.created_at = meta.created_at;
        accumulator.updated_at = meta.updated_at;
    }

    let messages = topic_value
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    for message in messages {
        let timestamp = message
            .get("createdAt")
            .and_then(Value::as_str)
            .and_then(parse_rfc3339)
            .or_else(|| {
                message
                    .get("updatedAt")
                    .and_then(Value::as_str)
                    .and_then(parse_rfc3339)
            });

        if accumulator.created_at.is_none() {
            accumulator.created_at = timestamp;
        }
        accumulator.updated_at = accumulator.updated_at.max(timestamp);

        if accumulator.preview.is_none() {
            accumulator.preview = extract_backup_message_preview(&message, block_previews)
                .filter(|value| !looks_meta_command(value));
        }

        if accumulator.title.is_none()
            && message.get("role").and_then(Value::as_str) == Some("user")
        {
            accumulator.title = extract_backup_message_preview(&message, block_previews)
                .filter(|value| !looks_meta_command(value))
                .map(|value| truncate(&value, 72));
        }

        if accumulator.model.as_deref() == Some("unknown") || accumulator.model.is_none() {
            accumulator.model = extract_model(&message).or_else(|| Some("unknown".into()));
        }

        let usage_metrics = extract_usage_metrics(&message);
        let total_tokens = usage_metrics
            .as_ref()
            .map(UsageMetrics::total_tokens)
            .unwrap_or(0);
        if total_tokens > 0 {
            accumulator.total_tokens += total_tokens;
            if let Some(timestamp) = timestamp {
                let usage_metrics = usage_metrics.unwrap_or_default();
                usage_events.push(UsageEvent {
                    source_id: SOURCE_ID,
                    occurred_at: timestamp,
                    model: usage_metrics
                        .model
                        .or_else(|| accumulator.model.clone())
                        .unwrap_or_else(|| "unknown".into()),
                    token_breakdown: usage_metrics.token_breakdown,
                    total_tokens,
                    calculation_method: CalculationMethod::Native,
                    session_id: topic_id.clone(),
                    explicit_cost_usd: None,
                });
            }
        }
    }

    let created_at = accumulator.created_at.or(accumulator.updated_at)?;
    let updated_at = accumulator.updated_at.unwrap_or(created_at);
    let total_tokens = accumulator.total_tokens;
    let calculation_method = if total_tokens > 0 {
        CalculationMethod::Native
    } else {
        CalculationMethod::Estimated
    };

    Some((
        SessionRecord {
            updated_at,
            summary: SessionSummary {
                id: accumulator.id,
                source_id: SOURCE_ID.into(),
                title: accumulator
                    .title
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "Untitled Cherry topic".into()),
                preview: accumulator
                    .preview
                    .map(|value| truncate(&value, 180))
                    .unwrap_or_else(|| "No preview available.".into()),
                source: SOURCE_NAME.into(),
                workspace: accumulator
                    .assistant_name
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "backup".into()),
                model: accumulator.model.unwrap_or_else(|| "unknown".into()),
                started_at: created_at
                    .with_timezone(&Local)
                    .format("%b %-d %H:%M")
                    .to_string(),
                total_tokens,
                cost_usd: 0.0,
                priced_sessions: 0,
                pending_pricing_sessions: 0,
                pricing_coverage: PricingCoverage::Pending,
                pricing_state: "pending".into(),
                calculation_method,
                status: "indexed".into(),
                parent_session_id: None,
                session_role: SessionRole::Primary,
                agent_label: None,
            },
        },
        usage_events,
    ))
}

fn extract_backup_message_preview(
    message: &Value,
    block_previews: &HashMap<String, String>,
) -> Option<String> {
    if let Some(block_ids) = message.get("blocks").and_then(Value::as_array) {
        for block_id in block_ids {
            let Some(block_id) = block_id.as_str() else {
                continue;
            };
            if let Some(preview) = block_previews.get(block_id) {
                return Some(preview.clone());
            }
        }
    }

    extract_message_preview(message)
}

fn display_path(path: PathBuf) -> String {
    path.display().to_string()
}

fn retain_plausible_backup_usage_events(
    usage_events: &mut Vec<UsageEvent>,
    archive_modified_at: DateTime<Utc>,
) {
    let latest_plausible = archive_modified_at + chrono::Duration::hours(24);
    usage_events.retain(|event| event.occurred_at <= latest_plausible);
}

fn format_mtime(path: &Path) -> Result<String> {
    let modified = fs::metadata(path)?.modified()?;
    let modified: DateTime<Local> = modified.into();
    Ok(modified.format("%Y-%m-%d %H:%M").to_string())
}

fn parse_rfc3339(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|timestamp| timestamp.with_timezone(&Utc))
}

fn extract_message_preview(value: &Value) -> Option<String> {
    extract_text(value)
        .map(|value| normalize_text(&value))
        .filter(|value| !value.is_empty())
        .map(|value| truncate(&value, 180))
}

fn extract_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.to_string()),
        Value::Array(items) => items.iter().find_map(extract_text),
        Value::Object(map) => {
            if let Some(message) = map.get("message") {
                if let Some(text) = extract_text(message) {
                    return Some(text);
                }
            }

            for key in ["mainText", "text", "content", "thinkingText"] {
                if let Some(text) = map.get(key).and_then(extract_text) {
                    return Some(text);
                }
            }

            if let Some(parts) = map.get("parts").and_then(Value::as_array) {
                for part in parts {
                    if let Some(text) = extract_text(part) {
                        return Some(text);
                    }
                }
            }

            None
        }
        _ => None,
    }
}

fn extract_model(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            if let Some(message) = map.get("message") {
                if let Some(model) = extract_model(message) {
                    return Some(model);
                }
            }

            if let Some(model) = map.get("model") {
                match model {
                    Value::String(model) => return Some(model.to_string()),
                    Value::Object(model_map) => {
                        if let Some(model_id) = model_map.get("id").and_then(Value::as_str) {
                            return Some(model_id.to_string());
                        }
                        if let Some(model_name) = model_map.get("name").and_then(Value::as_str) {
                            return Some(model_name.to_string());
                        }
                    }
                    _ => {}
                }
            }

            map.get("modelId")
                .and_then(Value::as_str)
                .map(str::to_string)
        }
        _ => None,
    }
}

fn extract_usage_metrics(value: &Value) -> Option<UsageMetrics> {
    let usage = if let Some(message) = value.get("message") {
        message.get("usage").or_else(|| value.get("usage"))
    } else {
        value.get("usage")
    };

    usage
        .and_then(usage_metrics_from_usage)
        .or_else(|| value.get("totalUsage").and_then(usage_metrics_from_usage))
        .or_else(|| value.get("metrics").and_then(usage_metrics_from_usage))
}

fn usage_metrics_from_usage(value: &Value) -> Option<UsageMetrics> {
    let direct_total = ["total_tokens", "totalTokens", "tokenCount"]
        .iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
        .unwrap_or(0);
    let input_tokens = usage_value(value, &["input_tokens", "inputTokens"]);
    let cache_creation_input_tokens = usage_value(
        value,
        &["cache_creation_input_tokens", "cacheCreationInputTokens"],
    );
    let cached_input_tokens = usage_value(
        value,
        &[
            "cache_read_input_tokens",
            "cacheReadInputTokens",
            "cached_input_tokens",
            "cachedInputTokens",
        ],
    );
    let output_tokens = usage_value(value, &["output_tokens", "outputTokens"])
        .saturating_add(usage_value(value, &["reasoning_tokens", "reasoningTokens"]))
        .saturating_add(usage_value(
            value,
            &["reasoning_output_tokens", "reasoningOutputTokens"],
        ));

    let classified_total = input_tokens
        .saturating_add(cache_creation_input_tokens)
        .saturating_add(cached_input_tokens)
        .saturating_add(output_tokens);
    let total_tokens = direct_total.max(classified_total);
    if total_tokens == 0 {
        return None;
    }

    Some(UsageMetrics {
        model: extract_model(value),
        token_breakdown: TokenBreakdown {
            input_tokens,
            cache_creation_input_tokens,
            cached_input_tokens,
            output_tokens,
            other_tokens: total_tokens.saturating_sub(classified_total),
        },
    })
}

fn usage_value(value: &Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
        .unwrap_or(0)
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

fn looks_meta_command(text: &str) -> bool {
    let trimmed = text.trim();
    let normalized = trimmed.to_ascii_lowercase();
    trimmed.starts_with("```")
        || normalized.eq_ignore_ascii_case("clear")
        || normalized.starts_with('/')
        || normalized.starts_with('{')
        || normalized.starts_with("debug")
        || normalized.starts_with("tool ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    #[test]
    fn backup_topic_without_message_timestamp_skips_usage_event() {
        let mut topic_meta = HashMap::new();
        topic_meta.insert(
            "topic-1".to_string(),
            BackupTopicMeta {
                created_at: Some(Utc.with_ymd_and_hms(2026, 3, 1, 12, 0, 0).unwrap()),
                ..Default::default()
            },
        );

        let topic = json!({
            "id": "topic-1",
            "messages": [
                {
                    "role": "assistant",
                    "usage": {
                        "total_tokens": 42
                    }
                }
            ]
        });

        let (session, events) =
            backup_topic_to_session(&topic, &topic_meta, &HashMap::new()).expect("session");

        assert_eq!(session.summary.total_tokens, 42);
        assert!(events.is_empty());
    }

    #[test]
    fn retain_plausible_backup_usage_events_discards_future_events() {
        let archive_modified_at = Utc.with_ymd_and_hms(2026, 3, 19, 0, 11, 6).unwrap();
        let mut events = vec![
            UsageEvent {
                source_id: SOURCE_ID,
                occurred_at: Utc.with_ymd_and_hms(2026, 3, 18, 12, 0, 0).unwrap(),
                model: "unknown".into(),
                token_breakdown: TokenBreakdown {
                    other_tokens: 10,
                    ..TokenBreakdown::default()
                },
                total_tokens: 10,
                calculation_method: CalculationMethod::Native,
                session_id: "topic-a".into(),
                explicit_cost_usd: None,
            },
            UsageEvent {
                source_id: SOURCE_ID,
                occurred_at: Utc.with_ymd_and_hms(2026, 3, 23, 12, 0, 0).unwrap(),
                model: "unknown".into(),
                token_breakdown: TokenBreakdown {
                    other_tokens: 20,
                    ..TokenBreakdown::default()
                },
                total_tokens: 20,
                calculation_method: CalculationMethod::Native,
                session_id: "topic-b".into(),
                explicit_cost_usd: None,
            },
        ];

        retain_plausible_backup_usage_events(&mut events, archive_modified_at);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].session_id, "topic-a");
    }

    #[test]
    fn topic_to_session_prefers_first_and_last_message_timestamps() {
        let topic = HistoryTopic {
            topic_id: "topic-1".into(),
            topic_name: Some("Topic".into()),
            assistant_name: Some("Assistant".into()),
            created_at: Some("2026-03-23T16:00:00Z".into()),
            updated_at: Some("2026-03-23T16:45:00Z".into()),
            first_message_at: Some("2026-03-23T16:05:00Z".into()),
            last_message_at: Some("2026-03-23T16:40:00Z".into()),
            preview: Some("Preview".into()),
        };

        let session = topic_to_session(&topic, Some(&"gpt-5.4".to_string())).expect("session");

        assert_eq!(session.summary.started_at, "Mar 23 12:05");
        assert_eq!(
            session.updated_at,
            Utc.with_ymd_and_hms(2026, 3, 23, 16, 40, 0).unwrap()
        );
    }

    #[test]
    fn apply_backup_import_skips_backup_when_live_history_is_available() {
        let import = BackupImport {
            sessions: vec![SessionRecord {
                updated_at: Utc.with_ymd_and_hms(2026, 3, 20, 12, 0, 0).unwrap(),
                summary: SessionSummary {
                    id: "backup-topic".into(),
                    source_id: SOURCE_ID.into(),
                    title: "Backup Topic".into(),
                    preview: "Preview".into(),
                    source: SOURCE_NAME.into(),
                    workspace: "backup".into(),
                    model: "unknown".into(),
                    started_at: "Mar 20 08:00".into(),
                    total_tokens: 123,
                    cost_usd: 0.0,
                    priced_sessions: 0,
                    pending_pricing_sessions: 0,
                    pricing_coverage: PricingCoverage::Pending,
                    pricing_state: "pending".into(),
                    calculation_method: CalculationMethod::Native,
                    status: "indexed".into(),
                    parent_session_id: None,
                    session_role: SessionRole::Primary,
                    agent_label: None,
                },
            }],
            usage_events: vec![UsageEvent {
                source_id: SOURCE_ID,
                occurred_at: Utc.with_ymd_and_hms(2026, 3, 20, 12, 0, 0).unwrap(),
                model: "unknown".into(),
                token_breakdown: TokenBreakdown {
                    other_tokens: 123,
                    ..TokenBreakdown::default()
                },
                total_tokens: 123,
                calculation_method: CalculationMethod::Native,
                session_id: "backup-topic".into(),
                explicit_cost_usd: None,
            }],
            topic_count: 1,
            ..Default::default()
        };
        let mut sessions_by_id = HashMap::new();
        let mut usage_events = Vec::new();

        let applied = apply_backup_import(&mut sessions_by_id, &mut usage_events, &import, true);

        assert!(!applied);
        assert!(sessions_by_id.is_empty());
        assert!(usage_events.is_empty());
    }

    #[test]
    fn api_server_profile_parses_uppercase_base_url_field() {
        let profile: ApiServerProfile = serde_json::from_value(json!({
            "baseURL": "http://127.0.0.1:23333/v1",
            "apiKey": "cs-sk-test",
            "enabled": true,
            "updatedAt": "2026-03-22T08:02:52.498Z"
        }))
        .expect("parse profile");

        assert_eq!(
            profile.base_url.as_deref(),
            Some("http://127.0.0.1:23333/v1")
        );
        assert_eq!(profile.api_key.as_deref(), Some("cs-sk-test"));
        assert_eq!(profile.enabled, Some(true));
        assert_eq!(
            profile.updated_at.as_deref(),
            Some("2026-03-22T08:02:52.498Z")
        );
    }
}
