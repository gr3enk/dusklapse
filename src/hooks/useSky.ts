import { useCallback, useEffect, useState } from "react";

import { api, errorMessage } from "../lib/api";
import type { Location, RampSettings, SkyState } from "../lib/types";

/** How often to re-read the sky. */
const POLL_MS = 30_000;

/**
 * How long to wait for a position before giving up.
 *
 * Enforced here because the plugin's own `timeout` option is documented as ignored on iOS, and a
 * simulator with no location set never answers at all - leaving the button stuck on "Locating…"
 * with nothing to explain it.
 */
const LOCATE_TIMEOUT_MS = 15_000;

export interface Sky {
    /** Where the sun is, or `null` when the curve is off or has no position. */
    state: SkyState | null;
    /** True while a position request is in flight. */
    locating: boolean;
    /** Whether this build can ask the device where it is at all. */
    canLocate: boolean;
    error: string | null;
    /** Ask the device for its position. Resolves to `null` when it could not be had. */
    locate: () => Promise<Location | null>;
}

/**
 * The sun's position, kept current.
 *
 * Polled rather than pushed because it is the one value here that changes without anyone
 * touching anything - and slowly. Half a minute is well inside the resolution of the readout:
 * even at the fastest, twilight moves the daylight percentage by about one point a minute.
 *
 * `settings` is a dependency and not just a trigger: the effective reference depends on the
 * stored reference and the factor, so a fresh answer is needed the moment either moves, not
 * up to thirty seconds later.
 */
export function useSky(settings: RampSettings | null): Sky {
    const [state, setState] = useState<SkyState | null>(null);
    const [locating, setLocating] = useState(false);
    const [canLocate, setCanLocate] = useState(false);
    const [error, setError] = useState<string | null>(null);

    // A fact about the build, so it is asked once and never changes.
    useEffect(() => {
        let current = true;
        api.hasGeolocation()
            .then((supported) => {
                if (current) setCanLocate(supported);
            })
            .catch(() => {
                // Not knowing means falling back to the typed coordinates, which always work.
                if (current) setCanLocate(false);
            });
        return () => {
            current = false;
        };
    }, []);

    const enabled = settings?.daylight.enabled ?? false;
    const location = settings?.daylight.location ?? null;
    const factor = settings?.daylight.factor;
    const shape = settings?.daylight.shape;
    const twilight = settings?.daylight.twilight;
    const reference = settings?.reference.value;
    // The mode too: the shapes are applied to progress through the event, which counts from full
    // day at sunset and full night at sunrise, so switching direction changes the answer.
    const mode = settings?.mode;

    useEffect(() => {
        // Nothing to poll for, and clearing is what makes the readout disappear the moment the
        // curve is switched off rather than freezing on its last value.
        if (!enabled || !location) {
            setState(null);
            return;
        }

        let current = true;
        const read = () => {
            api.rampSky()
                .then((sky) => {
                    if (current) {
                        setState(sky);
                        setError(null);
                    }
                })
                .catch((cause) => {
                    if (current) setError(errorMessage(cause));
                });
        };

        read();
        const timer = window.setInterval(read, POLL_MS);
        return () => {
            current = false;
            window.clearInterval(timer);
        };
        // Latitude and longitude rather than the object: a new object with the same coordinates
        // would otherwise restart the interval on every settings write.
    }, [enabled, location, location?.latitude, location?.longitude, factor, shape, twilight, mode, reference]);

    const locate = useCallback(async (): Promise<Location | null> => {
        setLocating(true);
        setError(null);
        try {
            // Imported here rather than at the top of the file because the plugin does not
            // exist in a desktop build, and a static import would fail the module load for
            // everyone instead of only the caller who needed it.
            const geolocation = await import("@tauri-apps/plugin-geolocation");

            let permission = await geolocation.checkPermissions();
            if (permission.location === "prompt" || permission.location === "prompt-with-rationale") {
                permission = await geolocation.requestPermissions(["location"]);
            }
            if (permission.location !== "granted") {
                setError("Location access was declined. You can still type the coordinates in.");
                return null;
            }

            const position = await withTimeout(geolocation.getCurrentPosition());
            if (!position) {
                setError("The device did not report a position. In the simulator, set one under Features › Location.");
                return null;
            }

            return {
                latitude: position.coords.latitude,
                longitude: position.coords.longitude,
            };
        } catch (cause) {
            setError(errorMessage(cause));
            return null;
        } finally {
            setLocating(false);
        }
    }, []);

    return { state, locating, canLocate, error, locate };
}

/**
 * Resolve to `null` rather than waiting forever.
 *
 * The pending request is not cancelled - there is no way to cancel it - it is simply no longer
 * waited on, so a late answer is discarded instead of arriving after the UI has moved on.
 */
async function withTimeout<T>(work: Promise<T>): Promise<T | null> {
    let timer = 0;
    const expiry = new Promise<null>((resolve) => {
        timer = window.setTimeout(() => resolve(null), LOCATE_TIMEOUT_MS);
    });
    try {
        return await Promise.race([work, expiry]);
    } finally {
        window.clearTimeout(timer);
    }
}
