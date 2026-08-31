import { useEffect, useState } from "react";

import { CameraPanel } from "./components/CameraPanel";
import { ConnectScreen } from "./components/ConnectScreen";
import { api } from "./lib/api";
import type { CameraInfo } from "./lib/types";
import { cn } from "./lib/utils";
import "./App.css";

export default function App() {
    const [info, setInfo] = useState<CameraInfo | null>(null);
    const [restoring, setRestoring] = useState(true);
    // Held here rather than in the connect screen, which is torn down the moment a camera answers.
    // Kept for the run of the app and no longer: a development aid should not persist into a
    // session nobody asked it to.
    const [developerMode, setDeveloperMode] = useState(false);

    // The session lives in Rust, so a WebView reload - which happens constantly
    // during development - must not drop a camera that is still connected.
    useEffect(() => {
        api.status()
            .then(setInfo)
            .catch(() => setInfo(null))
            .finally(() => setRestoring(false));
    }, []);

    // The working screen owns the whole viewport and lays itself out; the connect and
    // loading screens are single centred cards. Two different jobs, so the container cannot
    // centre unconditionally.
    //
    // The definite height on the working screen is load-bearing, not decoration: with only a
    // minimum the container stays content-sized, the workspace's `fr` rows have nothing to
    // resolve against, and the ramping panel grows the page instead of scrolling inside
    // itself. `overflow-hidden` is the backstop - the panes each manage their own scrolling
    // and the page itself must never scroll.
    const filling = !restoring && info !== null;

    return (
        <main
            className={cn(
                "flex min-h-lvh flex-col",
                filling
                    ? cn(
                          "h-lvh max-h-lvh items-stretch justify-stretch overflow-hidden",
                          // Tighter on a phone, where an inch of margin is an inch the
                          // controls do not get. The safe-area insets are added on top of it
                          // either way, so the notch is still cleared.
                          "pt-[calc(var(--spacing-safe-t)+0.75rem)] pr-[calc(var(--spacing-safe-r)+0.75rem)] pb-[calc(var(--spacing-safe-b)+0.5rem)] pl-[calc(var(--spacing-safe-l)+0.75rem)]",
                          "sm:pt-[calc(var(--spacing-safe-t)+1.5rem)] sm:pr-[calc(var(--spacing-safe-r)+1.25rem)] sm:pl-[calc(var(--spacing-safe-l)+1.25rem)]",
                      )
                    : "items-center justify-center",
            )}
        >
            {restoring ? (
                <p className="text-text-muted">Checking for a connected camera…</p>
            ) : info ? (
                <CameraPanel info={info} onDisconnected={() => setInfo(null)} />
            ) : (
                <ConnectScreen
                    onConnected={setInfo}
                    developerMode={developerMode}
                    onUnlockDeveloper={() => setDeveloperMode(true)}
                />
            )}
        </main>
    );
}
