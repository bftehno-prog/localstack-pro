import type { ButtonHTMLAttributes, ReactNode } from "react";
import { useT } from "../ui/i18n";

type Variant = "primary" | "neutral" | "danger" | "icon";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  icon?: ReactNode;
}

export function Button({ variant = "neutral", icon, children, className = "", title, "aria-label": ariaLabel, ...props }: ButtonProps) {
  const t = useT();
  const label = typeof children === "string" ? t(children) : children;
  const tooltip = title ?? (typeof ariaLabel === "string" ? String(t(ariaLabel)) : typeof children === "string" ? String(t(children)) : undefined);
  const ariaText = typeof ariaLabel === "string" ? String(t(ariaLabel)) : tooltip;
  return (
    <button className={`btn btn-${variant} ${className}`} title={tooltip} aria-label={ariaText} {...props}>
      {icon}
      {label && <span>{label}</span>}
    </button>
  );
}
