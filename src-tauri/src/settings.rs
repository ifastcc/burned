use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub cherry_studio: CherryStudioSettings,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CherryStudioSettings {
    pub preferred_backup_dir: Option<String>,
    #[serde(default)]
    pub known_backup_dirs: Vec<String>,
    pub last_verified_at: Option<String>,
    pub last_success_archive: Option<String>,
}

pub fn load_app_settings() -> Result<AppSettings> {
    let path = settings_path()?;
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))
}

pub fn save_app_settings(settings: &AppSettings) -> Result<()> {
    let path = settings_path()?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("settings path has no parent directory"))?;
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;

    let raw = serde_json::to_string_pretty(settings).context("serialize app settings")?;
    fs::write(&path, raw).with_context(|| format!("write {}", path.display()))
}

pub fn settings_path() -> Result<PathBuf> {
    let Some(base_dir) = dirs::data_local_dir() else {
        return Err(anyhow!(
            "could not resolve local application data directory"
        ));
    };

    Ok(base_dir.join("Burned").join("settings.json"))
}

pub fn default_cherry_backup_dir() -> Option<PathBuf> {
    dirs::document_dir().map(|dir| dir.join("cherry_data_backup"))
}

pub fn set_cherry_backup_dir(path: &str) -> Result<AppSettings> {
    let normalized = normalize_directory_path(path)?;
    let mut settings = load_app_settings().unwrap_or_default();

    settings.cherry_studio.preferred_backup_dir = Some(normalized.clone());
    if !settings
        .cherry_studio
        .known_backup_dirs
        .iter()
        .any(|known| known == &normalized)
    {
        settings.cherry_studio.known_backup_dirs.push(normalized);
    }
    settings.cherry_studio.last_verified_at = Some(Utc::now().to_rfc3339());

    save_app_settings(&settings)?;
    Ok(settings)
}

pub fn clear_cherry_backup_dir() -> Result<AppSettings> {
    let mut settings = load_app_settings().unwrap_or_default();
    settings.cherry_studio.preferred_backup_dir = None;
    settings.cherry_studio.last_verified_at = Some(Utc::now().to_rfc3339());

    save_app_settings(&settings)?;
    Ok(settings)
}

fn normalize_directory_path(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("backup directory path cannot be empty"));
    }

    let expanded = expand_path(trimmed);
    if !expanded.exists() {
        return Err(anyhow!("backup directory does not exist"));
    }

    let canonical = expanded
        .canonicalize()
        .with_context(|| format!("canonicalize {}", expanded.display()))?;
    if !canonical.is_dir() {
        return Err(anyhow!("backup path is not a directory"));
    }

    Ok(canonical.display().to_string())
}

fn expand_path(raw: &str) -> PathBuf {
    if raw == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(raw));
    }

    if let Some(stripped) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }

    PathBuf::from(raw)
}
