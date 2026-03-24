import { invoke } from "@tauri-apps/api/core";
import { startTransition, useEffect, useEffectEvent, useRef, useState } from "react";
import { ConnectorSessions } from "./components/ConnectorSessions";
import { MetricTile } from "./components/MetricTile";
import { SourceStatusBoard } from "./components/SourceStatusBoard";
import { TodaySources } from "./components/TodaySources";
import { TrendExplorer } from "./components/TrendExplorer";
import { createEmptyDashboardSnapshot } from "./data/empty-dashboard";
import { mockDashboard } from "./data/mock-dashboard";
import type { AppSettings, DashboardSnapshot } from "./data/schema";
import {
  detectInitialLocale,
  formatCompactNumber,
  formatFriendlyNumber,
  formatLocalizedDate,
  formatLocalizedDateTime,
  formatNumber,
  getCopy,
  type Locale
} from "./i18n";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

const shouldUseMockDashboard =
  typeof window !== "undefined" &&
  !window.__TAURI_INTERNALS__ &&
  import.meta.env.VITE_USE_MOCK_DASHBOARD === "true";

async function getDashboardSnapshot() {
  if (window.__TAURI_INTERNALS__) {
    return invoke<DashboardSnapshot>("get_dashboard_snapshot");
  }

  try {
    const response = await fetch("/api/snapshot", {
      headers: {
        Accept: "application/json"
      }
    });

    if (!response.ok) {
      throw new Error(`Snapshot request failed with ${response.status}`);
    }

    return (await response.json()) as DashboardSnapshot;
  } catch (error) {
    if (shouldUseMockDashboard) {
      return mockDashboard;
    }

    throw error;
  }
}

const defaultSettings: AppSettings = {
  cherryStudio: {
    preferredBackupDir: null,
    knownBackupDirs: [],
    lastVerifiedAt: null,
    lastSuccessArchive: null
  }
};

async function getAppSettings() {
  if (window.__TAURI_INTERNALS__) {
    return invoke<AppSettings>("get_app_settings");
  }

  try {
    const response = await fetch("/api/settings", {
      headers: {
        Accept: "application/json"
      }
    });

    if (response.ok) {
      return (await response.json()) as AppSettings;
    }
  } catch {
    // Fall back to default settings when Burned runs without a settings backend.
  }

  return defaultSettings;
}

async function saveCherryBackupDir(path: string) {
  if (window.__TAURI_INTERNALS__) {
    return invoke<AppSettings>("set_cherry_backup_dir", { path });
  }

  const response = await fetch("/api/settings/cherry-backup-dir", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Accept: "application/json"
    },
    body: JSON.stringify({ path })
  });

  if (!response.ok) {
    throw new Error("Failed to save Cherry backup dir");
  }

  return (await response.json()) as AppSettings;
}

async function clearSavedCherryBackupDir() {
  if (window.__TAURI_INTERNALS__) {
    return invoke<AppSettings>("clear_cherry_backup_dir");
  }

  const response = await fetch("/api/settings/cherry-backup-dir", {
    method: "DELETE",
    headers: {
      Accept: "application/json"
    }
  });

  if (!response.ok) {
    throw new Error("Failed to clear Cherry backup dir");
  }

  return (await response.json()) as AppSettings;
}

