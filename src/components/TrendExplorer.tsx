import { useState } from "react";
import type { DailyUsagePoint } from "../data/schema";
import {
  formatCompactNumber,
  formatFriendlyNumber,
  formatNumber,
  formatPercent,
  getCopy,
  localeTag,
  type Granularity,
  type Locale
} from "../i18n";

type TrendExplorerProps = {
  history: DailyUsagePoint[];
  locale: Locale;
};

type PeriodPoint = {
  key: string;
  label: string;
  subLabel: string;
  rangeStart: string;
  rangeEnd: string;
  totalTokens: number;
  totalCostUsd: number;
  exactShare: number;
  activeSources: number;
  sessionCount: number;
};

const windowSizeByGranularity: Record<Granularity, number> = {
  day: 14,
  week: 12,
  month: 12
};

function parseLocalDate(isoDate: string) {
  return new Date(`${isoDate}T12:00:00`);
}

function toIsoDate(date: Date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function startOfWeek(date: Date) {
  const nextDate = new Date(date);
  const offset = (nextDate.getDay() + 6) % 7;
  nextDate.setDate(nextDate.getDate() - offset);
  return nextDate;
}

function formatDayLabel(isoDate: string, locale: Locale) {
  const date = parseLocalDate(isoDate);
  return {
    label: new Intl.DateTimeFormat(localeTag(locale), { weekday: "short" }).format(date),
    subLabel: new Intl.DateTimeFormat(localeTag(locale), {
      month: "numeric",
      day: "numeric"
    }).format(date)
  };
}

function formatShortDate(isoDate: string, locale: Locale) {
  return new Intl.DateTimeFormat(localeTag(locale), {
    month: "short",
    day: "numeric"
  }).format(parseLocalDate(isoDate));
}

function formatMonthLabel(isoDate: string, locale: Locale) {
  return new Intl.DateTimeFormat(localeTag(locale), {
    month: "short",
    year: "numeric"
  }).format(parseLocalDate(isoDate));
}

function aggregateHistory(
  history: DailyUsagePoint[],
  granularity: Granularity,
  locale: Locale
): PeriodPoint[] {
  const copy = getCopy(locale);

  if (granularity === "day") {
    return history.map((point) => {
      const { label, subLabel } = formatDayLabel(point.date, locale);
      return {
        key: point.date,
        label,
        subLabel,
        rangeStart: point.date,
        rangeEnd: point.date,
        totalTokens: point.totalTokens,
        totalCostUsd: point.totalCostUsd,
        exactShare: point.exactShare,
        activeSources: point.activeSources,
        sessionCount: point.sessionCount
      };
    });
  }

  const buckets = new Map<
    string,
    {
      rangeStart: string;
      rangeEnd: string;
      totalTokens: number;
      totalCostUsd: number;
      exactTokens: number;
      activeSources: number;
      sessionCount: number;
      dayCount: number;
    }
  >();

  history.forEach((point) => {
    const date = parseLocalDate(point.date);
    const bucketStart =
      granularity === "week"
        ? startOfWeek(date)
        : new Date(date.getFullYear(), date.getMonth(), 1);
    const key = toIsoDate(bucketStart);
    const bucket = buckets.get(key) ?? {
      rangeStart: point.date,
      rangeEnd: point.date,
      totalTokens: 0,
      totalCostUsd: 0,
      exactTokens: 0,
      activeSources: 0,
      sessionCount: 0,
      dayCount: 0
    };

    bucket.rangeStart = bucket.rangeStart < point.date ? bucket.rangeStart : point.date;
    bucket.rangeEnd = bucket.rangeEnd > point.date ? bucket.rangeEnd : point.date;
    bucket.totalTokens += point.totalTokens;
    bucket.totalCostUsd += point.totalCostUsd;
    bucket.exactTokens += point.totalTokens * point.exactShare;
    bucket.activeSources = Math.max(bucket.activeSources, point.activeSources);
    bucket.sessionCount += point.sessionCount;
    bucket.dayCount += 1;
    buckets.set(key, bucket);
  });

  return Array.from(buckets.entries()).map(([key, bucket]) => {
    if (granularity === "week") {
      return {
        key,
        label: formatShortDate(bucket.rangeStart, locale),
        subLabel: copy.trend.weekSpan,
        rangeStart: bucket.rangeStart,
        rangeEnd: bucket.rangeEnd,
        totalTokens: bucket.totalTokens,
        totalCostUsd: bucket.totalCostUsd,
        exactShare: bucket.totalTokens === 0 ? 0 : bucket.exactTokens / bucket.totalTokens,
        activeSources: bucket.activeSources,
        sessionCount: bucket.sessionCount
      };
    }

    return {
      key,
      label: formatMonthLabel(bucket.rangeStart, locale),
      subLabel: copy.trend.dayCount(bucket.dayCount),
      rangeStart: bucket.rangeStart,
      rangeEnd: bucket.rangeEnd,
      totalTokens: bucket.totalTokens,
      totalCostUsd: bucket.totalCostUsd,
      exactShare: bucket.totalTokens === 0 ? 0 : bucket.exactTokens / bucket.totalTokens,
      activeSources: bucket.activeSources,
      sessionCount: bucket.sessionCount
    };
  });
}

function windowRangeLabel(points: PeriodPoint[], locale: Locale) {
  const first = points[0];
  const last = points[points.length - 1];

  if (!first || !last) {
    return "";
  }

  if (first.rangeStart === last.rangeEnd) {
    return formatShortDate(first.rangeStart, locale);
  }

  return `${formatShortDate(first.rangeStart, locale)} - ${formatShortDate(
    last.rangeEnd,
    locale
  )}`;
}

export function TrendExplorer({ history, locale }: TrendExplorerProps) {
  const copy = getCopy(locale);
  const [granularity, setGranularity] = useState<Granularity>("day");
  const [pageByGranularity, setPageByGranularity] = useState<Record<Granularity, number>>({
    day: 0,
    week: 0,
    month: 0
  });

  const periods = aggregateHistory(history, granularity, locale);
  if (periods.length === 0) {
    return (
      <section className="panel trend-panel">
        <div className="trend-empty">
          <p className="panel-kicker">{copy.trend.kicker}</p>
          <h2>{copy.trend.emptyTitle}</h2>
        </div>
      </section>
    );
  }

  const windowSize = Math.min(windowSizeByGranularity[granularity], periods.length);
  const maxPage = Math.max(0, Math.ceil(periods.length / windowSize) - 1);
  const page = Math.min(pageByGranularity[granularity], maxPage);
  const end = periods.length - page * windowSize;
  const start = Math.max(0, end - windowSize);
  const visiblePeriods = periods.slice(start, end);
  const previousPeriods = periods.slice(Math.max(0, start - windowSize), start);
  const currentTokens = visiblePeriods.reduce((total, period) => total + period.totalTokens, 0);
  const averageTokens =
    visiblePeriods.length === 0 ? 0 : Math.round(currentTokens / visiblePeriods.length);
  const previousTokens = previousPeriods.reduce((total, period) => total + period.totalTokens, 0);
  const tokenDelta =
    previousPeriods.length === 0 || previousTokens === 0
      ? null
      : (currentTokens - previousTokens) / previousTokens;
  const weightedExactTokens = visiblePeriods.reduce(
    (total, period) => total + period.totalTokens * period.exactShare,
    0
  );
  const exactShare = currentTokens === 0 ? 0 : weightedExactTokens / Math.max(currentTokens, 1);
  const peakSources = visiblePeriods.reduce(
    (max, period) => Math.max(max, period.activeSources),
    0
  );
  const sessionActivity = visiblePeriods.reduce(
    (total, period) => total + period.sessionCount,
    0
  );
  const peakPeriod = [...visiblePeriods].sort((left, right) => right.totalTokens - left.totalTokens)[0];
  const maxTokens = Math.max(1, ...visiblePeriods.map((period) => period.totalTokens));
  const chartWidth = Math.max(560, visiblePeriods.length * 76);
  const chartHeight = 280;
  const horizontalPadding = 28;
  const verticalPadding = 20;
  const baseline = chartHeight - 34;
  const chartPoints = visiblePeriods.map((period, index) => {
    const x =
      visiblePeriods.length === 1
        ? chartWidth / 2
        : horizontalPadding +
          (index * (chartWidth - horizontalPadding * 2)) / (visiblePeriods.length - 1);
    const y =
      baseline - (period.totalTokens / maxTokens) * (baseline - verticalPadding);
    return { x, y };
  });
  const linePath = chartPoints
    .map(({ x, y }, index) => `${index === 0 ? "M" : "L"} ${x} ${y}`)
    .join(" ");
  const areaPath =
    chartPoints.length > 0
      ? `${linePath} L ${chartPoints[chartPoints.length - 1]?.x ?? 0} ${baseline} L ${
          chartPoints[0]?.x ?? 0
        } ${baseline} Z`
      : "";

  function updatePage(nextPage: number) {
    setPageByGranularity((current) => ({
      ...current,
      [granularity]: Math.max(0, Math.min(maxPage, nextPage))
    }));
  }

  const deltaText =
    tokenDelta == null
      ? copy.trend.noPriorSlice(copy.trend.granularity[granularity])
      : copy.trend.previousWindowDelta(
          `${tokenDelta >= 0 ? "+" : ""}${formatPercent(Math.abs(tokenDelta), locale)}`,
          copy.trend.granularity[granularity]
        );

  const peakLabel = peakPeriod ? `${peakPeriod.label} ${peakPeriod.subLabel}` : "--";

  return (
    <section className="panel trend-panel">
      <div className="trend-main trend-main-full">
        <div className="panel-heading trend-heading">
          <div>
            <p className="panel-kicker">{copy.trend.kicker}</p>
            <h2>{copy.trend.title}</h2>
          </div>
          <p className="trend-range">{windowRangeLabel(visiblePeriods, locale)}</p>
        </div>

        <div className="trend-toolbar">
          <div className="segmented-control" role="tablist" aria-label={copy.trend.tabAriaLabel}>
            {(["day", "week", "month"] as const).map((option) => (
              <button
                aria-selected={option === granularity}
                className={`segment-button ${
                  option === granularity ? "segment-button-active" : ""
                }`}
                key={option}
                onClick={() => setGranularity(option)}
                type="button"
              >
                {copy.trend.granularity[option]}
              </button>
            ))}
          </div>

          <div className="trend-nav">
            <button
              className="ghost-button"
              disabled={page >= maxPage}
              onClick={() => updatePage(page + 1)}
              type="button"
            >
              {copy.trend.earlier}
            </button>
            <button
              className="ghost-button"
              disabled={page === 0}
              onClick={() => updatePage(page - 1)}
              type="button"
            >
              {copy.trend.later}
            </button>
          </div>
        </div>

        <div className="trend-summary-strip">
          <article className="trend-summary-tile">
            <span>{copy.trend.selectedRange}</span>
            <strong>{formatFriendlyNumber(currentTokens, locale, 2)}</strong>
            <p>{copy.trend.selectedRangeDetail(deltaText)}</p>
          </article>
          <article className="trend-summary-tile">
            <span>{copy.trend.averagePerPeriod(copy.trend.granularity[granularity])}</span>
            <strong>{formatFriendlyNumber(averageTokens, locale, 2)}</strong>
            <p>{copy.trend.averagePerPeriodDetail(copy.trend.granularity[granularity])}</p>
          </article>
          <article className="trend-summary-tile">
            <span>{copy.trend.peakPeriod}</span>
            <strong>{formatFriendlyNumber(peakPeriod?.totalTokens ?? 0, locale, 2)}</strong>
            <p>{copy.trend.peakPeriodDetail(peakLabel)}</p>
          </article>
        </div>

        <div className="trend-meta-row">
          <span className="trend-meta-pill">
            {copy.trend.confidence}: {formatPercent(exactShare, locale)}
          </span>
          <span className="trend-meta-pill">
            {copy.trend.peakConnectors}: {peakSources}
          </span>
          <span className="trend-meta-pill">
            {copy.trend.sessionActivity}: {formatNumber(sessionActivity, locale)}
          </span>
        </div>

        <div className="trend-scroll-shell">
          <div className="trend-scroll" style={{ minWidth: `${chartWidth}px` }}>
            <div className="trend-chart-frame">
              <svg
                aria-label="Usage trend chart"
                className="trend-chart"
                role="img"
                viewBox={`0 0 ${chartWidth} ${chartHeight}`}
              >
                <defs>
                  <linearGradient id="trendAreaFill" x1="0%" x2="0%" y1="0%" y2="100%">
                    <stop offset="0%" stopColor="#ffca7a" stopOpacity="0.45" />
                    <stop offset="100%" stopColor="#ff8a45" stopOpacity="0.04" />
                  </linearGradient>
                </defs>

                {[0.25, 0.5, 0.75].map((ratio) => {
                  const y = baseline - ratio * (baseline - verticalPadding);
                  return (
                    <line
                      className="trend-gridline"
                      key={ratio}
                      x1={horizontalPadding}
                      x2={chartWidth - horizontalPadding}
                      y1={y}
                      y2={y}
                    />
                  );
                })}

                <path className="trend-area" d={areaPath} />
                <path className="trend-line" d={linePath} />

                {chartPoints.map((point, index) => (
                  <g key={visiblePeriods[index]?.key}>
                    <circle className="trend-point" cx={point.x} cy={point.y} r="4.5" />
                    <line
                      className="trend-stem"
                      x1={point.x}
                      x2={point.x}
                      y1={point.y + 12}
                      y2={baseline}
                    />
                  </g>
                ))}
              </svg>
            </div>

            <div
              className="trend-axis"
              style={{
                gridTemplateColumns: `repeat(${visiblePeriods.length}, minmax(0, 1fr))`
              }}
            >
              {visiblePeriods.map((period) => (
                <div className="trend-axis-item" key={period.key}>
                  <span className="trend-axis-label">{period.label}</span>
                  <strong className="trend-axis-value">
                    {formatFriendlyNumber(period.totalTokens, locale)}
                  </strong>
                  <span className="trend-axis-sublabel">{period.subLabel}</span>
                </div>
              ))}
            </div>
          </div>
        </div>

        <p className="trend-scale-note">{copy.trend.scaleNote(formatCompactNumber(maxTokens, locale))}</p>
      </div>
    </section>
  );
}
