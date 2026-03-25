import { invoke } from "@tauri-apps/api/core";
import { startTransition, useEffect, useEffectEvent, useRef, useState } from "react";
import { createEmptyDashboardSnapshot } from "./data/empty-dashboard";
import { resolveSelectedDateAfterRefresh, toLocalIsoDate } from "./date-utils.mjs";
import { showcaseCopy } from "./showcase-copy.mjs";
import type {
  DailyUsagePoint,
  DashboardSnapshot,
  SessionSummary,
  SourceDetailSnapshot,
  SourceUsage,
} from "./data/schema";
import {
  calculationLabel,
  detectInitialLocale,
  formatCompactNumber,
  formatFriendlyNumber,
  formatLocalizedDateTime,
  getCopy,
  getLocaleLabel,
  sourceStateLabel,
  supportedLocales,
  type Locale,
} from "./i18n";
import "./styles.css";

type AppRoute = { kind: "home" } | { kind: "source"; sourceId: string };

function formatUsd(value: number, locale: Locale) {
  return new Intl.NumberFormat(locale === "zh-CN" ? "en-US" : "en-US", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: value >= 1 ? 2 : 3,
    maximumFractionDigits: value >= 1 ? 2 : 3,
  }).format(value);
}

function readRoute(pathname: string): AppRoute {
  const match = pathname.match(/^\/sources\/([^/]+)\/?$/);
  if (!match) {
    return { kind: "home" };
  }

  try {
    return { kind: "source", sourceId: decodeURIComponent(match[1]) };
  } catch {
    return { kind: "source", sourceId: match[1] };
  }
}

function routeToPath(route: AppRoute) {
  if (route.kind === "home") {
    return "/";
  }

  return `/sources/${encodeURIComponent(route.sourceId)}`;
}

/* =========================================================
   Tauri / browser data layer
   ========================================================= */

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

async function getDashboardSnapshot() {
  if (window.__TAURI_INTERNALS__) {
    return invoke<DashboardSnapshot>("get_dashboard_snapshot");
  }

  const response = await fetch("/api/snapshot", {
    headers: { Accept: "application/json" },
  });
  if (!response.ok) {
    throw new Error(`Snapshot request failed with ${response.status}`);
  }

  return (await response.json()) as DashboardSnapshot;
}

function getSourceSnapshot(sourceId: string) {
  if (window.__TAURI_INTERNALS__) {
    return invoke<SourceDetailSnapshot>("get_source_snapshot", { sourceId });
  }

  return fetch(`/api/sources/${encodeURIComponent(sourceId)}`, {
    headers: { Accept: "application/json" },
  }).then(async (response) => {
    if (!response.ok) {
      throw new Error(`Source snapshot request failed with ${response.status}`);
    }

    return (await response.json()) as SourceDetailSnapshot;
  });
}

/* =========================================================
   Sub-components
   ========================================================= */

function SectionHeader({ label }: { label: string }) {
  return (
    <div className="sec-head">
      <span className="sec-label">{label}</span>
      <span className="sec-rule" />
    </div>
  );
}

function formatDayStamp(date: string, locale: Locale) {
  const d = new Date(`${date}T12:00:00`);
  const loc = locale === "zh-CN" ? "zh-CN" : "en-US";
  const weekday = d.toLocaleDateString(loc, { weekday: "short" });
  const monthDay = d.toLocaleDateString(loc, {
    month: "numeric",
    day: "numeric",
  });

  return `${weekday} ${monthDay}`;
}

function formatSignedPercent(value: number, locale: Locale) {
  return new Intl.NumberFormat(locale === "zh-CN" ? "zh-CN" : "en-US", {
    style: "percent",
    signDisplay: "always",
    maximumFractionDigits: 0,
  }).format(value);
}

function pickPeakDay(data: DailyUsagePoint[]) {
  return data.reduce((peak, day) => {
    if (day.totalTokens > peak.totalTokens) {
      return day;
    }

    if (day.totalTokens === peak.totalTokens && day.date > peak.date) {
      return day;
    }

    return peak;
  });
}

function formatTokenFigure(tokens: number, locale: Locale) {
  return formatFriendlyNumber(tokens, locale, 1);
}

function useRetainedDateSelection(data: DailyUsagePoint[]) {
  const latestDay = data[data.length - 1];
  const [selectedDate, setSelectedDate] = useState(latestDay.date);
  const previousLatestDateRef = useRef(latestDay.date);

  useEffect(() => {
    const availableDates = data.map((day) => day.date);
    setSelectedDate((current) =>
      resolveSelectedDateAfterRefresh({
        currentDate: current,
        previousLatestDate: previousLatestDateRef.current,
        nextLatestDate: latestDay.date,
        availableDates,
      }),
    );
    previousLatestDateRef.current = latestDay.date;
  }, [data, latestDay.date]);

  return { latestDay, selectedDate, setSelectedDate };
}

