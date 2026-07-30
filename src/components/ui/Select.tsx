import type { ReactNode, SelectHTMLAttributes } from "react";

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
    /**
     * Keep the label for assistive technology but not on screen.
     *
     * For the cases where a visible caption already sits next to the control; two copies of
     * the same words is noise, and dropping the label entirely would leave the select
     * unnamed for a screen reader.
     */
    hideLabel?: boolean;
    /**
     * Offer the empty option even when a value is set, so the choice can be cleared.
     *
     * Off by default and deliberately so: on the exposure dials an empty selection would be
     * a value sent to the camera that it never offered. Only settings that are genuinely
     * optional - a ramp limit that has not been chosen - want this.
     */
    allowEmpty?: boolean;
    /**
     * Rendered beside the caption, for a status marker that belongs to this control.
     *
     * Separate from `label` so that stays a plain string: it is also the accessible name and
     * the `sr-only` fallback, and a node there would put markup in both.
     */
    labelAdornment?: ReactNode;
}

/**
 * A labelled native select.
 *
 * Native on purpose: on iOS this becomes the system picker, which is far better on a
 * touch screen than any custom dropdown, and it needs no code to be accessible.
 */
export function Select({
    label,
    options,
    emptyLabel = "-",
    value,
    fieldClassName,
    className,
    hideLabel,
    allowEmpty,
    labelAdornment,
    ...rest
}: Props) {
    return (
        <label className={cn("flex min-w-0 flex-1 flex-col gap-1", fieldClassName)}>
            {hideLabel ? (
                <span className="sr-only">{label}</span>
            ) : (
                <Label className="flex items-center gap-1.5">
                    {label}
                    {labelAdornment}
                </Label>
            )}
            <select
                value={value ?? ""}
                className={cn(
                    "min-h-tap rounded-lg border border-border bg-surface-raised px-2 text-[1.05rem] tabular-nums",
                    "disabled:opacity-50 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
                    className,
                )}
                {...rest}
            >
                {/* A control whose value we cannot read still needs one stable option, or the
                    browser shows the first real one and misreports the camera. */}
                {(allowEmpty || value === undefined) && <option value="">{emptyLabel}</option>}
                {options.map((option) => (
                    <option key={option.value} value={option.value}>
                        {option.label}
                    </option>
                ))}
            </select>
        </label>
    );
}
