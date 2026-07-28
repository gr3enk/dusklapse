import type { InputHTMLAttributes } from "react";

import { cn } from "../../lib/utils";
import { Label } from "./Label";

interface Props extends InputHTMLAttributes<HTMLInputElement> {
    label: string;
    /** Wrapper class, for the flex sizing the row around it needs. */
    fieldClassName?: string;
}

/**
 * A labelled text input.
 *
 * `text-base` is not cosmetic: below 16px iOS zooms the whole viewport when the field
 * takes focus, which on this layout throws the controls off screen.
 */
export function TextField({ label, fieldClassName, className, ...rest }: Props) {
    return (
        <label className={cn("flex min-w-0 flex-1 flex-col gap-1", fieldClassName)}>
            <Label>{label}</Label>
            <input
                className={cn(
                    "min-h-tap rounded-card border border-border bg-surface px-[0.85rem] text-base",
                    "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
                    className,
                )}
                {...rest}
            />
        </label>
    );
}
