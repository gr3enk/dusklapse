import { useEffect, useState } from "react";

/**
 * Milliseconds since a moment, kept current.
 *
 * A ticking clock has to come from somewhere, and it should not be the render loop of whatever
 * happens to display it: this way one interval serves the readout and stops itself the moment
 * there is nothing to count from.
 *
 * `null` in, `null` out - there is no elapsed time before a start.
 */
export function useElapsed(since: number | null): number | null {
    const [now, setNow] = useState(() => Date.now());

    useEffect(() => {
        if (since === null) return;

        // Set once immediately so the first render after a start is not up to a second stale.
        setNow(Date.now());
        const timer = window.setInterval(() => setNow(Date.now()), 1000);
        return () => window.clearInterval(timer);
    }, [since]);

    return since === null ? null : Math.max(0, now - since);
}
