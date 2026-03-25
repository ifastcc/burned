use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Result};
use tiny_http::{Header, Response, Server, StatusCode};

#[derive(Default)]
struct ScanProgressState {
    completed: usize,
    total: usize,
    label: String,
    detail: Option<String>,
}

fn main() {
    if let Err(error) = dispatch() {
        eprintln!("burned: {error:#}");
        std::process::exit(1);
    }
}

fn dispatch() -> Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [] => run_server(),
        [scope, command] if scope == "config" && command == "show" => {
            println!("{}", burned_lib::load_app_settings_json().context("serialize app settings")?);
            Ok(())
        }
        [scope, command, key, value]
            if scope == "config" && command == "set" && key == "cherry-backup-dir" =>
        {
            let settings =
                burned_lib::update_cherry_backup_dir(value.to_string()).map_err(anyhow::Error::msg)?;
            println!(
                "Saved Cherry Studio backup dir: {}",
                settings
                    .cherry_studio
                    .preferred_backup_dir
                    .unwrap_or_else(|| "(none)".into())
            );
            Ok(())
        }
        [scope, command, key]
            if scope == "config" && command == "clear" && key == "cherry-backup-dir" =>
        {
            burned_lib::reset_cherry_backup_dir().map_err(anyhow::Error::msg)?;
            println!("Cleared Cherry Studio backup dir.");
            Ok(())
        }
        _ => Err(anyhow!(
            "usage:\n  burned\n  burned config show\n  burned config set cherry-backup-dir <path>\n  burned config clear cherry-backup-dir"
        )),
    }
}

fn run_server() -> Result<()> {
    let dist_dir = find_dist_dir()?;
    let (server, port) = bind_server()?;
    let initial_snapshot = Arc::new(Mutex::new(Some(scan_initial_snapshot()?)));
    let url = format!("http://127.0.0.1:{port}/");

    println!("Burned dashboard is running at {url}");
    println!("Press Ctrl+C to stop the local server.");

    let _ = webbrowser::open(&url);

    for request in server.incoming_requests() {
        if let Err(error) = handle_request(request, &dist_dir, &initial_snapshot) {
            eprintln!("burned: request handling failed: {error:#}");
        }
    }

    Ok(())
}

fn scan_initial_snapshot() -> Result<String> {
    let progress_state = Arc::new(Mutex::new(ScanProgressState::default()));
    burned_lib::set_scan_detail_hook(Some({
        let progress_state = Arc::clone(&progress_state);
        Arc::new(move |label: String, detail: String| {
            let mut state = progress_state
                .lock()
                .expect("Burned scan progress mutex poisoned");
            state.label = label;
            state.detail = Some(detail);
            if state.total > 0 {
                print_progress_line(
                    state.completed,
                    state.total,
                    &state.label,
                    state.detail.as_deref(),
                );
            }
        })
    }));

    let body = burned_lib::build_dashboard_snapshot_json_with_progress({
        let progress_state = Arc::clone(&progress_state);
        move |completed, total, label| {
            let mut state = progress_state
                .lock()
                .expect("Burned scan progress mutex poisoned");
            state.completed = completed;
            state.total = total;
            state.label = label.to_string();
            state.detail = None;
            print_progress_line(completed, total, label, None);
        }
    })
    .context("serialize dashboard snapshot");
    burned_lib::set_scan_detail_hook(None);
    let body = body?;

    let total_steps = progress_state
        .lock()
        .expect("Burned scan progress mutex poisoned")
        .total;
    if total_steps > 0 {
        print_completion_line(total_steps, "Initial scan complete");
        println!();
    }

    Ok(body)
}

fn bind_server() -> Result<(Server, u16)> {
    for port in 47831..47851 {
        if let Ok(server) = Server::http(format!("127.0.0.1:{port}")) {
            return Ok((server, port));
        }
    }

    Err(anyhow!(
        "could not bind a local port for Burned between 47831 and 47850"
    ))
}

