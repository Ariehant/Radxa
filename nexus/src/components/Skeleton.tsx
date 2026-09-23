export function Skeleton({ lines = 6 }: { lines?: number }) {
  return (
    <div className="skeleton" aria-busy="true">
      {Array.from({ length: lines }, (_, i) => (
        <div key={i} className="skeleton-line" style={{ width: `${55 + ((i * 37) % 40)}%` }} />
      ))}
    </div>
  );
}
