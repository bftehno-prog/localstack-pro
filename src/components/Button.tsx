import type { ButtonHTMLAttributes, ReactNode } from "react";
import { useT } from "../ui/i18n";

type Variant = "primary" | "neutral" | "danger" | "icon";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  icon?: ReactNode;
}

export function Button({ variant = "neutral", icon, children, className = "", ...props }: ButtonProps) {
  const t = useT();
  const label = typeof children === "string" ? t(children) : children;
  return (
    <button className={`btn btn-${variant} ${className}`} {...props}>
      {icon}
      {label && <span>{label}</span>}
    </button>
  );
}
