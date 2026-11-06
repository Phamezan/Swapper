import type { ReactNode, SyntheticEvent } from "react";
import { Tooltip as TooltipPrimitive } from "radix-ui";
import "./tooltip.css";

export const TooltipProvider = ({ children }: { children: ReactNode }) =>
  <TooltipPrimitive.Provider delayDuration={300} skipDelayDuration={300} disableHoverableContent>{children}</TooltipPrimitive.Provider>;

// Let an inner tooltip own focus/hover instead of opening its ancestors too.
function ignoreNestedTrigger(event: SyntheticEvent) {
  if ((event.target as Element).closest("[data-tooltip]") !== event.currentTarget) event.preventDefault();
}

/** Styled replacement for a native `title` tooltip. Wraps `children` with `asChild`, so it adds no extra DOM node.
 *  `data-tooltip` mirrors the label onto the trigger element so e2e tests can query it without simulating a hover. */
export function Tooltip({ label, side = "top", children }: { label?: string; side?: "top" | "right" | "bottom" | "left"; children: ReactNode }) {
  if (!label) return children;
  return <TooltipPrimitive.Root>
    <TooltipPrimitive.Trigger asChild data-tooltip={label} onFocus={ignoreNestedTrigger} onPointerMove={ignoreNestedTrigger}>{children}</TooltipPrimitive.Trigger>
    <TooltipPrimitive.Portal>
      <TooltipPrimitive.Content className="ui-tooltip" side={side} sideOffset={6} collisionPadding={8}>
        {label}
        <TooltipPrimitive.Arrow className="ui-tooltip-arrow" />
      </TooltipPrimitive.Content>
    </TooltipPrimitive.Portal>
  </TooltipPrimitive.Root>;
}