fn find_dist_dir() -> Result<PathBuf> {
    let current_dir = env::current_dir().context("resolve current working directory")?;
    let candidates = [
        current_dir.join("dist"),
        current_dir.join("../dist"),
        current_dir.join("src-tauri").join("../dist"),
    ];

    for candidate in candidates {
        if candidate.join("index.html").exists() {
            return Ok(candidate);
        }
    }

    Err(anyhow!(
        "dist/index.html was not found. Run `pnpm build` in the Burned workspace first."
    ))
}

fn handle_request(
    mut request: tiny_http::Request,
    dist_dir: &Path,
    initial_snapshot: &Arc<Mutex<Option<String>>>,
) -> Result<()> {
    let request_path = request.url().split('?').next().unwrap_or("/");

    if request_path == "/api/snapshot" {
        let body = initial_snapshot
            .lock()
            .expect("Burned initial snapshot mutex poisoned")
            .take()
            .map(Ok)
            .unwrap_or_else(|| {
                burned_lib::build_dashboard_snapshot_json().context("serialize dashboard snapshot")
            })?;
        let response = Response::from_string(body)
            .with_status_code(StatusCode(200))
            .with_header(content_type_header("application/json; charset=utf-8"));
        request.respond(response).context("respond with snapshot")?;
        return Ok(());
    }

    if let Some(source_id) = request_path.strip_prefix("/api/sources/") {
        match burned_lib::build_source_snapshot_json(source_id) {
            Ok(body) => {
                let response = Response::from_string(body)
                    .with_status_code(StatusCode(200))
                    .with_header(content_type_header("application/json; charset=utf-8"));
                request
                    .respond(response)
                    .context("respond with source snapshot")?;
            }
            Err(error) => {
                let response = Response::from_string(error)
                    .with_status_code(StatusCode(404))
                    .with_header(content_type_header("text/plain; charset=utf-8"));
                request
                    .respond(response)
                    .context("respond with missing source snapshot")?;
            }
        }
        return Ok(());
    }

    if request_path == "/api/settings" {
        let body = burned_lib::load_app_settings_json().context("serialize app settings")?;
        let response = Response::from_string(body)
            .with_status_code(StatusCode(200))
            .with_header(content_type_header("application/json; charset=utf-8"));
        request.respond(response).context("respond with settings")?;
        return Ok(());
    }

    if request_path == "/api/settings/cherry-backup-dir" && request.method().as_str() == "POST" {
        let mut body = String::new();
        request
            .as_reader()
            .read_to_string(&mut body)
            .context("read Cherry backup dir request body")?;
        let payload: serde_json::Value =
            serde_json::from_str(&body).context("parse Cherry backup dir payload")?;
        let path = payload
            .get("path")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow!("Cherry backup dir payload is missing `path`"))?;
        let settings = match burned_lib::update_cherry_backup_dir(path.to_string()) {
            Ok(settings) => settings,
            Err(error) => {
                let response = Response::from_string(error)
                    .with_status_code(StatusCode(400))
                    .with_header(content_type_header("text/plain; charset=utf-8"));
                request
                    .respond(response)
                    .context("respond with Cherry backup dir validation error")?;
                return Ok(());
            }
        };
        let response = Response::from_string(
            serde_json::to_string(&settings).context("serialize updated settings")?,
        )
        .with_status_code(StatusCode(200))
        .with_header(content_type_header("application/json; charset=utf-8"));
        request
            .respond(response)
            .context("respond with updated settings")?;
        return Ok(());
    }

    if request_path == "/api/settings/cherry-backup-dir" && request.method().as_str() == "DELETE" {
        let settings = match burned_lib::reset_cherry_backup_dir() {
            Ok(settings) => settings,
            Err(error) => {
                let response = Response::from_string(error)
                    .with_status_code(StatusCode(400))
                    .with_header(content_type_header("text/plain; charset=utf-8"));
                request
                    .respond(response)
                    .context("respond with Cherry backup dir clear error")?;
                return Ok(());
            }
        };
        let response = Response::from_string(
            serde_json::to_string(&settings).context("serialize cleared settings")?,
        )
        .with_status_code(StatusCode(200))
        .with_header(content_type_header("application/json; charset=utf-8"));
        request
            .respond(response)
            .context("respond with cleared settings")?;
        return Ok(());
    }

    let asset_path = resolve_asset_path(dist_dir, request_path);
    if let Some(path) = asset_path.filter(|path| path.is_file()) {
        let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        let response = Response::from_data(bytes)
            .with_status_code(StatusCode(200))
            .with_header(content_type_for_path(&path));
        request
            .respond(response)
            .context("respond with static asset")?;
        return Ok(());
    }

    if request_path.starts_with("/api/") {
        let response = Response::from_string("Not found").with_status_code(StatusCode(404));
        request.respond(response).context("respond with 404")?;
        return Ok(());
    }

    let index_path = dist_dir.join("index.html");
    let html = fs::read(&index_path).with_context(|| format!("read {}", index_path.display()))?;
    let response = Response::from_data(html)
        .with_status_code(StatusCode(200))
        .with_header(content_type_header("text/html; charset=utf-8"));
    request
        .respond(response)
        .context("respond with index.html")?;

    Ok(())
}