function TrendInspector({
  day,
  locale,
  estimatedCost,
  pricingPending,
}: {
  day: DailyUsagePoint;
  locale: Locale;
  estimatedCost: (cost: string) => string;
  pricingPending: string;
}) {
  const hasUsage = day.totalTokens > 0;
  const hasCost = day.totalCostUsd > 0;

  return (
    <div className="trend-inspector">
      <span className="trend-inspector-date">{formatDayStamp(day.date, locale)}</span>
      <strong className="trend-inspector-value">
        {formatTokenFigure(day.totalTokens, locale)}
      </strong>
      <span className={`trend-inspector-cost${hasUsage && !hasCost ? " pending" : ""}`}>
        {!hasUsage
          ? "—"
          : hasCost
            ? estimatedCost(formatUsd(day.totalCostUsd, locale))
            : pricingPending}
      </span>
    </div>
  );
}

function WeeklyDayFocus({
  day,
  locale,
  estimatedCost,
  pricingPending,
}: {
  day: DailyUsagePoint;
  locale: Locale;
  estimatedCost: (cost: string) => string;
  pricingPending: string;
}) {
  const hasUsage = day.totalTokens > 0;
  const hasCost = day.totalCostUsd > 0;

  return (
    <div className="weekly-focus">
      <strong className="weekly-focus-value">
        {hasUsage ? formatTokenFigure(day.totalTokens, locale) : "—"}
      </strong>
      <div className="weekly-focus-meta">
        <span className="weekly-focus-date">{formatDayStamp(day.date, locale)}</span>
        {hasUsage && (
          <>
            <span className="weekly-focus-sep" aria-hidden="true">
              ·
            </span>
            <span className={`weekly-focus-cost${hasCost ? "" : " pending"}`}>
              {hasCost ? estimatedCost(formatUsd(day.totalCostUsd, locale)) : pricingPending}
            </span>
          </>
        )}
      </div>
    </div>
  );
}

function SparklineGraphic({
  data,
  averageTokens,
  locale,
  activeDate,
  onSelectDate,
}: {
  data: DailyUsagePoint[];
  averageTokens: number;
  locale: Locale;
  activeDate: string;
  onSelectDate: (date: string) => void;
}) {
  if (data.length < 2) return null;

  const max = Math.max(...data.map((d) => d.totalTokens), 1);
  const vw = 300;
  const vh = 78;
  const pad = 2;

  const pts = data.map((d, i) => {
    const x = pad + (i / (data.length - 1)) * (vw - pad * 2);
    const y = vh - pad - (d.totalTokens / max) * (vh - pad * 2);
    return {
      day: d,
      x,
      y,
      xPct: (x / vw) * 100,
      yPct: (y / vh) * 100,
    };
  });

  const line = pts.map(({ x, y }) => `${x},${y}`).join(" ");
  const area = `${pad},${vh} ${line} ${vw - pad},${vh}`;
  const avgY = vh - pad - (averageTokens / max) * (vh - pad * 2);
  const activePoint = pts.find((point) => point.day.date === activeDate) ?? pts[pts.length - 1];

  return (
    <div className="spark-frame">
      <svg
        viewBox={`0 0 ${vw} ${vh}`}
        preserveAspectRatio="none"
        className="spark-svg"
        aria-label={`sparkline-${locale}`}
      >
        <defs>
          <linearGradient id="sf" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="rgba(255,107,44,0.28)" />
            <stop offset="100%" stopColor="rgba(255,107,44,0)" />
          </linearGradient>
        </defs>
        <polygon points={area} fill="url(#sf)" />
        <polyline
          points={line}
          fill="none"
          stroke="var(--ember)"
          strokeWidth="2"
          strokeLinejoin="round"
          vectorEffect="non-scaling-stroke"
        />
        {averageTokens > 0 && (
          <line
            x1={pad}
            y1={avgY}
            x2={vw - pad}
            y2={avgY}
            stroke="var(--flame)"
            strokeWidth="1"
            strokeDasharray="4,3"
            opacity="0.45"
            vectorEffect="non-scaling-stroke"
          />
        )}
        <circle
          cx={activePoint.x}
          cy={activePoint.y}
          r="3.8"
          fill="var(--flame)"
          stroke="rgba(15, 9, 7, 0.92)"
          strokeWidth="1.6"
        />
      </svg>
      {pts.map((point) => {
        const isActive = point.day.date === activeDate;
        return (
          <button
            key={point.day.date}
            type="button"
            className={`spark-point-button${isActive ? " active" : ""}`}
            style={{
              left: `${point.xPct}%`,
              top: `${point.yPct}%`,
            }}
            onMouseEnter={() => onSelectDate(point.day.date)}
            onFocus={() => onSelectDate(point.day.date)}
            onClick={() => onSelectDate(point.day.date)}
            aria-pressed={isActive}
            aria-label={`${formatDayStamp(point.day.date, locale)} ${formatTokenFigure(point.day.totalTokens, locale)}`}
          >
            <span className="spark-point-dot" />
          </button>
        );
      })}
    </div>
  );
}

