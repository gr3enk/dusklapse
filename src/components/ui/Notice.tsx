import type { ReactNode } from "react";

import { cn } from "../../lib/utils";

export type NoticeVariant = "error" | "info";

const VARIANTS: Record<NoticeVariant, string> = {
    error: "border-danger/40 bg-danger/15 text-danger",
    info: "border-border bg-surface text-text-muted",
};

/**
 * A short block of prose that is not part of the flow of controls.
 *
 * Errors carry `role="alert"` so a screen reader announces them when they appear; that
 * is the whole reason this distinguishes the two variants rather than taking a colour.
 */
export function Notice({ variant = "info", className, children }: { variant?: NoticeVariant; className?: string; children: ReactNode }) {
    return (
        <p className={cn("m-0 rounded-card border px-[0.9rem] py-3 text-[0.9rem]", VARIANTS[variant], className)} role={variant === "error" ? "alert" : undefined}>
            {children}
        </p>
    );
}
