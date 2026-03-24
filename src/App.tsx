import { invoke } from "@tauri-apps/api/core";
import { startTransition, useEffect, useEffectEvent, useRef, useState } from "react";
import { createEmptyDashboardSnapshot } from "./data/empty-dashboard";
import { toLocalIsoDate } from "./date-utils.mjs";
import { appCopy } from "./app-copy.mjs";
import type {
  BillingState,
  DailyUsagePoint,
  DashboardSnapshot,
  PeriodicBreakdownRow,
  PricingCoverage,
  SessionSummary,
  SourceDetailSnapshot,
  SourceStatus,
  SourceUsage,
  UsageWindowSummary,
} from "./data/schema";
import {
  calculationLabel,
  detectInitialLocale,
  formatCompactNumber,
  formatFriendlyNumber,
  formatLocalizedDateTime,
  getCopy,
  sourceStateLabel,
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

function pricingCoverageLabel(locale: Locale, coverage: PricingCoverage) {
  if (locale === "zh-CN") {
    switch (coverage) {
      case "actual":
        return "已计价";
      case "partial":
        return "部分计价";
      default:
        return "待补价";
    }
  }

  switch (coverage) {
    case "actual":
      return "Actual";
    case "partial":
      return "Partial";
    default:
      return "Pending";
  }
}

function billingStateLabel(locale: Locale, state: BillingState["state"]) {
  if (locale === "zh-CN") {
    switch (state) {
      case "ready":
        return "可用";
      case "partial":
        return "部分可用";
      default:
        return "不可用";
    }
  }

  switch (state) {
    case "ready":
      return "Ready";
    case "partial":
      return "Partial";
    default:
      return "Unavailable";
  }
}

function formatCoverageCost(
  costUsd: number,
  coverage: PricingCoverage,
  locale: Locale,
  estimatedCost: (cost: string) => string,
  pricingPending: string,
) {
  if (costUsd <= 0) {
    return pricingPending;
  }

  const formatted = formatUsd(costUsd, locale);
  return coverage === "actual" ? formatted : estimatedCost(formatted);
}

function formatSignedTokenDelta(value: number, locale: Locale) {
  const abs = Math.abs(value);
  const sign = value > 0 ? "+" : value < 0 ? "-" : "";
  return `${sign}${formatCompactNumber(abs, locale, 1)}`;
}

function formatShare(value: number, locale: Locale) {
  return new Intl.NumberFormat(locale === "zh-CN" ? "zh-CN" : "en-US", {
    style: "percent",
    maximumFractionDigits: 0,
  }).format(value);
}

function formatPeriodDate(date: string, locale: Locale) {
  return new Date(`${date}T12:00:00`).toLocaleDateString(
    locale === "zh-CN" ? "zh-CN" : "en-US",
    {
      month: "short",
      day: "numeric",
    },
  );
}

function formatBillingUsage(billingState: BillingState, locale: Locale, fallback: string) {
  const formatValue = (value: number | null) =>
    value == null ? null : formatFriendlyNumber(value, locale, 1);
  const current = formatValue(billingState.current);
  const limit = formatValue(billingState.limit);
  const unit = billingState.unit ? ` ${billingState.unit}` : "";

  if (current && limit) {
    return `${current} / ${limit}${unit}`;
  }

  if (current) {
    return `${current}${unit}`;
  }

  if (limit) {
    return `${limit}${unit}`;
  }

  return fallback;
}

function sortSessions(
  sessions: SessionSummary[],
  sort: "recent" | "tokens" | "cost",
) {
  if (sort === "recent") {
    return sessions;
  }

  const sorted = [...sessions];
  if (sort === "tokens") {
    sorted.sort(
      (left, right) =>
        right.totalTokens - left.totalTokens ||
        right.costUsd - left.costUsd ||
        left.title.localeCompare(right.title),
    );
    return sorted;
  }

  sorted.sort(
    (left, right) =>
      right.costUsd - left.costUsd ||
      right.totalTokens - left.totalTokens ||
      left.title.localeCompare(right.title),
  );
  return sorted;
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
            ? formatCoverageCost(
                day.totalCostUsd,
                day.pricingCoverage,
                locale,
                estimatedCost,
                pricingPending,
              )
            : pricingPending}
      </span>
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
  totalLabel,
  avgDayLabel,
  estimatedCost,
  pricingPending,
}: {
  data: DailyUsagePoint[];
  locale: Locale;
  label: string;
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
  const peakDay = pickPeakDay(data);
  const [selectedDate, setSelectedDate] = useState(peakDay.date);

  useEffect(() => {
    setSelectedDate((current) =>
      data.some((day) => day.date === current) ? current : peakDay.date,
    );
  }, [data, peakDay.date]);

  const activeDay = data.find((day) => day.date === selectedDate) ?? peakDay;

  return (
    <section className="trend-section weekly-trend-section">
      <article className="weekly-burn-card">
        <div className="weekly-burn-head">
          <div className="trend-copy">
            <p className="trend-kicker">{label}</p>
            <h2 className="trend-title">{formatDayStamp(activeDay.date, locale)}</h2>
            <TrendInspector
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
  const latestDay = data[data.length - 1];
  const [selectedDate, setSelectedDate] = useState(latestDay.date);

  useEffect(() => {
    setSelectedDate((current) =>
      data.some((day) => day.date === current) ? current : latestDay.date,
    );
  }, [data, latestDay.date]);

  const headline =
    delta != null && Math.abs(delta) >= 0.005
      ? monthDeltaText(formatSignedPercent(delta, locale))
      : monthFlatText;
  const activeDay = data.find((day) => day.date === selectedDate) ?? latestDay;

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
  const maxTokens = Math.max(...sources.map((s) => s.tokens), 1);

  return (
    <div className="source-list">
      {sources.map((s, i) => {
        const pct = (s.tokens / maxTokens) * 100;
        const icon = s.trend === "up" ? "↑" : s.trend === "down" ? "↓" : "→";
        return (
          <button
            key={s.sourceId}
            type="button"
            className="source-row"
            style={{ animationDelay: `${i * 50}ms` }}
            onClick={() => onOpenSource(s.sourceId)}
            aria-label={`${s.source} ${formatTokenFigure(s.tokens, locale)}`}
          >
            <div className="source-main">
              <span className="source-name">{s.source}</span>
              <span className={`source-cost${s.costUsd > 0 ? "" : " pending"}`}>
                {s.costUsd > 0
                  ? estimatedCost(formatUsd(s.costUsd, locale))
                  : pricingPending}
              </span>
            </div>
            <div className="source-bar-bg">
              <div
                className="source-bar-fill"
                style={{
                  width: `${Math.max(pct, 3)}%`,
                  animationDelay: `${i * 70}ms`,
                }}
              />
            </div>
            <span className="source-tokens">
              {formatCompactNumber(s.tokens, locale, 1)}
            </span>
            <span className={`source-trend ${s.trend}`}>{icon}</span>
          </button>
        );
      })}
    </div>
  );
}

function ConnectorGrid({
  statuses,
  locale,
  onOpenSource,
}: {
  statuses: SourceStatus[];
  locale: Locale;
  onOpenSource: (sourceId: string) => void;
}) {
  return (
    <div className="conn-grid">
      {statuses.map((st) => (
        <button
          key={st.id}
          type="button"
          className="conn-card"
          onClick={() => onOpenSource(st.id)}
          aria-label={`${st.name} ${sourceStateLabel(locale, st.state)}`}
        >
          <span className={`conn-dot ${st.state}`} />
          <div className="conn-info">
            <span className="conn-name">{st.name}</span>
            <span className="conn-state">
              {sourceStateLabel(locale, st.state)}
            </span>
          </div>
          {st.sessionCount != null && (
            <span className="conn-meta">{st.sessionCount} sess</span>
          )}
        </button>
      ))}
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
                ? formatCoverageCost(
                    s.costUsd,
                    s.pricingCoverage,
                    locale,
                    estimatedCost,
                    pricingPending,
                  )
                : pricingPending}
            </span>
          </div>
        </div>
      ))}
    </div>
  );
}

