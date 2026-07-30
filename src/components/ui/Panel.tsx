import type { ReactNode } from "react";

import { cn } from "../../lib/utils";

/**
 * The raised surface the working screen is built from - status strip, ramping column,
 * preview frame.
 *
 * Exists so the border, radius and background of those three cannot drift apart; each
 * one still brings its own padding and layout, which differ for good reasons.
 */
export function Panel({
    className,
    children,
    ...rest
}: { className?: string; children?: ReactNode } & React.HTMLAttributes<HTMLDivElement>) {
    return (
        <div className={cn("rounded-card border border-border bg-surface", className)} {...rest}>
            {children}
        </div>
    );
}
