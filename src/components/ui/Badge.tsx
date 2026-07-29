import type { HTMLAttributes, ReactNode } from "react";

import { cn } from "../../lib/utils";

/**
 * A small pill for a single reading - frame count, battery, and whatever else the status
 * strip grows.
 *
 * Tabular figures on purpose: these are watched continuously while a sequence runs, and
 * proportional digits make the pill twitch sideways as the number changes.
 *
 * Span attributes pass through, which is what lets a pill carry a `title`. Several of these are a
 * number beside an icon and nothing else - without a name on hover or to a screen reader, "7.0s"
 * next to a clock is a guess.
 */
interface Props extends HTMLAttributes<HTMLSpanElement> {
    children: ReactNode;
}

export function Badge({ className, children, ...rest }: Props) {
    return (
        <span className={cn("rounded-full border border-border px-[0.6rem] py-[0.3rem] text-[0.8rem] whitespace-nowrap tabular-nums text-text-muted", className)} {...rest}>
            {children}
        </span>
    );
}
