import type { ButtonHTMLAttributes, ReactNode } from "react";

import { cn } from "../../lib/utils";

export type ButtonVariant = "primary" | "secondary" | "icon" | "danger";
export type ButtonSize = "default" | "compact";

interface Props extends ButtonHTMLAttributes<HTMLButtonElement> {
    variant?: ButtonVariant;
    size?: ButtonSize;
    children?: ReactNode;
}

const BASE =
    "inline-flex items-center justify-center gap-2 rounded-card border font-[550] " +
    "cursor-pointer select-none " +
    "active:not-disabled:translate-y-px disabled:opacity-50 disabled:cursor-default " +
    "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

const VARIANTS: Record<ButtonVariant, string> = {
    primary: "border-transparent bg-accent text-accent-text font-[650]",
    secondary: "border-border bg-surface-raised text-text",
    danger: "border-danger bg-danger/10 text-danger",
    /**
     * Square, and coloured like `primary`.
     *
     * Shape and colour are fused here because this is the only icon button the app has.
     * The moment a neutral one is needed, the two want splitting into separate props
     * rather than growing an `icon-secondary` variant.
     */
    icon: "border-transparent bg-accent text-accent-text aspect-square p-0 shrink-0",
};

const SIZES: Record<ButtonSize, string> = {
    default: "min-h-tap px-[1.1rem]",
    compact: "min-h-9 px-[0.7rem] text-[0.85rem]",
};

/**
 * Every button in the app.
 *
 * `min-h-tap` is the load-bearing part: iOS wants 44pt on anything tappable, and this is
 * the one place that can guarantee it for all of them at once.
 *
 * `type` defaults to `"button"`. HTML defaults it to `"submit"`, so a button dropped
 * into a form submits it - a footgun better opted into here than remembered at every
 * call site.
 */
export function Button({ variant = "secondary", size = "default", type = "button", className, children, ...rest }: Props) {
    return (
        <button
            type={type}
            // An icon button is sized by its square, so the padding scale would only
            // fight it.
            className={cn(BASE, VARIANTS[variant], variant === "icon" ? "min-h-tap w-tap" : SIZES[size], className)}
            {...rest}
        >
            {children}
        </button>
    );
}
