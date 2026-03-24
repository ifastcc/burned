import { useEffect, useState } from "react";
import type { AppSettings, SourceStatus } from "../data/schema";
import { getCopy, sourceStateLabel, type Locale } from "../i18n";

type SourceStatusBoardProps = {
  locale: Locale;
  onClearCherryBackupDir: () => Promise<void>;
  onSaveCherryBackupDir: (path: string) => Promise<void>;
  settings: AppSettings;
  statuses: SourceStatus[];
};

export function SourceStatusBoard({
  locale,
  onClearCherryBackupDir,
  onSaveCherryBackupDir,
  settings,
  statuses
}: SourceStatusBoardProps) {
  const copy = getCopy(locale);
  const readyCount = statuses.filter((status) => status.state === "ready").length;
  const partialCount = statuses.filter((status) => status.state === "partial").length;
  const connectedCount = statuses.filter((status) => status.state !== "missing").length;
  const [cherryBackupDir, setCherryBackupDir] = useState(
    settings.cherryStudio.preferredBackupDir ?? ""
  );
  const [feedback, setFeedback] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    setCherryBackupDir(settings.cherryStudio.preferredBackupDir ?? "");
  }, [settings.cherryStudio.preferredBackupDir]);

  return (
    <details className="panel connector-drawer">
      <summary className="connector-drawer-summary">
        <div>
          <p className="panel-kicker">{copy.connectors.healthKicker}</p>
          <h2>{copy.connectors.healthTitle}</h2>
        </div>
        <p className="connector-drawer-copy">
          {copy.connectors.healthSummary(connectedCount, readyCount, partialCount)}
        </p>
      </summary>

      <div className="connector-panel">
        <div className="panel-heading connector-heading">
          <div>
            <p className="panel-kicker">{copy.connectors.surfaceKicker}</p>
            <h2>{copy.connectors.surfaceTitle}</h2>
          </div>
          <p className="connector-summary">{copy.connectors.surfaceSummary}</p>
        </div>

        <div className="connector-grid">
          {statuses.map((status) => (
            <article className="connector-card" key={status.id}>
              <div className="connector-card-top">
                <div>
                  <p className="connector-name">{status.name}</p>
                  <p className="connector-note">{status.note}</p>
                </div>
                <span className={`source-state-pill state-${status.state}`}>
                  {sourceStateLabel(locale, status.state)}
                </span>
              </div>

              <div className="connector-meta">
                <span>
                  {copy.connectors.sessionCount}:{" "}
                  {status.sessionCount == null
                    ? copy.connectors.pending
                    : status.sessionCount}
                </span>
                <span>
                  {copy.connectors.lastSeen}:{" "}
                  {status.lastSeenAt == null
                    ? copy.connectors.pending
                    : status.lastSeenAt}
                </span>
              </div>

              {status.localPath ? <p className="connector-path">{status.localPath}</p> : null}

              <div className="connector-capabilities">
                {status.capabilities.map((capability) => (
                  <span className="connector-pill" key={capability}>
                    {capability}
                  </span>
                ))}
              </div>

              {status.id === "cherry_studio" ? (
                <div className="connector-config">
                  <p className="connector-config-title">{copy.connectors.backupTitle}</p>
                  <p className="connector-config-copy">{copy.connectors.backupSummary}</p>
                  <label className="connector-config-label" htmlFor="cherry-backup-dir">
                    {copy.connectors.backupLabel}
                  </label>
                  <input
                    className="connector-input"
                    id="cherry-backup-dir"
                    onChange={(event) => setCherryBackupDir(event.target.value)}
                    placeholder={copy.connectors.backupPlaceholder}
                    type="text"
                    value={cherryBackupDir}
                  />
                  <div className="connector-actions">
                    <button
                      className="solid-button connector-action"
                      disabled={isSaving || cherryBackupDir.trim().length === 0}
                      onClick={async () => {
                        setIsSaving(true);
                        setFeedback(null);
                        try {
                          await onSaveCherryBackupDir(cherryBackupDir);
                          setFeedback(copy.connectors.backupSaved);
                        } catch {
                          setFeedback(copy.connectors.backupSaveFailed);
                        } finally {
                          setIsSaving(false);
                        }
                      }}
                      type="button"
                    >
                      {copy.connectors.backupSave}
                    </button>
                    <button
                      className="ghost-button connector-action"
                      disabled={isSaving || !settings.cherryStudio.preferredBackupDir}
                      onClick={async () => {
                        setIsSaving(true);
                        setFeedback(null);
                        try {
                          await onClearCherryBackupDir();
                          setFeedback(copy.connectors.backupCleared);
                        } catch {
                          setFeedback(copy.connectors.backupClearFailed);
                        } finally {
                          setIsSaving(false);
                        }
                      }}
                      type="button"
                    >
                      {copy.connectors.backupClear}
                    </button>
                  </div>
                  {feedback ? <p className="connector-feedback">{feedback}</p> : null}
                </div>
              ) : null}
            </article>
          ))}
        </div>
      </div>
    </details>
  );
}