function BillingSummaryCard({
  billingState,
  locale,
  sc,
  pricingPending,
}: {
  billingState: BillingState;
  locale: Locale;
  sc: typeof appCopy["en-US"];
  pricingPending: string;
}) {
  const updatedAt =
    formatLocalizedDateTime(billingState.updatedAt ?? undefined, locale) ?? billingState.updatedAt;
  const usageLabel =
    billingState.kind === "quota"
      ? locale === "zh-CN"
        ? "周期配额"
        : "Periodic quota"
      : locale === "zh-CN"
        ? "Credits"
        : "Credits";

  return (
    <article className="source-summary-card billing-summary-card">
      <div className="source-summary-top">
        <span className="source-summary-window">{sc.billingTitle}</span>
        <span className={`coverage-pill ${billingState.state}`}>
          {billingStateLabel(locale, billingState.state)}
        </span>
      </div>
      <strong className="source-summary-value">
        {formatBillingUsage(billingState, locale, pricingPending)}
      </strong>
      <p className="source-summary-cost">{billingState.note ?? usageLabel}</p>
      <div className="source-summary-meta">
        <span>
          {sc.billingUsage} {usageLabel}
        </span>
        <span>
          {updatedAt ? `${sc.billingUpdated} ${updatedAt}` : billingStateLabel(locale, billingState.state)}
        </span>
      </div>
    </article>
  );
}

