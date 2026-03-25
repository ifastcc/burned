import type { DailyUsagePoint, SourceUsage } from "../data/schema";

type UsageBarsProps = {
  week: DailyUsagePoint[];
  sources: SourceUsage[];
};

const trendCopy: Record<SourceUsage["trend"], string> = {
  up: "Burn is rising",
  flat: "Holding steady",
  down: "Cooling off"
};

export function UsageBars({ week, sources }: UsageBarsProps) {
  const maxTokens = Math.max(1, ...week.map((point) => point.totalTokens));
  const maxSourceTokens = Math.max(1, ...sources.map((source) => source.tokens));

  return (
    <section className="panel panel-grid">
      <div className="chart-card">
        <div className="panel-heading">
          <p className="panel-kicker">Seven-day burn</p>
          <h2>Daily token load</h2>
        </div>
        <div className="weekly-bars" aria-label="Weekly usage chart">
          {week.map((point) => (
            <div className="bar-day" key={point.date}>
              <span className="bar-caption">{point.date}</span>
              <div className="bar-track">
                <div
                  className="bar-fill"
                  style={{ height: `${(point.totalTokens / maxTokens) * 100}%` }}
                />
              </div>
              <span className="bar-value">
                {(point.totalTokens / 1000).toFixed(0)}k
              </span>
            </div>
          ))}
        </div>
      </div>

      <div className="source-card">
        <div className="panel-heading">
          <p className="panel-kicker">Source pressure</p>
          <h2>Where the burn comes from</h2>
        </div>
        <div className="source-list">
          {sources.map((source) => (
            <div className="source-row" key={source.source}>
              <div className="source-meta">
                <div>
                  <h3>{source.source}</h3>
                  <p>
                    {source.sessions} sessions · {source.calculationMix}
                  </p>
                </div>
                <span className={`trend-pill trend-${source.trend}`}>
                  {trendCopy[source.trend]}
                </span>
              </div>
              <div className="source-track">
                <div
                  className="source-fill"
                  style={{ width: `${(source.tokens / maxSourceTokens) * 100}%` }}
                />
              </div>
              <div className="source-stats">
                <span>{(source.tokens / 1000).toFixed(0)}k tokens</span>
                <span>
                  {source.pricingCoverage === "pending"
                    ? "pricing pending"
                    : source.pricingCoverage === "partial"
                      ? `$${source.costUsd.toFixed(2)} · partial pricing`
                      : `$${source.costUsd.toFixed(2)}`}
                </span>
              </div>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
