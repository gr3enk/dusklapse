import type { ReactNode } from "react";

import { cn } from "../../lib/utils";

/**
 * A small pill for a single reading - frame count, battery, and whatever else the status
 * strip grows.
 *
 * Tabular figures on purpose: these are watched continuously while a sequence runs, and
 * proportional digits make the pill twitch sideways as the number changes.
 */
export function Badge({ className, children }: { className?: string; children: ReactNode }) {
    return <span className={cn("rounded-full border border-border px-[0.6rem] py-[0.3rem] text-[0.8rem] whitespace-nowrap tabular-nums text-text-muted", className)}>{children}</span>;
}
