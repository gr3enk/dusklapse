import { useEffect, useRef, useState } from "react";

import { api, errorMessage } from "../lib/api";
import type { PreviewInfo, RampOutcome } from "../lib/types";

export interface AutoRamp {
    /** What the ramp did about the most recent frame, or `null` if it has done nothing yet. */
    outcome: RampOutcome | null;
    error: string | null;
}

/**
 * Corrects the exposure once per measured frame.
 *
 * Keyed on the frame object rather than on a counter, because the backend can only correct a
 * frame it has already analysed and cached - and that is exactly the moment this identity
 * changes. Firing on the frame *count* instead would race the fetch and read the previous
 * frame's brightness.
 *
 * A separate hook rather than more lines inside the working screen: it is its own concern with
 * its own trigger, and the interval and keyframe controls still to come will each want the
 * same treatment.
 */
export function useAutoRamp(frame: PreviewInfo | null, active: boolean, onApplied?: () => void): AutoRamp {
    const [outcome, setOutcome] = useState<RampOutcome | null>(null);
    const [error, setError] = useState<string | null>(null);
    // Held in a ref so a caller passing an inline function does not re-run the effect and
    // correct the same frame twice.
    const applied = useRef(onApplied);
    applied.current = onApplied;
    // Guards against a second pass over the same frame, which a re-render would otherwise
    // cause - and a duplicate correction would double the exposure change.
    const corrected = useRef<PreviewInfo | null>(null);

    useEffect(() => {
        if (!active || !frame?.analysis) return;
        if (corrected.current === frame) return;
        corrected.current = frame;

        let current = true;
        api.rampApply()
            .then((next) => {
                // Null means there was nothing to decide. Keeping the previous outcome on
                // screen is better than blanking it on every frame that needed no change.
                if (!current || !next) return;
                setOutcome(next);

                // The camera has just been written to from the Rust side, so nothing else knows
                // its dials moved. Without this the status bar keeps showing the old aperture
                // until the next poll - twenty seconds on a body with a push channel.
                if (next.change?.applied) applied.current?.();
            })
            .catch((cause) => {
                if (current) setError(errorMessage(cause));
            });

        return () => {
            current = false;
        };
    }, [frame, active]);

    return { outcome, error };
}
