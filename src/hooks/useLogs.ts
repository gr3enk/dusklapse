import { useSyncExternalStore } from "react";

import { snapshotLogs, subscribeToLogs, type LogEntry } from "../lib/logStore";

/**
 * The log buffer, as React state.
 *
 * `useSyncExternalStore` rather than an effect and `useState`: the store exists before any
 * component mounts and is written to from outside React, which is precisely the case this hook was
 * added to the language for. It also gets the initial value right, so a dialog opened after a
 * hundred lines have gone by shows all of them rather than starting empty.
 */
export function useLogs(): LogEntry[] {
    return useSyncExternalStore(subscribeToLogs, snapshotLogs);
}