function FlameChart({
  data,
  locale,
  activeDate,
  onSelectDate,
}: {
  data: DailyUsagePoint[];
  locale: Locale;
  activeDate: string;
  onSelectDate: (date: string) => void;
}) {
  const maxTokens = Math.max(...data.map((d) => d.totalTokens), 1);
  const todayStr = toLocalIsoDate();
  const loc = locale === "zh-CN" ? "zh-CN" : "en-US";

  return (
    <div className="flame-chart">
      {data.map((day, i) => {
        const pct = (day.totalTokens / maxTokens) * 100;
        const isToday = day.date === todayStr;
        const isActive = day.date === activeDate;
        const d = new Date(`${day.date}T12:00:00`);
        const dayLabel = d.toLocaleDateString(loc, { weekday: "short" });
        const dateLabel = d.toLocaleDateString(loc, {
          month: "numeric",
          day: "numeric",
        });
        return (
          <button
            key={day.date}
            type="button"
            className={`flame-hitbox${isActive ? " active" : ""}`}
            style={{ animationDelay: `${i * 55}ms` }}
            onMouseEnter={() => onSelectDate(day.date)}
            onFocus={() => onSelectDate(day.date)}
            onClick={() => onSelectDate(day.date)}
            aria-pressed={isActive}
            aria-label={`${formatDayStamp(day.date, locale)} ${formatTokenFigure(day.totalTokens, locale)}`}
          >
            <span className={`flame-val${isActive ? " active" : ""}`}>
              {day.totalTokens > 0 ? formatCompactNumber(day.totalTokens, locale, 1) : "–"}
            </span>
            <div className={`flame-track${isActive ? " active" : ""}`}>
              <div
                className={`flame-bar${isToday ? " today" : ""}${isActive ? " active" : ""}`}
                style={{ height: `${Math.max(pct, 4)}%` }}
              />
            </div>
            <span className={`flame-day${isToday ? " today" : ""}${isActive ? " active" : ""}`}>
              {dayLabel}
            </span>
            <span className={`flame-date${isToday ? " today" : ""}${isActive ? " active" : ""}`}>
              {dateLabel}
            </span>
          </button>
        );
      })}
    </div>
  );
}

function WeeklyBurnCard({
  data,
  locale,
  label,
  title,
  totalLabel,
  avgDayLabel,
  estimatedCost,
  pricingPending,
}: {
  data: DailyUsagePoint[];
  locale: Locale;
  label: string;
  title: string;
  totalLabel: string;
  avgDayLabel: string;
  estimatedCost: (cost: string) => string;
  pricingPending: string;
}) {
  if (data.length === 0) {
    return null;
  }

  const total7 = data.reduce((sum, day) => sum + day.totalTokens, 0);
  const avg7 = data.length === 0 ? 0 : Math.round(total7 / data.length);
  const { selectedDate, setSelectedDate } = useRetainedDateSelection(data);

  const activeDay = data.find((day) => day.date === selectedDate) ?? data[data.length - 1];

  return (
    <section className="trend-section weekly-trend-section">
      <article className="weekly-burn-card">
        <div className="weekly-burn-head">
          <div className="trend-copy weekly-trend-copy">
            <p className="trend-kicker">{label}</p>
            <h2 className="trend-title">{title}</h2>
            <WeeklyDayFocus
              day={activeDay}
              locale={locale}
              estimatedCost={estimatedCost}
              pricingPending={pricingPending}
            />
          </div>
          <div className="trend-stat-grid weekly-stat-grid">
            <div className="trend-stat">
              <span className="trend-stat-label">{totalLabel}</span>
              <strong className="trend-stat-value">
                {formatFriendlyNumber(total7, locale, 1)}
              </strong>
            </div>
            <div className="trend-stat">
              <span className="trend-stat-label">{avgDayLabel}</span>
              <strong className="trend-stat-value">
                {formatFriendlyNumber(avg7, locale, 1)}
              </strong>
            </div>
          </div>
        </div>
        <div className="weekly-chart-shell">
          <FlameChart
            data={data}
            locale={locale}
            activeDate={activeDay.date}
            onSelectDate={setSelectedDate}
          />
        </div>
      </article>
    </section>
  );
}

