import type { ReactNode } from "react";

import { Button } from "./Button";
import { Modal } from "./Modal";
import { cn } from "../../lib/utils";

interface Props {
    open: boolean;
    title: string;
    /** What the action will do. Say the consequence, not the question. */
    children: ReactNode;
    confirmLabel: string;
    cancelLabel?: string;
    /** Colours the confirming button as a warning. For anything that loses work. */
    destructive?: boolean;
    onConfirm: () => void;
    onCancel: () => void;
}

/**
 * A modal that asks before doing something that cannot be taken back.
 *
 * Cancel comes first in the DOM so it takes focus when the dialog opens, and Escape and a click on
 * the backdrop both cancel. Every way of dismissing this without reading it therefore lands on the
 * safe side, which is the whole reason the dialog exists.
 *
 * Visually the confirming button is still on the right, where a confirmation is looked for - the
 * `order` utilities separate the reading order from the focus order.
 */
export function ConfirmDialog({
    open,
    title,
    children,
    confirmLabel,
    cancelLabel = "Cancel",
    destructive,
    onConfirm,
    onCancel,
}: Props) {
    return (
        <Modal open={open} onClose={onCancel} title={title} className="w-[min(26rem,92vw)]">
            <div className="text-[0.95rem] text-text-muted">{children}</div>

            <div className="flex flex-wrap justify-end gap-2">
                <Button onClick={onCancel} className="order-1">
                    {cancelLabel}
                </Button>
                <Button
                    variant={destructive ? "secondary" : "primary"}
                    onClick={onConfirm}
                    className={cn("order-2", destructive && "border-danger/50 bg-danger/15 text-danger")}
                >
                    {confirmLabel}
                </Button>
            </div>
        </Modal>
    );
}
