import { attachLogger, LogLevel } from "@tauri-apps/plugin-log";

/**
 * How many entries to keep.
 *
 * A connected camera produces a handful of lines a minute, so this is several hours. A cap rather
 * than none because the app is meant to be left running and an unbounded array on a tablet is
 * eventually a leak.
 */
const MAX_ENTRIES = 2000;

export interface LogEntry {
    /** Monotonic within a session; the key React needs and the order to display in. */
    id: number;
    /** When the line reached the WebView. */
    at: number;
    level: LogLevel;
    /**
     * The formatted line from Rust.
     *
     * On mobile that is `[target] message`; on desktop the plugin prepends its own date, time and
     * level as well. The time and level shown beside this come from here, not from the text, so
     * the view reads the same on both - at the cost of some repetition in a desktop build.
     */
    message: string;
}

/**
 * Every log line since the app started.
 *
 * A module-level buffer rather than component state, for two reasons. It starts collecting at
 * import time, so the lines from connecting are already there before any screen that could display
 * them exists; and it survives the working screen being torn down, which is exactly when the
 * interesting lines appear - a lost connection takes that screen with it.
 */
let entries: LogEntry[] = [];
let nextId = 1;

const listeners = new Set<(entries: LogEntry[]) => void>();

function publish() {
    for (const listener of listeners) listener(entries);
}

/** Subscribe to the buffer. Returns the unsubscribe function. */
export function subscribeToLogs(listener: (entries: LogEntry[]) => void): () => void {
    listeners.add(listener);
    listener(entries);
    return () => listeners.delete(listener);
}

export function snapshotLogs(): LogEntry[] {
    return entries;
}

export function clearLogs() {
    entries = [];
    publish();
}

// Attached once, at import. `attachLogger` resolves to an unlisten function that is deliberately
// dropped: this subscription is meant to last as long as the app does.
//
// Guarded because this module is imported by code that also runs outside a WebView - a test, or a
// browser opened against the dev server - where the plugin has nothing to talk to. Losing the log
// there is not worth failing the import over.
void attachLogger(({ level, message }) => {
    // A new array rather than a push: subscribers compare by identity to decide whether to render.
    entries = [...entries, { id: nextId++, at: Date.now(), level, message }];
    if (entries.length > MAX_ENTRIES) entries = entries.slice(entries.length - MAX_ENTRIES);
    publish();
}).catch(() => {
    // No Tauri backend to listen to. The view will simply stay empty.
});