function SourceSummaryCards({
  snapshot,
  locale,
  sc,
  estimatedCost,
  pricingPending,
}: {
  snapshot: SourceDetailSnapshot;
  locale: Locale;
  sc: typeof appCopy["en-US"];
  estimatedCost: (cost: string) => string;
  pricingPending: string;
}) {
  const cards: Array<{
    id: string;
    label: string;
    summary: UsageWindowSummary;
  }> = [
    {
      id: "today",
      label: locale === "zh-CN" ? "今天" : "Today",
      summary: snapshot.todaySummary,
    },
    {
      id: "7d",
      label: "7D",
      summary: snapshot.last7dSummary,
    },
    {
      id: "30d",
      label: "30D",
      summary: snapshot.last30dSummary,
    },
    {
      id: "lifetime",
      label: locale === "zh-CN" ? "累计" : "Lifetime",
      summary: snapshot.lifetimeSummary,
    },
  ];

  return (
    <section className="source-summary-section">
      <SectionHeader label={sc.summaryWindows} />
      <div className="source-summary-grid">
        {cards.map(({ id, label, summary }) => {
          const delta = summary.deltaVsPreviousPeriod;
          const deltaTone =
            delta == null
              ? "muted"
              : delta.tokensDelta > 0
                ? "up"
                : delta.tokensDelta < 0
                  ? "down"
                  : "flat";
          const peakLabel = summary.peakDay
            ? `${sc.peakDayLabel} ${formatDayStamp(summary.peakDay.date, locale)}`
            : `${sc.shareLabel} ${formatShare(summary.exactShare, locale)}`;

          return (
            <article key={id} className="source-summary-card">
              <div className="source-summary-top">
                <span className="source-summary-window">{label}</span>
                <span className={`coverage-pill ${summary.pricingCoverage}`}>
                  {pricingCoverageLabel(locale, summary.pricingCoverage)}
                </span>
              </div>
              <strong className="source-summary-value">
                {formatCompactNumber(summary.tokens, locale, 1)}
              </strong>
              <p className={`source-summary-cost${summary.costUsd > 0 ? "" : " pending"}`}>
                {formatCoverageCost(
                  summary.costUsd,
                  summary.pricingCoverage,
                  locale,
                  estimatedCost,
                  pricingPending,
                )}
              </p>
              <div className="source-summary-meta">
                <span>
                  {formatFriendlyNumber(summary.sessions, locale, 0)} {sc.sessionsLabel}
                </span>
                <span>
                  {formatFriendlyNumber(summary.activeDays, locale, 0)} {sc.activeDaysLabel}
                </span>
                <span>{peakLabel}</span>
              </div>
              <p className={`source-summary-delta ${deltaTone}`}>
                {delta
                  ? `${formatSignedTokenDelta(delta.tokensDelta, locale)} ${
                      sc.tokensLabel
                    } · ${
                      delta.tokensPercentChange == null
                        ? pricingCoverageLabel(locale, summary.pricingCoverage)
                        : formatSignedPercent(delta.tokensPercentChange, locale)
                    }`
                  : `${formatFriendlyNumber(summary.pricedSessions, locale, 0)} / ${formatFriendlyNumber(
                      summary.sessions,
                      locale,
                      0,
                    )} ${locale === "zh-CN" ? "会话已定价" : "sessions priced"}`}
              </p>
            </article>
          );
        })}

        {snapshot.billingState ? (
          <BillingSummaryCard
            billingState={snapshot.billingState}
            locale={locale}
            sc={sc}
            pricingPending={pricingPending}
          />
        ) : null}
      </div>
    </section>
  );
}

