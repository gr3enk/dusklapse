import type { SelectHTMLAttributes } from "react";

import { cn } from "../../lib/utils";
import { Label } from "./Label";

interface Option {
    value: string;
    label: string;
}

interface Props extends Omit<SelectHTMLAttributes<HTMLSelectElement>, "children"> {
    label: string;
    options: Option[];
    /** Shown as a stable placeholder when the current value is unknown. */
    emptyLabel?: string;
    fieldClassName?: string;
}

/**
 * A labelled native select.
 *
 * Native on purpose: on iOS this becomes the system picker, which is far better on a
 * touch screen than any custom dropdown, and it needs no code to be accessible.
 */
export function Select({ label, options, emptyLabel = "-", value, fieldClassName, className, ...rest }: Props) {
    return (
        <label className={cn("flex min-w-0 flex-1 flex-col gap-1", fieldClassName)}>
            <Label>{label}</Label>
            <select
                value={value ?? ""}
                className={cn(
                    "min-h-tap rounded-lg border border-border bg-surface-raised px-2 text-[1.05rem] tabular-nums",
                    "disabled:opacity-50 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
                    className,
                )}
                {...rest}
            >
                {/* A control whose value we cannot read still needs one stable option, or
                    the browser shows the first real one and misreports the camera. */}
                {value === undefined && <option value="">{emptyLabel}</option>}
                {options.map((option) => (
                    <option key={option.value} value={option.value}>
                        {option.label}
                    </option>
                ))}
            </select>
        </label>
    );
}
