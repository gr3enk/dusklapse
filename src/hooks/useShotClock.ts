import { useCallback, useState } from "react";

/**
 * Timestamps kept for measuring the interval.
 *
 * Only the most recent handful are ever read, so there is no reason to hold thousands. The first
 * one is kept separately because the running time needs it for as long as the session lasts.
 */
const RECENT = 16;

export interface ShotClock {
    /** Every shot the camera has reported, transferred or not. */
    count: number;
    /** When the first shot landed, or `null` before there was one. */
    firstAt: number | null;
    /** The most recent timestamps, oldest first. For measuring the interval. */
    recent: number[];
    /** Record a shot. Call once per frame the camera reports. */
    record: () => void;
}

/**
 * Counts and times every shot, whether or not its image was fetched.
 *
 * Separate from the frame history on purpose. With transfers thinned to one in n, the history has
 * a sample only for the frames that crossed the network - which is right for the luminance reading
 * and for the ramp, and wrong for the two readouts that answer "is the intervalometer doing what I
 * set it to". Measuring the interval from transfers would report n times the real one.
 */
export function useShotClock(): ShotClock {
    const [state, setState] = useState<{ count: number; firstAt: number | null; recent: number[] }>({
        count: 0,
        firstAt: null,
        recent: [],
    });

    const record = useCallback(() => {
        setState((previous) => {
            const at = Date.now();
            const recent = [...previous.recent, at];
            return {
                count: previous.count + 1,
                firstAt: previous.firstAt ?? at,
                recent: recent.length > RECENT ? recent.slice(recent.length - RECENT) : recent,
            };
        });
    }, []);

    return { ...state, record };
}
