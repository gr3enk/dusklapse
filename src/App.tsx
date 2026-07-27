import { useEffect, useState } from "react";

import { CameraPanel } from "./components/CameraPanel";
import { ConnectScreen } from "./components/ConnectScreen";
import { api } from "./lib/api";
import type { CameraInfo } from "./lib/types";
import "./App.css";

export default function App() {
    const [info, setInfo] = useState<CameraInfo | null>(null);
    const [restoring, setRestoring] = useState(true);

    // The session lives in Rust, so a WebView reload - which happens constantly
    // during development - must not drop a camera that is still connected.
    useEffect(() => {
        api.status()
            .then(setInfo)
            .catch(() => setInfo(null))
            .finally(() => setRestoring(false));
    }, []);

    // The working screen fills the viewport and lays itself out; the connect and
    // loading screens are single centred cards. Two different jobs, so the container
    // cannot centre unconditionally.
    const filling = !restoring && info !== null;

    return (
        <main className={filling ? "app app--filling" : "app"}>
            {restoring ? (
                <p className="app__loading">Checking for a connected camera…</p>
            ) : info ? (
                <CameraPanel info={info} onDisconnected={() => setInfo(null)} />
            ) : (
                <ConnectScreen onConnected={setInfo} />
            )}
        </main>
    );
}
