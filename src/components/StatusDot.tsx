import type { ServiceStatus } from "../ui/types";
import { useT } from "../ui/i18n";

export function StatusDot({ status, label }: { status: ServiceStatus | "valid" | "warning" | "error"; label?: string }) {
  const t = useT();
  return (
    <span className={`status status-${status}`}>
      <span className="status-dot" />
      {t(label ?? statusLabel(status))}
    </span>
  );
}

function statusLabel(status: ServiceStatus | "valid" | "warning" | "error") {
  if (status === "running") return "Running";
  if (status === "starting") return "Starting";
  if (status === "stopped") return "Stopped";
  if (status === "valid") return "Valid";
  if (status === "warning") return "Warning";
  if (status === "error") return "Error";
  return "Error";
}
