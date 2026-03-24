type MetricTileProps = {
  eyebrow: string;
  value: string;
  detail: string;
};

export function MetricTile({ eyebrow, value, detail }: MetricTileProps) {
  return (
    <article className="metric-tile">
      <p className="metric-eyebrow">{eyebrow}</p>
      <h3 className="metric-value">{value}</h3>
      <p className="metric-detail">{detail}</p>
    </article>
  );
}
