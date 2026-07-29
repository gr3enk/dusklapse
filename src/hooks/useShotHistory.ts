import { useEffect, useRef, useState } from "react";

import type { ExposureSettings, PreviewInfo, RampSettings, SkyState } from "../lib/types";

/**
 * How many shots to keep.
 *
 * A sunset at a seven-second interval produces about five hundred frames an hour, so this is
 * several sessions' worth. A cap rather than none because the app is meant to be left running
 * and an unbounded array on a tablet eventually is a leak.
 */
const MAX_SHOTS = 2000;

/** One frame, and what the ramp was doing when it was taken. */
export interface Shot {
    /** 1-based shot number. The x axis. */
    index: number;
    /** Stops of each dial, or `null` on bulb or auto where there is no stop position. */
    shutter: number | null;
    aperture: number | null;
    iso: number | null;
    /** Measured brightness of the frame. */
    luminance: number;
    /** The stored reference at the time. */
    reference: number;
    /** The reference after the daylight curve, equal to `reference` when the curve is off. */
    effectiveReference: number;
}

/**
 * Every frame so far, for the history charts.
 *
 * The exposure recorded is the one the frame was *taken* with, not the one the ramp moved to
 * afterwards. That is the honest pairing: this shutter and this ISO produced this brightness.
 * It works out that way because the ramp's correction is a round trip that lands after this
 * effect has already read the current values.
 *
 * Frontend-only and deliberately so. The backend keeps no history because nothing in the ramp
 * decision needs one - each frame is judged against the reference on its own - and inventing a
 * store there would mean a second source of truth for something only a chart reads.
 */
export function useShotHistory(frame: PreviewInfo | null, exposure: ExposureSettings | null, ramp: RampSettings | null, sky: SkyState | null): Shot[] {
    const [shots, setShots] = useState<Shot[]>([]);

    // Read at the moment a frame arrives rather than depended upon: a change of exposure or
    // reference is not itself a new shot, and listing them as dependencies would append one.
    const latest = useRef({ exposure, ramp, sky });
    latest.current = { exposure, ramp, sky };

    // Same guard as the ramp uses: a re-render must not record the same frame twice.
    const recorded = useRef<PreviewInfo | null>(null);

    useEffect(() => {
        if (!frame?.analysis) return;
        if (recorded.current === frame) return;
        recorded.current = frame;

        const { exposure: current, ramp: settings, sky: state } = latest.current;
        const reference = settings?.reference.value ?? 0;

        setShots((previous) => {
            const shot: Shot = {
                index: previous.length + 1,
                shutter: current?.shutter?.stops ?? null,
                aperture: current?.aperture?.stops ?? null,
                iso: current?.iso?.stops ?? null,
                luminance: frame.analysis!.luminance.value,
                reference,
                // Falls back to the stored reference so the third curve is always drawable; with
                // the curve off it simply sits on top of the second one, which is the truth.
                effectiveReference: state?.effectiveReference.value ?? reference,
            };

            const next = [...previous, shot];
            return next.length > MAX_SHOTS ? next.slice(next.length - MAX_SHOTS) : next;
        });
    }, [frame]);

    return shots;
}
