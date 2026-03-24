pub mod antigravity;
pub mod cherry_studio;
pub mod claude_code;
pub mod codex;
pub mod cursor;

use std::sync::{Arc, Mutex, OnceLock};

use chrono::{DateTime, Utc};

use crate::models::{CalculationMethod, SessionSummary, SourceStatus};
use crate::pricing::{estimate_cost_usd, TokenBreakdown};

type ScanDetailHook = Arc<dyn Fn(String, String) + Send + Sync>;
static SCAN_DETAIL_HOOK: OnceLock<Mutex<Option<ScanDetailHook>>> = OnceLock::new();

#[derive(Clone)]
pub struct UsageEvent {
    pub source_id: &'static str,
    pub occurred_at: DateTime<Utc>,
    pub model: String,
    pub token_breakdown: TokenBreakdown,
    pub total_tokens: u64,
    pub calculation_method: CalculationMethod,
    pub session_id: String,
}

impl UsageEvent {
    pub fn estimated_cost_usd(&self) -> Option<f64> {
        estimate_cost_usd(&self.model, self.token_breakdown)
    }
}

#[derive(Clone)]
pub struct SessionRecord {
    pub updated_at: DateTime<Utc>,
    pub summary: SessionSummary,
}

pub struct SourceReport {
    pub status: SourceStatus,
    pub usage_events: Vec<UsageEvent>,
    pub sessions: Vec<SessionRecord>,
}

pub trait SourceConnector {
    fn collect(&self) -> SourceReport;
}

pub fn set_scan_detail_hook(hook: Option<ScanDetailHook>) {
    let slot = SCAN_DETAIL_HOOK.get_or_init(|| Mutex::new(None));
    *slot.lock().expect("scan detail hook mutex poisoned") = hook;
}

pub(crate) fn report_scan_detail(source: &'static str, detail: impl Into<String>) {
    let detail = detail.into();
    let hook = SCAN_DETAIL_HOOK
        .get()
        .and_then(|slot| slot.lock().ok().and_then(|guard| guard.clone()));

    if let Some(hook) = hook {
        hook(source.to_string(), detail);
    }
}

pub fn collect_all() -> Vec<SourceReport> {
    collect_all_with_progress(|_, _, _| {})
}

pub fn collect_all_with_progress<F>(mut on_progress: F) -> Vec<SourceReport>
where
    F: FnMut(usize, usize, &str),
{
    collect_with_progress(default_connectors(), &mut on_progress)
}

fn default_connectors() -> Vec<(&'static str, Box<dyn SourceConnector>)> {
    vec![
        ("Codex", Box::new(codex::CodexConnector)),
        ("Claude Code", Box::new(claude_code::ClaudeCodeConnector)),
        ("Cherry Studio", Box::new(cherry_studio::CherryStudioConnector)),
        ("Cursor", Box::new(cursor::CursorConnector)),
        ("Antigravity", Box::new(antigravity::AntigravityConnector)),
    ]
}

fn collect_with_progress<F>(
    connectors: Vec<(&'static str, Box<dyn SourceConnector>)>,
    on_progress: &mut F,
) -> Vec<SourceReport>
where
    F: FnMut(usize, usize, &str),
{
    let total = connectors.len();
    connectors
        .into_iter()
        .enumerate()
        .map(|(index, (label, connector))| {
            on_progress(index, total, label);
            connector.collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{SourceState, SourceStatus};

    struct FakeConnector {
        id: &'static str,
        name: &'static str,
    }

    impl SourceConnector for FakeConnector {
        fn collect(&self) -> SourceReport {
            SourceReport {
                status: SourceStatus {
                    id: self.id.into(),
                    name: self.name.into(),
                    state: SourceState::Ready,
                    capabilities: Vec::new(),
                    note: String::new(),
                    local_path: None,
                    session_count: None,
                    last_seen_at: None,
                },
                usage_events: Vec::new(),
                sessions: Vec::new(),
            }
        }
    }

    #[test]
    fn collect_with_progress_reports_completed_connector_count() {
        let connectors: Vec<(&'static str, Box<dyn SourceConnector>)> = vec![
            (
                "Codex",
                Box::new(FakeConnector {
                    id: "codex",
                    name: "Codex",
                }),
            ),
            (
                "Claude Code",
                Box::new(FakeConnector {
                    id: "claude_code",
                    name: "Claude Code",
                }),
            ),
        ];
        let mut progress = Vec::new();

        let reports = collect_with_progress(connectors, &mut |completed, total, label| {
            progress.push((completed, total, label.to_string()));
        });

        assert_eq!(reports.len(), 2);
        assert_eq!(
            progress,
            vec![
                (0, 2, "Codex".to_string()),
                (1, 2, "Claude Code".to_string()),
            ]
        );
    }
}
