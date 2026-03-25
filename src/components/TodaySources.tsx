import type { SourceUsage } from "../data/schema";
import {
  calculationLabel,
  formatFriendlyNumber,
  formatNumber,
  formatPercent,
  getCopy,
  type Locale
} from "../i18n";

type TodaySourcesProps = {
  locale: Locale;
  sources: SourceUsage[];
};

export function TodaySources({ locale, sources }: TodaySourcesProps) {
  const copy = getCopy(locale);
  const visibleSources = [...sources]
    .filter((source) => source.analyticsState === "ready" && ((source.tokens ?? 0) > 0 || (source.sessions ?? 0) > 0))
    .sort((left, right) => (right.tokens ?? 0) - (left.tokens ?? 0));

  if (visibleSources.length === 0) {
    return null;
  }

  const totalTokens = visibleSources.reduce((total, source) => total + (source.tokens ?? 0), 0);

  return (
    <section className="panel source-breakdown-panel">
      <div className="panel-heading grouped-session-heading">
        <div>
          <p className="panel-kicker">{copy.sources.kicker}</p>
          <h2>{copy.sources.title}</h2>
        </div>
        <p className="connector-summary">{copy.sources.summary}</p>
      </div>

      <div className="source-breakdown-meta">
        <span className="trend-pill trend-flat">{copy.sources.todayScope}</span>
      </div>

      <div className="source-breakdown-grid">
        {visibleSources.map((source) => {
          const tokenCount = source.tokens ?? 0;
          const sessionCount = source.sessions ?? 0;
          const share = totalTokens > 0 ? tokenCount / totalTokens : 0;

          return (
            <article className="source-breakdown-card" key={source.source}>
              <div className="source-meta">
                <div>
                  <h3>{source.source}</h3>
                  <p>{copy.trend.sessionsLabel(sessionCount, calculationLabel(locale, source.calculationMix))}</p>
                </div>
                <span className={`trend-pill trend-${source.trend ?? "flat"}`}>{formatPercent(share, locale)}</span>
              </div>

              <div className="source-track">
                <div
                  className="source-fill"
                  style={{ width: `${Math.max(share * 100, tokenCount > 0 ? 6 : 0)}%` }}
                />
              </div>

              <div className="source-stats">
                <span>{formatFriendlyNumber(tokenCount, locale)} tokens</span>
                <span>{formatNumber(tokenCount, locale)}</span>
              </div>
            </article>
          );
        })}
      </div>
    </section>
  );
}
