import type { ReactNode } from "react";

import { cn } from "../../lib/utils";

interface Props<T extends string> {
    items: { key: T; content: ReactNode }[];
    active: T;
    className?: string;
}

/**
 * Shows one of several short texts without the box changing height.
 *
 * Every item is rendered into the same grid cell and all but one is hidden, so the box is
 * always as tall as the longest of them. A `min-height` in `em` would need a magic number
 * and would still be wrong at some width, because how many lines a text wraps to depends
 * on the screen.
 *
 * `invisible` rather than `hidden`: a hidden item must still take part in sizing the
 * cell, which is the entire point. It also keeps it out of the accessibility tree and the
 * tab order.
 */
export function HintStack<T extends string>({ items, active, className }: Props<T>) {
    return (
        <div className={cn("grid text-[0.85rem] text-text-muted", className)}>
            {items.map((item) => (
                <span
                    key={item.key}
                    className={cn("[grid-area:1/1]", item.key === active ? "visible" : "invisible")}
                    aria-hidden={item.key !== active}
                >
                    {item.content}
                </span>
            ))}
        </div>
    );
}
