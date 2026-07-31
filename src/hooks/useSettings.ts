import { useCallback, useEffect, useRef, useState } from "react";

import { api, errorMessage } from "../lib/api";
import type { AppSettings } from "../lib/types";

export interface Settings {
    /** `null` until the first load returns. */
    value: AppSettings | null;
    error: string | null;
    saving: boolean;
    /** Change one or more fields. Everything else is left as it is. */
    update: (patch: Partial<AppSettings>) => void;
}

/**
 * The secondary settings, mirrored from Rust.
 *
 * Write-through rather than optimistic-only, for the same reason as `useRamp`: the backend owns
 * these and clamps them, so the value it replies with has to win. Reading its answer is what makes
 * that automatic instead of a rule someone has to remember.
 */
export function useSettings(): Settings {
    const [value, setValue] = useState<AppSettings | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [saving, setSaving] = useState(false);
    // See `useRamp`: replies are adopted, so an older one arriving late must not undo a newer
    // change. Counted rather than prevented by disabling the controls.
    const writes = useRef(0);

    useEffect(() => {
        let current = true;
        api.settings()
            .then((loaded) => {
                if (current) setValue(loaded);
            })
            .catch((cause) => {
                if (current) setError(errorMessage(cause));
            });
        return () => {
            current = false;
        };
    }, []);

    const update = useCallback((patch: Partial<AppSettings>) => {
        setValue((previous) => {
            // Nothing loaded yet means there is no complete set to send, and a partial one would
            // let the backend fill the gaps with defaults.
            if (!previous) return previous;

            const next = { ...previous, ...patch };
            const write = ++writes.current;
            setSaving(true);
            setError(null);
            api.setSettings(next)
                .then((stored) => {
                    if (write === writes.current) setValue(stored);
                })
                .catch((cause) => {
                    setError(errorMessage(cause));
                    // Put the stored value back on screen rather than leaving a change that did
                    // not happen.
                    void api
                        .settings()
                        .then(setValue)
                        .catch(() => {});
                })
                .finally(() => {
                    if (write === writes.current) setSaving(false);
                });

            // Shown immediately; the reply above replaces it either way.
            return next;
        });
    }, []);

    return { value, error, saving, update };
}
