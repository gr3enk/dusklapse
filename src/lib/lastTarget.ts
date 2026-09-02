import type { Vendor } from "./types";

/**
 * The camera the app was last connected to, remembered on the device.
 *
 * Only what the connect screen needs to offer the same camera again: which vendor, at which
 * address. Nothing here is read by a decision - it is a starting point for three form fields, and
 * losing it costs a few taps rather than a sequence.
 *
 * That is why it lives in the WebView's own storage rather than in Rust with the ramp and the app
 * settings. Those are owned by Rust because a reload must not undo a choice made an hour into a
 * shoot; this one is the opposite kind of state. Keeping it here needs no plugin, no capability and
 * no file, and it survives the app being closed, which the Rust-side settings currently do not.
 */
export interface LastTarget {
    vendor: Vendor;
    host: string;
    port: string;
}

const KEY = "dusklapse.lastTarget";

/**
 * The last camera, or `null` when there is none to offer.
 *
 * Every path returns `null` rather than throwing. Storage is unavailable in a private window,
 * throws outright in some embedded contexts, and can come back holding whatever an older version
 * of this app wrote - none of which is a reason to fail to draw a connect screen.
 */
export function loadLastTarget(storage: Storage | undefined = safeStorage()): LastTarget | null {
    try {
        const stored = storage?.getItem(KEY);
        if (!stored) return null;

        const parsed: unknown = JSON.parse(stored);
        return isLastTarget(parsed) ? parsed : null;
    } catch {
        return null;
    }
}

/** Remember a camera that was actually reached. */
export function saveLastTarget(target: LastTarget, storage: Storage | undefined = safeStorage()) {
    try {
        storage?.setItem(KEY, JSON.stringify(target));
    } catch {
        // A device that will not store this still connects perfectly well; it just does not
        // remember. Nothing to tell anyone about.
    }
}

/**
 * Checked rather than trusted, because this crosses app versions.
 *
 * A vendor that no longer exists, or a field that changed shape, would otherwise put the connect
 * screen into a state the rest of the app cannot describe.
 */
function isLastTarget(value: unknown): value is LastTarget {
    if (typeof value !== "object" || value === null) return false;
    const candidate = value as Record<string, unknown>;
    return (
        typeof candidate.vendor === "string" && typeof candidate.host === "string" && typeof candidate.port === "string"
    );
}

/** `localStorage` where there is one. Reading the property itself throws in some contexts. */
function safeStorage(): Storage | undefined {
    try {
        return globalThis.localStorage;
    } catch {
        return undefined;
    }
}