fn resolve_asset_path(dist_dir: &Path, request_path: &str) -> Option<PathBuf> {
    let trimmed = request_path.trim_start_matches('/');
    if trimmed.is_empty() {
        return Some(dist_dir.join("index.html"));
    }

    let relative = Path::new(trimmed);
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return None;
    }

    Some(dist_dir.join(relative))
}

fn content_type_for_path(path: &Path) -> Header {
    let content_type = match path.extension().and_then(|extension| extension.to_str()) {
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        _ => "text/html; charset=utf-8",
    };

    content_type_header(content_type)
}

fn content_type_header(value: &str) -> Header {
    Header::from_bytes(b"Content-Type", value.as_bytes())
        .expect("valid content-type header for Burned")
}

fn print_progress_line(completed: usize, total: usize, label: &str, detail: Option<&str>) {
    let line = render_progress_line(completed, total, label, detail);
    print!("\r{line:<120}");
    let _ = io::stdout().flush();
}

fn print_completion_line(total: usize, label: &str) {
    let line = render_completion_line(total, label);
    print!("\r{line:<80}");
    let _ = io::stdout().flush();
}

fn render_progress_line(
    completed: usize,
    total: usize,
    label: &str,
    detail: Option<&str>,
) -> String {
    let bar = progress_bar(completed, total);
    if let Some(detail) = detail.filter(|detail| !detail.is_empty()) {
        format!("{bar} {completed}/{total} Scanning {label} - {detail}")
    } else {
        format!("{bar} {completed}/{total} Scanning {label}")
    }
}

fn render_completion_line(total: usize, label: &str) -> String {
    let bar = progress_bar(total, total);
    format!("{bar} {total}/{total} {label}")
}

fn progress_bar(completed: usize, total: usize) -> String {
    let width = 10usize;
    let filled = if total == 0 {
        0
    } else {
        completed.saturating_mul(width) / total
    }
    .min(width);
    let empty = width.saturating_sub(filled);

    format!("[{}{}]", "#".repeat(filled), "-".repeat(empty))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_progress_line_formats_a_readable_progress_bar() {
        assert_eq!(
            render_progress_line(2, 5, "Claude Code", None),
            "[####------] 2/5 Scanning Claude Code"
        );
    }

    #[test]
    fn render_progress_line_includes_detail_when_present() {
        assert_eq!(
            render_progress_line(2, 5, "Cherry Studio", Some("Transcript 4/8")),
            "[####------] 2/5 Scanning Cherry Studio - Transcript 4/8"
        );
    }
}
