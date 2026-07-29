import { XIcon } from "lucide-react";
import { useEffect, useRef, type ReactNode } from "react";

import { Button } from "./Button";
import { cn } from "../../lib/utils";

interface Props {
    open: boolean;
    onClose: () => void;
    title: string;
    children: ReactNode;
    className?: string;
}

/**
 * A modal sheet over the working screen.
 *
 * Built on the native `<dialog>` rather than a positioned div: `showModal()` brings focus
 * trapping, restoring focus to whatever opened it, Escape to dismiss, inertness of the page
 * behind, and a real top layer that no `z-index` on the preview can beat. All of that is work
 * nobody should redo, and most of it is the part hand-rolled modals get wrong.
 *
 * Supported from iOS 15.4, and this app asks for 16.4.
 */
export function Modal({ open, onClose, title, children, className }: Props) {
    const dialog = useRef<HTMLDialogElement | null>(null);

    useEffect(() => {
        const element = dialog.current;
        if (!element) return;

        // `open` as an attribute would show it non-modally, without the top layer or the focus
        // trap, so the two states are driven through the methods instead.
        if (open && !element.open) element.showModal();
        else if (!open && element.open) element.close();
    }, [open]);

    return (
        <dialog
            ref={dialog}
            // Fires for Escape and for `close()` alike, so the caller's state cannot drift out of
            // step with what is actually on screen.
            onClose={onClose}
            // The backdrop is part of the element, so a click lands on the dialog itself. Comparing
            // the target is what distinguishes "outside" from "on the content".
            onClick={(event) => {
                if (event.target === dialog.current) onClose();
            }}
            className={cn(
                "m-auto max-h-[85dvh] w-[min(34rem,92vw)] overflow-y-auto overscroll-contain",
                "rounded-card border border-border bg-surface p-0 text-text",
                "backdrop:bg-black/60 backdrop:backdrop-blur-sm",
                className,
            )}
        >
            {/* Sticky so the way out stays reachable however long the list below grows. */}
            <header className="sticky top-0 flex items-center justify-between gap-3 border-b border-border bg-surface px-4 py-3">
                <h2 className="m-0 text-[1.05rem] font-[650]">{title}</h2>
                <Button variant="icon" onClick={onClose} aria-label="Close" className="min-h-0 size-9 w-9">
                    <XIcon className="size-4" />
                </Button>
            </header>

            <div className="flex flex-col gap-5 p-4">{children}</div>
        </dialog>
    );
}
