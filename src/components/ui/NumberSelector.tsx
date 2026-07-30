import { MinusIcon, PlusIcon } from "lucide-react";

import { Button } from "./Button";
import { ClassValue } from "clsx";
import { cn } from "../../lib/utils";

interface Props {
    value: number;
    secondaryValue?: number;
    onChange: (value: number) => void;
    disabled?: boolean;
    step?: number;
    min?: number;
    max?: number;
    /** Announced to screen readers, which otherwise hear only "minus" and "plus". */
    label?: string;
    className?: ClassValue;
}

/**
 * Nudge a number up or down by a fixed step.
 *
 * The value in the middle is a readout, not a control. It was a `Button` whose click
 * handler set the value it already held - focusable, pressable, and doing nothing, which
 * is the kind of thing people press twice before deciding the app is broken.
 *
 * Clamping is the other reason this takes bounds: the luminance scale runs 0 to 10000, and
 * without a limit the buttons walk the reference off the end of it into a value the backend
 * has to reject.
 */
export default function NumberSelector({
    value,
    secondaryValue,
    onChange,
    disabled = false,
    step = 100,
    min = Number.NEGATIVE_INFINITY,
    max = Number.POSITIVE_INFINITY,
    label,
    className,
}: Props) {
    const clamp = (next: number) => Math.min(max, Math.max(min, next));

    return (
        <div className={cn("flex items-center gap-2", className)} role="group" aria-label={label}>
            <Button
                variant="secondary"
                onClick={() => onChange(clamp(value - step))}
                disabled={disabled || value <= min}
                aria-label={label ? `Decrease ${label}` : "Decrease"}
            >
                <MinusIcon className="size-4" />
            </Button>
            <div className="flex flex-col flex-1">
                <output className="text-center tabular-nums text-text">{value}</output>
                {secondaryValue && (
                    <output className="text-alert-info text-center tabular-nums">{secondaryValue}</output>
                )}
            </div>
            <Button
                variant="secondary"
                onClick={() => onChange(clamp(value + step))}
                disabled={disabled || value >= max}
                aria-label={label ? `Increase ${label}` : "Increase"}
            >
                <PlusIcon className="size-4" />
            </Button>
        </div>
    );
}
