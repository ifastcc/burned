import type { SessionSummary } from "../data/schema";

type SessionTableProps = {
  sessions: SessionSummary[];
};

export function SessionTable({ sessions }: SessionTableProps) {
  return (
    <section className="panel">
      <div className="panel-heading">
        <p className="panel-kicker">Session ledger</p>
        <h2>Latest indexed sessions</h2>
      </div>
      <div className="session-table">
        <div className="session-head">
          <span>Session</span>
          <span>Source</span>
          <span>Workspace</span>
          <span>Model</span>
          <span>Started</span>
          <span>Method</span>
          <span>Status</span>
          <span className="session-numeric">Tokens</span>
        </div>
        {sessions.map((session) => (
          <div className="session-row" key={session.id}>
            <div className="session-main">
              <strong>{session.title}</strong>
              <p>{session.preview}</p>
            </div>
            <span>{session.source}</span>
            <span>{session.workspace}</span>
            <span>{session.model}</span>
            <span>{session.startedAt}</span>
            <span className={`calc-pill calc-${session.calculationMethod}`}>
              {session.calculationMethod}
            </span>
            <span className={`status-pill status-${session.status}`}>
              {session.status}
            </span>
            <span className="session-numeric">
              {session.totalTokens > 0
                ? `${(session.totalTokens / 1000).toFixed(0)}k`
                : "--"}
            </span>
          </div>
        ))}
      </div>
    </section>
  );
}