function PeriodicBreakdown({
  title,
  rows,
  locale,
  sc,
  estimatedCost,
  pricingPending,
}: {
  title: string;
  rows: PeriodicBreakdownRow[];
  locale: Locale;
  sc: typeof appCopy["en-US"];
  estimatedCost: (cost: string) => string;
  pricingPending: string;
}) {
  if (rows.length === 0) {
    return null;
  }

  const costLabel = locale === "zh-CN" ? "费用" : "Cost";

  return (
    <article className="periodic-breakdown">
      <div className="periodic-breakdown-head">
        <div>
          <p className="trend-kicker">{title}</p>
          <h2 className="periodic-breakdown-title">{title}</h2>
        </div>
        <span className="periodic-breakdown-count">{rows.length}</span>
      </div>
      <div className="periodic-breakdown-list">
        {rows.map((row) => (
          <div
            key={`${row.label}:${row.startDate}`}
            className="periodic-breakdown-row"
          >
            <div className="periodic-breakdown-period">
              <strong>{row.label}</strong>
              <span>
                {formatPeriodDate(row.startDate, locale)} -{" "}
                {formatPeriodDate(row.endDate, locale)}
              </span>
            </div>
            <div className="periodic-breakdown-stats">
              <div className="periodic-breakdown-stat">
                <span>{sc.tokensLabel}</span>
                <strong>{formatCompactNumber(row.tokens, locale, 1)}</strong>
              </div>
              <div className="periodic-breakdown-stat">
                <span>{sc.sessionsLabel}</span>
                <strong>{formatFriendlyNumber(row.sessions, locale, 0)}</strong>
                <small>
                  {formatFriendlyNumber(row.activeDays, locale, 0)} {sc.activeDaysLabel}
                </small>
              </div>
              <div className="periodic-breakdown-stat">
                <span>{costLabel}</span>
                <strong className={row.costUsd > 0 ? "" : "pending"}>
                  {formatCoverageCost(
                    row.costUsd,
                    row.pricingCoverage,
                    locale,
                    estimatedCost,
                    pricingPending,
                  )}
                </strong>
              </div>
              <div className="periodic-breakdown-stat">
                <span>{sc.pricingCoverage}</span>
                <strong className={`coverage-inline ${row.pricingCoverage}`}>
                  {pricingCoverageLabel(locale, row.pricingCoverage)}
                </strong>
                <small>
                  {formatFriendlyNumber(row.pricedSessions, locale, 0)} /{" "}
                  {formatFriendlyNumber(row.sessions, locale, 0)}{" "}
                  {locale === "zh-CN" ? "已定价" : "priced"}
                </small>
              </div>
            </div>
          </div>
        ))}
      </div>
    </article>
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
  sc: typeof appCopy["en-US"];
  copy: ReturnType<typeof getCopy>;
  estimatedCost: (cost: string) => string;
  pricingPending: string;
  emptyMessage: string;
  onNavigateHome: () => void;
}) {
  const [sessionSort, setSessionSort] = useState<"recent" | "tokens" | "cost">("recent");

  useEffect(() => {
    setSessionSort("recent");
  }, [snapshot?.sourceId]);

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
  const sourceSummary =
    snapshot.status.note ||
    (lastSeen
      ? `${copy.connectors.lastSeen} ${lastSeen}`
      : `${sc.pricingCoverage}: ${calculationLabel(locale, snapshot.calculationMix)}`);
  const sortedSessions = sortSessions(snapshot.sessions, sessionSort);

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
              <span className="detail-chip-label">{sc.pricingCoverage}</span>
              <strong className="detail-chip-value">
                {calculationLabel(locale, snapshot.calculationMix)}
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

      <SourceSummaryCards
        snapshot={snapshot}
        locale={locale}
        sc={sc}
        estimatedCost={estimatedCost}
        pricingPending={pricingPending}
      />

      <WeeklyBurnCard
        data={snapshot.week}
        locale={locale}
        label={sc.last7Days}
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

      <section className="periodic-breakdown-section">
        <div className="periodic-breakdown-grid">
          <PeriodicBreakdown
            title={sc.weeklyBreakdown}
            rows={snapshot.periodicBreakdowns.weekly}
            locale={locale}
            sc={sc}
            estimatedCost={estimatedCost}
            pricingPending={pricingPending}
          />
          <PeriodicBreakdown
            title={sc.monthlyBreakdown}
            rows={snapshot.periodicBreakdowns.monthly}
            locale={locale}
            sc={sc}
            estimatedCost={estimatedCost}
            pricingPending={pricingPending}
          />
        </div>
      </section>

      <section className="sess-section">
        <div className="sess-section-head">
          <SectionHeader label={sc.recentSessions} />
          <label className="session-sort">
            <span>{sc.sortByLabel}</span>
            <select
              value={sessionSort}
              onChange={(event) =>
                setSessionSort(event.target.value as "recent" | "tokens" | "cost")
              }
            >
              <option value="recent">{sc.sortRecent}</option>
              <option value="tokens">{sc.sortTokens}</option>
              <option value="cost">{sc.sortCost}</option>
            </select>
          </label>
        </div>
        {sortedSessions.length > 0 ? (
          <SessionFeed
            sessions={sortedSessions}
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

  const sc = appCopy[locale];
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
          <div className="locale-sw" aria-label={copy.app.locale.label}>
            <button
              className={`locale-btn${locale === "zh-CN" ? " active" : ""}`}
              onClick={() => setLocale("zh-CN")}
              type="button"
            >
              {copy.app.locale.chinese}
            </button>
            <button
              className={`locale-btn${locale === "en-US" ? " active" : ""}`}
              onClick={() => setLocale("en-US")}
              type="button"
            >
              {copy.app.locale.english}
            </button>
          </div>
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

          {snapshot.sourceStatuses.length > 0 && (
            <section className="conn-section">
              <SectionHeader label={sc.connected} />
              <ConnectorGrid
                statuses={snapshot.sourceStatuses}
                locale={locale}
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
