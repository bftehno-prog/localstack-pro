export function Logo({ compact = false }: { compact?: boolean }) {
  return (
    <div className="brand">
      <div className="brand-mark" aria-hidden="true">
        <span />
        <span />
        <span />
      </div>
      {!compact && (
        <div>
          <strong>LocalStack Pro</strong>
          <small>v1.0.2</small>
        </div>
      )}
    </div>
  );
}
