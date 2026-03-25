# Burned Two-Connector Focus Design

## Goal

Refocus Burned around two trustworthy local sources only: Codex and Claude Code.

## Product Decision

- Burned no longer treats Cursor, Cherry Studio, or Antigravity as part of the product surface.
- Those connectors are removed instead of being marked experimental, session-only, or pricing-pending.
- All dashboard totals, rankings, histories, source lists, and detail routes now operate only on Codex and Claude Code data.

## Scope

### Included

- Keep Codex ingestion, session indexing, and pricing estimation.
- Keep Claude Code ingestion, session indexing, and pricing estimation.
- Keep source detail pages for Codex and Claude Code.
- Keep homepage source/status surfaces only insofar as they describe those two remaining connectors.

### Removed

- Cursor connector module and all related references.
- Cherry Studio connector module and all related references.
- Antigravity connector module and all related references.
- Cherry backup-directory settings, CLI commands, API routes, and frontend configuration UI.
- Dead tests, fixtures, and types that only exist for the removed connectors.

## Backend Design

- `src-tauri/src/connectors/mod.rs` registers only Codex and Claude Code.
- `src-tauri/src/lib.rs` stops exposing settings commands and JSON helpers that only existed for Cherry Studio configuration.
- `src-tauri/src/settings.rs` is removed if nothing outside Cherry Studio still depends on it.
- Cargo dependencies used only by removed connectors are deleted.

## Frontend Design

- Remove connector modules and configuration flows that no longer have a backend.
- Remove dead components such as the unused source-status board if they only served the removed connector management surface.
- Keep the main dashboard and source detail views, but ensure they only receive and render Codex/Claude data.

## Verification

- Rust tests pass after connector removal.
- Frontend build passes after type and component cleanup.
- No UI or CLI path still references Cherry Studio backup configuration or the removed connectors.
