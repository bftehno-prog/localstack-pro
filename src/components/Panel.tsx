import type { ReactNode } from "react";
import { useT } from "../ui/i18n";

export function Panel({ title, action, children, className = "" }: { title?: string; action?: ReactNode; children: ReactNode; className?: string }) {
  const t = useT();
  return (
    <section className={`panel ${className}`}>
      {(title || action) && (
        <div className="panel-head">
          {title && <h2>{t(title)}</h2>}
          {action}
        </div>
      )}
      {children}
    </section>
  );
}
