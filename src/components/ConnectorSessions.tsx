import type { SessionGroup, SessionSummary } from "../data/schema";
import {
  calculationLabel,
  formatCompactNumber,
  getCopy,
  sessionStatusLabel,
  sourceStateLabel,
  type Locale
} from "../i18n";

type ConnectorSessionsProps = {
  groups: SessionGroup[];
  locale: Locale;
};

function formatTokens(value: number, locale: Locale) {
  if (value <= 0) {
    return "--";
  }

  return formatCompactNumber(value, locale);
}

function sessionMeta(session: SessionSummary, locale: Locale) {
  return [session.workspace, session.model, session.startedAt]
    .filter((value) => value && value !== getCopy(locale).common.unknown && value !== "unknown")
    .join(" · ");
}

export function ConnectorSessions({ groups, locale }: ConnectorSessionsProps) {
  const copy = getCopy(locale);

  return (
    <section className="panel grouped-session-panel">
      <div className="panel-heading grouped-session-heading">
        <div>
          <p className="panel-kicker">{copy.sessions.kicker}</p>
          <h2>{copy.sessions.title}</h2>
        </div>
        <p className="connector-summary">{copy.sessions.summary}</p>
      </div>

      <div className="grouped-session-grid">
        {groups.map((group) => (
          <article className="session-group-card" key={group.sourceId}>
            <div className="session-group-head">
              <div>
                <h3>{group.sourceName}</h3>
                <p>{copy.sessions.indexedCount(group.sessions.length)}</p>
              </div>
              <span className={`source-state-pill state-${group.sourceState}`}>
                {sourceStateLabel(locale, group.sourceState)}
              </span>
            </div>

            {group.sessions.length > 0 ? (
              <div className="session-card-list">
                {group.sessions.map((session) => (
                  <article className="session-card" key={session.id}>
                    <div className="session-card-main">
                      <strong>{session.title}</strong>
                      <p>{session.preview}</p>
                    </div>
                    <p className="session-card-meta">{sessionMeta(session, locale)}</p>
                    <div className="session-card-foot">
                      <span className={`calc-pill calc-${session.calculationMethod}`}>
                        {calculationLabel(locale, session.calculationMethod)}
                      </span>
                      <span className={`status-pill status-${session.status}`}>
                        {sessionStatusLabel(locale, session.status)}
                      </span>
                      <span className="session-card-tokens">
                        {formatTokens(session.totalTokens, locale)}
                      </span>
                    </div>
                  </article>
                ))}
              </div>
            ) : (
              <p className="empty-note">{copy.sessions.empty}</p>
            )}
          </article>
        ))}
      </div>
    </section>
  );
}