export default function App() {
  const [snapshot, setSnapshot] = useState<DashboardSnapshot>(() => createEmptyDashboardSnapshot());
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [locale, setLocale] = useState<Locale>(() => detectInitialLocale());
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [lastRefreshedAt, setLastRefreshedAt] = useState<string | null>(null);
  const [hasRefreshError, setHasRefreshError] = useState(false);
  const refreshInFlightRef = useRef(false);

  const refreshAppState = useEffectEvent(
    async (mode: "initial" | "manual" | "poll" | "focus" = "manual") => {
      if (refreshInFlightRef.current) {
        return;
      }

      refreshInFlightRef.current = true;
      if (mode !== "poll") {
        setIsRefreshing(true);
      }
      setHasRefreshError(false);

      try {
        const [nextSnapshot, nextSettings] = await Promise.all([
          getDashboardSnapshot(),
          getAppSettings()
        ]);
        startTransition(() => {
          setSnapshot(nextSnapshot);
          setSettings(nextSettings);
          setLastRefreshedAt(new Date().toISOString());
        });
      } catch {
        setHasRefreshError(true);
        if (mode === "initial") {
          startTransition(() => {
            setSettings(defaultSettings);
          });
        }
      } finally {
        refreshInFlightRef.current = false;
        setIsRefreshing(false);
      }
    }
  );

  useEffect(() => {
    void refreshAppState("initial");
  }, []);

  useEffect(() => {
    const interval = window.setInterval(() => {
      if (document.visibilityState === "visible") {
        void refreshAppState("poll");
      }
    }, 60_000);

    return () => window.clearInterval(interval);
  }, []);

  useEffect(() => {
    const handleFocus = () => {
      void refreshAppState("focus");
    };
    const handleVisibilityChange = () => {
      if (document.visibilityState === "visible") {
        void refreshAppState("focus");
      }
    };

    window.addEventListener("focus", handleFocus);
    document.addEventListener("visibilitychange", handleVisibilityChange);

    return () => {
      window.removeEventListener("focus", handleFocus);
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    };
  }, []);

  useEffect(() => {
    window.localStorage.setItem("burned.locale", locale);
  }, [locale]);

  const copy = getCopy(locale);
  const lastScannedLabel =
    formatLocalizedDateTime(lastRefreshedAt ?? undefined, locale) ?? lastRefreshedAt;
  const lastSevenDays = snapshot.dailyHistory.slice(-7);
  const lastSevenDayAverage =
    lastSevenDays.length === 0
      ? 0
      : Math.round(
          lastSevenDays.reduce((total, point) => total + point.totalTokens, 0) / lastSevenDays.length
        );
  const visibleSources = snapshot.sourceStatuses.filter(
    (status) => status.state !== "missing"
  );
  const sourceLabel = visibleSources
    .slice(0, 3)
    .map((status) => status.name)
    .join(" / ");
  const currentDate =
    formatLocalizedDate(snapshot.dailyHistory.at(-1)?.date, locale) ??
    formatLocalizedDate(snapshot.headlineDate, locale) ??
    snapshot.headlineDate;

  return (
    <main className="shell">
      <div className="shell-noise" />

      <section className="hero panel">
        <div className="hero-copy">
          <p className="eyebrow">{copy.app.eyebrow}</p>
          <h1>{copy.app.title}</h1>
          <p className="hero-text">{copy.app.description(currentDate)}</p>
        </div>
        <div className="hero-aside">
          <div className="hero-chip">{copy.app.historyChip}</div>

          <div className="locale-switch" aria-label={copy.app.locale.label}>
            <button
              className={`locale-button ${locale === "zh-CN" ? "locale-button-active" : ""}`}
              onClick={() => setLocale("zh-CN")}
              type="button"
            >
              {copy.app.locale.chinese}
            </button>
            <button
              className={`locale-button ${locale === "en-US" ? "locale-button-active" : ""}`}
              onClick={() => setLocale("en-US")}
              type="button"
            >
              {copy.app.locale.english}
            </button>
          </div>

          <div className="hero-refresh">
            <div className="hero-refresh-row">
              <button
                className="ghost-button hero-refresh-button"
                disabled={isRefreshing}
                onClick={() => {
                  void refreshAppState("manual");
                }}
                type="button"
              >
                {isRefreshing ? copy.app.refreshing : copy.app.refreshNow}
              </button>
              <span className="hero-refresh-copy">{copy.app.autoRefresh}</span>
            </div>
            <p className={`hero-refresh-status ${hasRefreshError ? "hero-refresh-status-error" : ""}`}>
              {lastScannedLabel
                ? copy.app.lastScanned(lastScannedLabel)
                : copy.app.lastScannedPending}
              {hasRefreshError ? ` · ${copy.app.refreshError}` : ""}
            </p>
          </div>

          <div className="hero-stat">
            <span>{copy.app.observedToday}</span>
            <strong>{formatCompactNumber(snapshot.totalTokensToday, locale, 2)}</strong>
          </div>
          <div className="hero-stat">
            <span>{copy.app.visibleConnectors}</span>
            <strong>
              {snapshot.activeSources}/{snapshot.connectedSources}
            </strong>
          </div>
          <p className="hero-aside-note">
            {sourceLabel || copy.app.noActiveSource}
            {visibleSources.length > 3
              ? copy.app.moreSources(visibleSources.length - 3)
              : ""}
          </p>
        </div>
      </section>

      <section className="metric-grid">
        <MetricTile
          eyebrow={copy.app.metrics.today}
          value={formatFriendlyNumber(snapshot.totalTokensToday, locale, 2)}
          detail={copy.app.metrics.todayDetail(formatNumber(snapshot.totalTokensToday, locale))}
        />
        <MetricTile
          eyebrow={copy.app.metrics.sevenDayAverage}
          value={formatFriendlyNumber(lastSevenDayAverage, locale, 2)}
          detail={copy.app.metrics.sevenDayAverageDetail(formatNumber(lastSevenDayAverage, locale))}
        />
        <MetricTile
          eyebrow={copy.app.metrics.activeConnectors}
          value={`${snapshot.activeSources} / ${snapshot.connectedSources}`}
          detail={copy.app.metrics.activeConnectorsDetail(
            snapshot.activeSources,
            snapshot.connectedSources
          )}
        />
        <MetricTile
          eyebrow={copy.app.metrics.nativeCoverage}
          value={`${Math.round(snapshot.exactShare * 100)}%`}
          detail={copy.app.metrics.nativeCoverageDetail}
        />
      </section>

      <TodaySources locale={locale} sources={snapshot.sources} />
      <TrendExplorer history={snapshot.dailyHistory} locale={locale} />
      <ConnectorSessions groups={snapshot.sessionGroups} locale={locale} />
      <SourceStatusBoard
        locale={locale}
        onClearCherryBackupDir={async () => {
          await clearSavedCherryBackupDir();
          await refreshAppState("manual");
        }}
        onSaveCherryBackupDir={async (path) => {
          await saveCherryBackupDir(path);
          await refreshAppState("manual");
        }}
        settings={settings}
        statuses={snapshot.sourceStatuses}
      />
    </main>
  );
}