function MonthlyTrendCard({
  history,
  week,
  locale,
  label,
  monthContextLabel,
  monthTotalLabel,
  monthPeakLabel,
  monthDeltaText,
  monthFlatText,
  avgDayLabel,
  estimatedCost,
  pricingPending,
}: {
  history: DailyUsagePoint[];
  week: DailyUsagePoint[];
  locale: Locale;
  label: string;
  monthContextLabel: string;
  monthTotalLabel: string;
  monthPeakLabel: string;
  monthDeltaText: (delta: string) => string;
  monthFlatText: string;
  avgDayLabel: string;
  estimatedCost: (cost: string) => string;
  pricingPending: string;
}) {
  const data = history.slice(-30);
  if (data.length < 2) {
    return null;
  }

  const total30 = data.reduce((sum, day) => sum + day.totalTokens, 0);
  const avg30 = data.length === 0 ? 0 : Math.round(total30 / data.length);
  const avg7 =
    week.length === 0
      ? 0
      : Math.round(week.reduce((sum, day) => sum + day.totalTokens, 0) / week.length);
  const delta = avg30 > 0 ? (avg7 - avg30) / avg30 : null;
  const peakDay = pickPeakDay(data);
  const { selectedDate, setSelectedDate } = useRetainedDateSelection(data);

  const headline =
    delta != null && Math.abs(delta) >= 0.005
      ? monthDeltaText(formatSignedPercent(delta, locale))
      : monthFlatText;
  const activeDay = data.find((day) => day.date === selectedDate) ?? data[data.length - 1];

  return (
    <section className="trend-section monthly-trend-section">
      <article className="monthly-trend-card">
        <div className="trend-copy monthly-trend-copy">
          <p className="trend-kicker">{monthContextLabel}</p>
          <h2 className="trend-title">{headline}</h2>
          <div className="trend-stat-grid monthly-stat-grid">
            <div className="trend-stat compact">
              <span className="trend-stat-label">{monthTotalLabel}</span>
              <strong className="trend-stat-value">
                {formatFriendlyNumber(total30, locale, 1)}
              </strong>
            </div>
            <div className="trend-stat compact">
              <span className="trend-stat-label">{avgDayLabel}</span>
              <strong className="trend-stat-value">
                {formatFriendlyNumber(avg30, locale, 1)}
              </strong>
            </div>
            <div className="trend-stat compact">
              <span className="trend-stat-label">{monthPeakLabel}</span>
              <strong className="trend-stat-value">
                {formatDayStamp(peakDay.date, locale)}
              </strong>
            </div>
          </div>
        </div>
        <div className="monthly-spark-shell">
          <div className="monthly-spark-head">
            <span className="spark-label">{label}</span>
            <span className="spark-avg">
              {formatFriendlyNumber(avg30, locale, 1)} {avgDayLabel}
            </span>
          </div>
          <TrendInspector
            day={activeDay}
            locale={locale}
            estimatedCost={estimatedCost}
            pricingPending={pricingPending}
          />
          <SparklineGraphic
            data={data}
            averageTokens={avg30}
            locale={locale}
            activeDate={activeDay.date}
            onSelectDate={setSelectedDate}
          />
        </div>
      </article>
    </section>
  );
}

function SourceList({
  sources,
  locale,
  estimatedCost,
  pricingPending,
  onOpenSource,
}: {
  sources: SourceUsage[];
  locale: Locale;
  estimatedCost: (cost: string) => string;
  pricingPending: string;
  onOpenSource: (sourceId: string) => void;
}) {
  const maxTokens = Math.max(...sources.map((s) => s.tokens ?? 0), 1);

  return (
    <div className="source-list">
      {sources.map((s, i) => {
        const tokenValue = s.tokens ?? 0;
        const pct = s.analyticsState === "ready" ? (tokenValue / maxTokens) * 100 : 0;
        const icon =
          s.trend === "up" ? "↑" : s.trend === "down" ? "↓" : s.trend === "flat" ? "→" : null;
        const statusCopy =
          s.analyticsState === "session_only" ? "analytics pending" : "data unavailable";
        const costLabel =
          s.analyticsState !== "ready"
            ? statusCopy
            : s.costUsd != null
              ? estimatedCost(formatUsd(s.costUsd, locale))
              : pricingPending;
        return (
          <button
            key={s.sourceId}
            type="button"
            className="source-row"
            style={{ animationDelay: `${i * 50}ms` }}
            onClick={() => onOpenSource(s.sourceId)}
            aria-label={
              s.analyticsState === "ready"
                ? `${s.source} ${formatTokenFigure(tokenValue, locale)}`
                : `${s.source} ${statusCopy}`
            }
          >
            <div className="source-main">
              <span className="source-name">{s.source}</span>
              <span className={`source-cost${s.analyticsState === "ready" && s.costUsd != null ? "" : " pending"}`}>
                {costLabel}
              </span>
            </div>
            <div className="source-bar-bg">
              <div
                className="source-bar-fill"
                style={{
                  width: `${s.analyticsState === "ready" ? Math.max(pct, 3) : 0}%`,
                  animationDelay: `${i * 70}ms`,
                }}
              />
            </div>
            <span className="source-tokens">
              {s.analyticsState === "ready"
                ? formatCompactNumber(tokenValue, locale, 1)
                : "—"}
            </span>
            {icon ? <span className={`source-trend ${s.trend}`}>{icon}</span> : null}
          </button>
        );
      })}
    </div>
  );
}

