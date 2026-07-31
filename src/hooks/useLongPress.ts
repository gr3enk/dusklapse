import { useCallback, useRef } from "react";

/**
 * Fire after the pointer has been held still on an element for a while.
 *
 * For a deliberately undiscoverable action. Nothing announces it and nothing shows progress: a
 * hint would defeat the point, and after several seconds of holding nobody is there by accident.
 *
 * The click that follows a completed press is swallowed. Without that, letting go would also
 * activate whatever the element normally does - which on a submit button means firing off the
 * very request the press was meant to avoid.
 */
export function useLongPress(onLongPress: () => void, ms: number) {
    const timer = useRef(0);
    const fired = useRef(false);

    // Held in a ref so an inline callback does not rebuild the handlers on every render.
    const callback = useRef(onLongPress);
    callback.current = onLongPress;

    const cancel = useCallback(() => {
        window.clearTimeout(timer.current);
        timer.current = 0;
    }, []);

    const start = useCallback(() => {
        fired.current = false;
        cancel();
        timer.current = window.setTimeout(() => {
            fired.current = true;
            callback.current();
        }, ms);
    }, [cancel, ms]);

    return {
        onPointerDown: start,
        onPointerUp: cancel,
        // A pointer that wanders off the element, or is taken over by a scroll, ends the press.
        // Otherwise a press begun here would still fire while the finger was somewhere else.
        onPointerLeave: cancel,
        onPointerCancel: cancel,
        onClickCapture: (event: React.MouseEvent) => {
            if (!fired.current) return;
            fired.current = false;
            event.preventDefault();
            event.stopPropagation();
        },
    };
}
