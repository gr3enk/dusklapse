import { useCallback, useEffect, useRef, useState } from "react";

import { api, errorMessage } from "../lib/api";
import type { RampSettings } from "../lib/types";

export interface Ramp {
    /** `null` until the first load returns. */
    settings: RampSettings | null;
    error: string | null;
    /**
     * True while a write is in flight.
     *
     * Not a reason to disable a control: these writes reach a lock in the backend, not the camera,
     * so they finish inside a frame and gating on them only made the whole panel blink. Kept for a
     * caller that wants to show progress for something genuinely slow.
     */
    saving: boolean;
    /** Change one or more fields. Everything else is left as it is. */
    update: (patch: Partial<RampSettings>) => void;
    /** Point the reference at the frame currently on screen. */
    useCurrentFrame: () => void;
    /**
     * Take on settings the backend has already stored.
     *
     * For the few paths that write through a command of their own - the opening anchor is one -
     * so their result reaches the controls without a second round trip, and without writing back
     * a value that is already there.
     */
    adopt: (settings: RampSettings) => void;
}

/**
 * The ramp configuration, mirrored from Rust.
 *
 * A hook rather than more `useState` in the working screen, for the reason the screen is
 * about to grow: interval, keyframes and engine status are each their own concern, and
 * each one added inline would make that component harder to read while this one stays the
 * same size. Composition beats accumulation.
 *
 * Writes go through the backend and the answer it returns becomes the new local value.
 * Optimistic updates were the alternative and would have been worse: the backend is the
 * owner, so a value it clamped or rejected has to win, and reading its reply is what makes
 * that automatic.
 */
export function useRamp(): Ramp {
    const [settings, setSettings] = useState<RampSettings | null>(null);
    /**
     * Which write is the current one.
     *
     * Replies are adopted, so an older one arriving late would undo a newer change. This used to be
     * prevented by disabling every control while a write was in flight - but the write is a lock in
     * the backend rather than a round trip to the camera, so it completes within a frame and all
     * that produced was a flicker across the whole panel. Counting the writes handles the race
     * without touching the controls.
     */
    const writes = useRef(0);
    const [error, setError] = useState<string | null>(null);
    const [saving, setSaving] = useState(false);

    useEffect(() => {
        let current = true;
        api.rampSettings()
            .then((loaded) => {
                if (current) setSettings(loaded);
            })
            .catch((cause) => {
                if (current) setError(errorMessage(cause));
            });
        return () => {
            current = false;
        };
    }, []);

    const update = useCallback((patch: Partial<RampSettings>) => {
        setSettings((previous) => {
            // Nothing loaded yet means there is no complete configuration to send, and
            // sending a partial one would let the backend fill the gaps with defaults.
            if (!previous) return previous;

            const next = { ...previous, ...patch };
            const write = ++writes.current;
            setSaving(true);
            setError(null);
            api.rampConfigure(next)
                .then((stored) => {
                    // Superseded: a later write already went out, and its reply is the truth.
                    if (write === writes.current) setSettings(stored);
                })
                .catch((cause) => {
                    setError(errorMessage(cause));
                    // Put the stored value back on screen rather than leaving the UI
                    // showing a change that did not happen.
                    void api
                        .rampSettings()
                        .then((stored) => {
                            if (write === writes.current) setSettings(stored);
                        })
                        .catch(() => {});
                })
                .finally(() => {
                    if (write === writes.current) setSaving(false);
                });

            // Shown immediately; the reply above replaces it either way. Without this
            // a toggle would not move until the round trip completed, which feels
            // broken even when it is fast.
            return next;
        });
    }, []);

    const useCurrentFrame = useCallback(() => {
        const write = ++writes.current;
        setSaving(true);
        setError(null);
        api.rampReferenceFromLatestFrame()
            .then((stored) => {
                if (write !== writes.current) return;
                // Null means no frame has been analysed yet. The button is disabled in that
                // case, so this is a race rather than a mistake worth reporting.
                if (stored) setSettings(stored);
            })
            .catch((cause) => setError(errorMessage(cause)))
            .finally(() => {
                if (write === writes.current) setSaving(false);
            });
    }, []);

    const adopt = useCallback((stored: RampSettings) => setSettings(stored), []);

    return { settings, error, saving, update, useCurrentFrame, adopt };
}