function SessionFeed({
  sessions,
  locale,
  estimatedCost,
  pricingPending,
  limit = 6,
}: {
  sessions: SessionSummary[];
  locale: Locale;
  estimatedCost: (cost: string) => string;
  pricingPending: string;
  limit?: number;
}) {
  return (
    <div className="sess-feed">
      {sessions.slice(0, limit).map((s) => (
        <div key={`${s.sourceId}:${s.id}`} className="sess-item">
          <div className="sess-top">
            <span className="sess-title">{s.title || "Untitled"}</span>
            <span className="sess-source">{s.source}</span>
          </div>
          <div className="sess-meta">
            <span>{s.model}</span>
            <span>{formatCompactNumber(s.totalTokens, locale, 1)} tokens</span>
            <span className={`sess-cost${s.costUsd > 0 ? "" : " pending"}`}>
              {s.costUsd > 0
                ? estimatedCost(formatUsd(s.costUsd, locale))
                : pricingPending}
            </span>
          </div>
        </div>
      ))}
    </div>
  );
}

function analyticsStateText(sc: typeof showcaseCopy["en-US"], state: SourceDetailSnapshot["analyticsState"]) {
  if (state === "ready") return sc.analyticsReady;
  if (state === "session_only") return sc.analyticsPending;
  return sc.analyticsUnavailable;
}

function pricingCoverageText(
  copy: ReturnType<typeof getCopy>,
  coverage: "actual" | "partial" | "pending" | null,
) {
  if (coverage === "actual") return copy.common.ready;
  if (coverage === "partial") return copy.common.partial;
  if (coverage === "pending") return copy.common.pending;
  return copy.common.unknown;
}

function SummaryStrip({
  locale,
  sc,
  estimatedCost,
  pricingPending,
  summaries,
}: {
  locale: Locale;
  sc: typeof showcaseCopy["en-US"];
  estimatedCost: (cost: string) => string;
  pricingPending: string;
  summaries: Array<{
    label: string;
    summary: SourceDetailSnapshot["todaySummary"];
  }>;
}) {
  return (
    <section className="source-summary-strip">
      {summaries.map(({ label, summary }) => (
        <article className="source-summary-card" key={label}>
          <span className="source-summary-label">{label}</span>
          <strong className="source-summary-value">
            {summary ? formatFriendlyNumber(summary.tokens, locale, 1) : "—"}
          </strong>
          <span className={`source-summary-cost${summary?.costUsd != null ? "" : " pending"}`}>
            {summary == null
              ? "—"
              : summary.costUsd != null
                ? estimatedCost(formatUsd(summary.costUsd, locale))
                : pricingPending}
          </span>
          <div className="source-summary-meta">
            <span>{sc.summarySessions}: {summary?.sessions ?? 0}</span>
            <span>{sc.summaryActiveDays}: {summary?.activeDays ?? 0}</span>
          </div>
        </article>
      ))}
    </section>
  );
}

function PeriodicBreakdownTables({
  locale,
  sc,
  estimatedCost,
  pricingPending,
  snapshot,
}: {
  locale: Locale;
  sc: typeof showcaseCopy["en-US"];
  estimatedCost: (cost: string) => string;
  pricingPending: string;
  snapshot: SourceDetailSnapshot;
}) {
  const breakdowns = snapshot.periodicBreakdowns;
  if (!breakdowns) {
    return null;
  }

  const renderTable = (
    title: string,
    rows: NonNullable<SourceDetailSnapshot["periodicBreakdowns"]>["weekly"],
  ) => (
    <section className="periodic-breakdown">
      <SectionHeader label={title} />
      <div className="periodic-breakdown-table">
        {rows.map((row) => (
          <div className="periodic-breakdown-row" key={`${title}:${row.startDate}`}>
            <span>{row.label}</span>
            <span>{formatFriendlyNumber(row.tokens, locale)}</span>
            <span className={row.costUsd != null ? "" : "pending"}>
              {row.costUsd != null
                ? estimatedCost(formatUsd(row.costUsd, locale))
                : pricingPending}
            </span>
          </div>
        ))}
      </div>
    </section>
  );

  return (
    <>
      {renderTable(sc.breakdownWeekly, breakdowns.weekly)}
      {renderTable(sc.breakdownMonthly, breakdowns.monthly)}
    </>
  );
}

