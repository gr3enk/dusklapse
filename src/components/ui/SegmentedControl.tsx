import { cn } from "../../lib/utils";

interface Option<T extends string> {
    value: T;
    label: string;
}

interface Props<T extends string> {
    options: Option<T>[];
    value: T;
    onChange: (value: T) => void;
    className?: string;
    "aria-label"?: string;
}

/**
 * A row of mutually exclusive choices.
 *
 * `aria-pressed` rather than a radio group: these are buttons that act immediately, and
 * announcing them as a set of radios would promise a form field that has to be
 * submitted.
 *
 * The track uses `auto-fit` with a minimum, so options wrap instead of squeezing
 * themselves narrower than their labels.
 */
export function SegmentedControl<T extends string>({ options, value, onChange, className, ...rest }: Props<T>) {
    return (
        <div
            role="group"
            className={cn(
                "grid grid-cols-[repeat(auto-fit,minmax(5rem,1fr))] gap-[0.35rem] rounded-card border border-border bg-surface p-[0.3rem]",
                className,
            )}
            {...rest}
        >
            {options.map((option) => {
                const active = option.value === value;
                return (
                    <button
                        key={option.value}
                        type="button"
                        aria-pressed={active}
                        onClick={() => onChange(option.value)}
                        className={cn(
                            "min-h-9 cursor-pointer rounded-lg border-0 bg-transparent text-[0.95rem] text-text-muted",
                            "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
                            active &&
                                "bg-surface-raised font-semibold text-text shadow-[inset_0_0_0_1px_var(--border)]",
                        )}
                    >
                        {option.label}
                    </button>
                );
            })}
        </div>
    );
}
