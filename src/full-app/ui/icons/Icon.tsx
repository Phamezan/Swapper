import type { ReactNode, SVGProps } from "react";
import "./icons.css";

export type IconProps = Omit<SVGProps<SVGSVGElement>, "children"> & { size?: number };

/** Shared 24px frame for Swapper's icon set: 1.75 stroke, round caps and joins (matches Outfit).
 *  Decorative by default; pass `aria-label` (and `aria-hidden={false}`) for a standalone icon. */
export function Icon({ size = 24, className, children, ...props }: IconProps & { children: ReactNode }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.75}
    strokeLinecap="round" strokeLinejoin="round" aria-hidden focusable="false" {...props}
    className={className ? `sw-icon ${className}` : "sw-icon"}>{children}</svg>;
}