function SourceDetailPage({
  snapshot,
  locale,
  sc,
  copy,
  estimatedCost,
  pricingPending,
  emptyMessage,
  onNavigateHome,
}: {
  snapshot: SourceDetailSnapshot | null;
  locale: Locale;
  sc: typeof showcaseCopy["en-US"];
  copy: ReturnType<typeof getCopy>;
  estimatedCost: (cost: string) => string;
  pricingPending: string;
  emptyMessage: string;
  onNavigateHome: () => void;
}) {
  if (!snapshot) {
    return (
      <section className="source-detail-page">
        <div className="source-detail-header">
          <button type="button" className="back-link" onClick={onNavigateHome}>
            {sc.backToOverview}
          </button>
          <div className="source-detail-empty">
            <p className="trend-kicker">{sc.sourceHistory}</p>
            <p className="empty-state">{emptyMessage}</p>
          </div>
        </div>
      </section>
    );
  }

  const lastSeen =
    formatLocalizedDateTime(snapshot.status.lastSeenAt ?? undefined, locale) ??
    snapshot.status.lastSeenAt;
  const analyticsState = analyticsStateText(sc, snapshot.analyticsState);
  const pricingCoverage =
    snapshot.todaySummary?.pricingCoverage ?? snapshot.last7dSummary?.pricingCoverage ?? null;
  const sourceSummary =
    snapshot.status.note ||
    (lastSeen
      ? `${copy.connectors.lastSeen} ${lastSeen}`
      : `${sc.pricingCoverage}: ${calculationLabel(locale, snapshot.calculationMix)}`);

  return (
    <section className="source-detail-page">
      <div className="source-detail-header">
        <button type="button" className="back-link" onClick={onNavigateHome}>
          {sc.backToOverview}
        </button>
        <div className="source-detail-head">
          <div className="source-detail-copy">
            <p className="trend-kicker">{sc.sourceHistory}</p>
            <h1 className="source-detail-title">{snapshot.sourceName}</h1>
            <p className="source-detail-summary">{sourceSummary}</p>
          </div>
          <div className="source-detail-meta">
            <div className="detail-chip">
              <span className="detail-chip-label">{sc.sourceState}</span>
              <strong className="detail-chip-value">
                {sourceStateLabel(locale, snapshot.status.state)}
              </strong>
            </div>
            <div className="detail-chip">
              <span className="detail-chip-label">{sc.analyticsState}</span>
              <strong className="detail-chip-value">
                {analyticsState}
              </strong>
            </div>
            <div className="detail-chip">
              <span className="detail-chip-label">{sc.pricingCoverage}</span>
              <strong className="detail-chip-value">
                {pricingCoverageText(copy, pricingCoverage)}
              </strong>
            </div>
          </div>
        </div>
        {snapshot.status.capabilities.length > 0 && (
          <div className="source-cap-list">
            {snapshot.status.capabilities.map((capability) => (
              <span key={capability} className="source-cap-pill">
                {capability}
              </span>
            ))}
          </div>
        )}
      </div>

      {snapshot.analyticsState === "ready" ? (
        <>
          <SummaryStrip
            locale={locale}
            sc={sc}
            estimatedCost={estimatedCost}
            pricingPending={pricingPending}
            summaries={[
              { label: sc.summaryToday, summary: snapshot.todaySummary },
              { label: sc.summaryLast7d, summary: snapshot.last7dSummary },
              { label: sc.summaryLast30d, summary: snapshot.last30dSummary },
              { label: sc.summaryLifetime, summary: snapshot.lifetimeSummary },
            ]}
          />

          <WeeklyBurnCard
            data={snapshot.week}
            locale={locale}
            label={sc.last7Days}
            title={sc.weekFocusTitle}
            totalLabel={sc.weekTotal}
            avgDayLabel={sc.avgDay}
            estimatedCost={estimatedCost}
            pricingPending={pricingPending}
          />

          <MonthlyTrendCard
            history={snapshot.dailyHistory}
            week={snapshot.week}
            locale={locale}
            label={sc.trend30d}
            monthContextLabel={sc.monthContext}
            monthTotalLabel={sc.monthTotal}
            monthPeakLabel={sc.monthPeak}
            monthDeltaText={sc.monthDelta}
            monthFlatText={sc.monthFlat}
            avgDayLabel={sc.avgDay}
            estimatedCost={estimatedCost}
            pricingPending={pricingPending}
          />

          <PeriodicBreakdownTables
            locale={locale}
            sc={sc}
            estimatedCost={estimatedCost}
            pricingPending={pricingPending}
            snapshot={snapshot}
          />
        </>
      ) : (
        <section className="source-analytics-callout">
          <SectionHeader label={analyticsState} />
          <p className="empty-state">
            {snapshot.analyticsState === "session_only"
              ? sc.analyticsPendingMessage
              : sc.analyticsUnavailableMessage}
          </p>
        </section>
      )}

      <section className="sess-section">
        <SectionHeader label={sc.recentSessions} />
        {snapshot.sessions.length > 0 ? (
          <SessionFeed
            sessions={snapshot.sessions}
            locale={locale}
            estimatedCost={estimatedCost}
            pricingPending={pricingPending}
            limit={12}
          />
        ) : (
          <p className="empty-state">{sc.noSourceSessions}</p>
        )}
      </section>
    </section>
  );
}

