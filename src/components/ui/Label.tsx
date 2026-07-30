import type { ReactNode } from "react";

import { cn } from "../../lib/utils";

interface Props {
    className?: string;
    children: ReactNode;
    /**
     * Render as a `<legend>` instead of a `<span>`.
     *
     * A fieldset takes its accessible name from its legend and from nothing else, so a
     * span there would leave the group unnamed for a screen reader.
     */
    asLegend?: boolean;
}

/**
 * The small uppercase caption above a control.
 *
 * A component rather than four repeated utilities, because it appears on every form
 * control and above the luminance readout, and a caption that is smaller in one place
 * than another looks like a mistake.
 */
export function Label({ className, children, asLegend }: Props) {
    const classes = cn("text-[0.7rem] font-semibold uppercase tracking-[0.06em] text-text-muted", className);
    return asLegend ? (
        <legend className={cn("p-0", classes)}>{children}</legend>
    ) : (
        <span className={classes}>{children}</span>
    );
}
