import { useEffect, useRef, useState } from "react";

import { api, errorMessage } from "../lib/api";
import type { PreviewInfo, RampSettings } from "../lib/types";

export interface PrimeReference {
    /**
     * False until the opening anchor has been dealt with, one way or the other.
     *
     * The ramp must not run before this settles: on the very first frame the stored reference is
     * still the default, and a correction against a number nobody chose would move the camera for
     * no reason. One frame of delay costs nothing - the reference it would have used is the one
     * being set right here.
     */
    settled: boolean;
    error: string | null;
}

/**
 * Anchors the luminance reference on the first frame of a session.
 *
 * A reference left at its default is a placeholder, not a decision, and starting from it means the
 * first correction chases an arbitrary target. The first measured frame is a far better opening
 * guess, and it costs nothing to take.
 *
 * The backend refuses if the reference has already been aimed, so this can run on every connect
 * without thinking about it - reconnecting to a sequence under way leaves its reference alone.
 * See `RampState::prime_reference`.
 */
export function usePrimeReference(frame: PreviewInfo | null, onPrimed: (settings: RampSettings) => void): PrimeReference {
    const [settled, setSettled] = useState(false);
    const [error, setError] = useState<string | null>(null);

    // Held in a ref so an inline callback does not re-run the effect and anchor twice.
    const primed = useRef(onPrimed);
    primed.current = onPrimed;

    // Once per mount, which is once per session: connecting builds this screen anew, while the
    // reconnect button leaves it standing.
    const attempted = useRef(false);

    useEffect(() => {
        if (attempted.current || !frame?.analysis) return;
        attempted.current = true;

        let current = true;
        api.rampPrimeReference()
            .then((stored) => {
                // Null means the reference was already aimed, which is a decision, not a failure.
                if (current && stored) primed.current(stored);
            })
            .catch((cause) => {
                if (current) setError(errorMessage(cause));
            })
            .finally(() => {
                if (current) setSettled(true);
            });

        return () => {
            current = false;
        };
    }, [frame]);

    return { settled, error };
}