/* =========================================================
   Main App
   ========================================================= */

export default function App() {
  const [route, setRoute] = useState<AppRoute>(() => readRoute(window.location.pathname));
  const [snapshot, setSnapshot] = useState<DashboardSnapshot>(() =>
    createEmptyDashboardSnapshot(),
  );
  const [sourceSnapshot, setSourceSnapshot] = useState<SourceDetailSnapshot | null>(null);
  const [locale, setLocale] = useState<Locale>(() => detectInitialLocale());
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [lastRefreshedAt, setLastRefreshedAt] = useState<string | null>(null);
  const [hasRefreshError, setHasRefreshError] = useState(false);
  const refreshInFlightRef = useRef(false);

  const refreshAppState = useEffectEvent(
    async (mode: "initial" | "manual" | "poll" | "focus" = "manual") => {
      if (refreshInFlightRef.current) return;
      refreshInFlightRef.current = true;
      if (mode !== "poll") setIsRefreshing(true);
      setHasRefreshError(false);

      try {
        if (route.kind === "source") {
          const nextSourceSnapshot = await getSourceSnapshot(route.sourceId);
          startTransition(() => {
            setSourceSnapshot(nextSourceSnapshot);
            setLastRefreshedAt(new Date().toISOString());
          });
        } else {
          const nextSnapshot = await getDashboardSnapshot();
          startTransition(() => {
            setSnapshot(nextSnapshot);
            setLastRefreshedAt(new Date().toISOString());
          });
        }
      } catch {
        setHasRefreshError(true);
        if (route.kind === "source") {
          startTransition(() => setSourceSnapshot(null));
        }
      } finally {
        refreshInFlightRef.current = false;
        setIsRefreshing(false);
      }
    },
  );

  const navigateToRoute = useEffectEvent((nextRoute: AppRoute) => {
    const nextPath = routeToPath(nextRoute);
    if (window.location.pathname !== nextPath) {
      window.history.pushState({}, "", nextPath);
    }

    startTransition(() => {
      setRoute(nextRoute);
      if (nextRoute.kind === "home") {
        setSourceSnapshot(null);
      } else {
        setSourceSnapshot((current) =>
          current?.sourceId === nextRoute.sourceId ? current : null,
        );
      }
    });

    window.scrollTo({ top: 0, behavior: "smooth" });
  });

  useEffect(() => {
    const onPopState = () => {
      startTransition(() => {
        setRoute(readRoute(window.location.pathname));
      });
    };

    window.addEventListener("popstate", onPopState);
    return () => window.removeEventListener("popstate", onPopState);
  }, []);

  useEffect(() => {
    if (route.kind === "source") {
      setSourceSnapshot((current) =>
        current?.sourceId === route.sourceId ? current : null,
      );
    }

    void refreshAppState("initial");
  }, [route.kind, route.kind === "source" ? route.sourceId : "home"]);

  useEffect(() => {
    const id = window.setInterval(() => {
      if (document.visibilityState === "visible") {
        void refreshAppState("poll");
      }
    }, 60_000);
    return () => window.clearInterval(id);
  }, []);

  useEffect(() => {
    const onFocus = () => void refreshAppState("focus");
    const onVisible = () => {
      if (document.visibilityState === "visible") {
        void refreshAppState("focus");
      }
    };
    window.addEventListener("focus", onFocus);
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      window.removeEventListener("focus", onFocus);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, []);

  useEffect(() => {
    window.localStorage.setItem("burned.locale", locale);
  }, [locale]);

  const sc = showcaseCopy[locale];
  const copy = getCopy(locale);
  const week = snapshot.week.length > 0 ? snapshot.week : snapshot.dailyHistory.slice(-7);
  const scanLabel =
    formatLocalizedDateTime(lastRefreshedAt ?? undefined, locale) ?? lastRefreshedAt;

  return (
    <div className="burned-app">
      <div className="bg-fx">
        <div className="bg-glow-a" />
        <div className="bg-glow-b" />
        <div className="bg-grid" />
      </div>

      <header className="topbar">
        <button
          className="brand brand-button"
          onClick={() => navigateToRoute({ kind: "home" })}
          type="button"
        >
          Burned
        </button>
        <div className="topbar-right">
          <label className="locale-sw" aria-label={copy.app.locale.label}>
            <select
              className="locale-select"
              onChange={(event) => setLocale(event.target.value as Locale)}
              value={locale}
            >
              {supportedLocales.map((localeCode) => (
                <option key={localeCode} value={localeCode}>
                  {getLocaleLabel(localeCode)}
                </option>
              ))}
            </select>
          </label>
          <button
            className="refresh-btn"
            disabled={isRefreshing}
            onClick={() => void refreshAppState("manual")}
            type="button"
          >
            {isRefreshing ? sc.refreshing : sc.refresh}
          </button>
        </div>
      </header>

      {route.kind === "home" ? (
        <>
          <section className="burn-hero">
            <p className="hero-tagline">{sc.tagline}</p>
            <hr className="hero-sep" />
            <span className="burn-number">
              {snapshot.totalTokensToday > 0
                ? formatCompactNumber(snapshot.totalTokensToday, locale, 1)
                : "—"}
            </span>
            {snapshot.totalTokensToday > 0 && (
              <p className={`burn-cost${snapshot.totalCostToday > 0 ? "" : " pending"}`}>
                {snapshot.totalCostToday > 0
                  ? sc.todayCost(formatUsd(snapshot.totalCostToday, locale))
                  : sc.pricingPending}
              </p>
            )}
            {snapshot.burnRatePerHour > 0 && (
              <div className="burn-rate-pill">
                <span className="burn-rate-fire">🔥</span>
                {formatCompactNumber(snapshot.burnRatePerHour, locale, 1)} {sc.perHour}
              </div>
            )}
          </section>

          {(scanLabel || hasRefreshError) && (
            <p className={`scan-line${hasRefreshError ? " has-error" : ""}`}>
              {scanLabel ? sc.lastScan(scanLabel) : sc.waitingScan}
              {hasRefreshError ? ` · ${sc.refreshFailed}` : ""}
            </p>
          )}

          <WeeklyBurnCard
            data={week}
            locale={locale}
            label={sc.last7Days}
            title={sc.weekFocusTitle}
            totalLabel={sc.weekTotal}
            avgDayLabel={sc.avgDay}
            estimatedCost={sc.estimatedCost}
            pricingPending={sc.pricingPending}
          />

          <MonthlyTrendCard
            history={snapshot.dailyHistory}
            week={week}
            locale={locale}
            label={sc.trend30d}
            monthContextLabel={sc.monthContext}
            monthTotalLabel={sc.monthTotal}
            monthPeakLabel={sc.monthPeak}
            monthDeltaText={sc.monthDelta}
            monthFlatText={sc.monthFlat}
            avgDayLabel={sc.avgDay}
            estimatedCost={sc.estimatedCost}
            pricingPending={sc.pricingPending}
          />

          {snapshot.sources.length > 0 && (
            <section className="source-section">
              <SectionHeader label={sc.whereItBurns} />
              <SourceList
                sources={snapshot.sources}
                locale={locale}
                estimatedCost={sc.estimatedCost}
                pricingPending={sc.pricingPending}
                onOpenSource={(sourceId) => navigateToRoute({ kind: "source", sourceId })}
              />
            </section>
          )}

          {snapshot.sessions.length > 0 && (
            <section className="sess-section">
              <SectionHeader label={sc.recentSessions} />
              <SessionFeed
                sessions={snapshot.sessions}
                locale={locale}
                estimatedCost={sc.estimatedCost}
                pricingPending={sc.pricingPending}
              />
            </section>
          )}
        </>
      ) : (
        <>
          {(scanLabel || hasRefreshError) && (
            <p className={`scan-line${hasRefreshError ? " has-error" : ""}`}>
              {scanLabel ? sc.lastScan(scanLabel) : sc.waitingScan}
              {hasRefreshError ? ` · ${sc.refreshFailed}` : ""}
            </p>
          )}

          <SourceDetailPage
            snapshot={sourceSnapshot}
            locale={locale}
            sc={sc}
            copy={copy}
            estimatedCost={sc.estimatedCost}
            pricingPending={sc.pricingPending}
            emptyMessage={hasRefreshError ? sc.sourceUnavailable : sc.noData}
            onNavigateHome={() => navigateToRoute({ kind: "home" })}
          />
        </>
      )}

      <footer className="burned-footer">
        <span>burned</span> · desktop AI burn tracker
      </footer>
    </div>
  );
}
