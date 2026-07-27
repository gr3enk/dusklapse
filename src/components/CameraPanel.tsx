import { useCallback, useEffect, useRef, useState } from "react";

import { api, errorMessage } from "../lib/api";
import type { BatteryStatus, CameraInfo, Dial, ExposureCapabilities, ExposureSettings } from "../lib/types";
import { CameraStatusBar } from "./CameraStatusBar";
import { PreviewPane } from "./PreviewPane";
import { RampingPanel } from "./RampingPanel";

/**
 * How often to re-read a camera that has to be asked.
 *
 * Canon has no push channel, so this is the only way its display stays honest
 * when someone turns a ring on the body.
 */
const POLL_INTERVAL_MS = 2000;

/**
 * How often to re-read a camera that announces its own changes.
 *
 * Not for freshness - events cover that, instantly, instead of the up-to-two
 * seconds of lag polling gave us. This is purely so a camera that vanished
 * (switched off, out of range) gets noticed within a reasonable time rather than
 * whenever someone next touches a control.
 */
const HEARTBEAT_INTERVAL_MS = 20000;

interface Props {
    info: CameraInfo;
    onDisconnected: () => void;
}

/**
 * Owns the camera session state and arranges the three working panes.
 *
 * The arrangement itself is CSS, not JavaScript: `.workspace` is a grid whose named
 * areas are reshuffled by an `(orientation: landscape)` media query. Rotating the
 * device therefore relayouts without a re-render and without a resize listener to
 * get wrong - and none of the three components below has to know which orientation
 * it is in.
 */
export function CameraPanel({ info, onDisconnected }: Props) {
    const [capabilities, setCapabilities] = useState<ExposureCapabilities | null>(null);
    const [exposure, setExposure] = useState<ExposureSettings | null>(null);
    const [battery, setBattery] = useState<BatteryStatus | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);
    // Counted from CaptureComplete, so one per exposure rather than one per file.
    const [frames, setFrames] = useState(0);

    // Read by the poll timer, which must not restart every time `busy` flips.
    const busyRef = useRef(false);
    busyRef.current = busy;

    const readAll = useCallback(async () => {
        // Capability lists depend on the shooting mode and the attached lens, so
        // they get re-read rather than cached once at connect.
        const [nextCapabilities, nextExposure, nextBattery] = await Promise.all([api.capabilities(), api.exposure(), api.battery()]);
        setCapabilities(nextCapabilities);
        setExposure(nextExposure);
        setBattery(nextBattery);
    }, []);

    const refresh = useCallback(async () => {
        setError(null);
        try {
            await readAll();
        } catch (cause) {
            setError(errorMessage(cause));
        }
    }, [readAll]);

    useEffect(() => {
        void refresh();
    }, [refresh]);

    useEffect(() => {
        const every = info.pushesEvents ? HEARTBEAT_INTERVAL_MS : POLL_INTERVAL_MS;
        const timer = setInterval(() => {
            // A write is already in flight; queueing reads behind it would only
            // make the UI feel sluggish.
            if (busyRef.current) return;
            void readAll().catch((cause) => setError(errorMessage(cause)));
        }, every);
        return () => clearInterval(timer);
    }, [readAll, info.pushesEvents]);

    // React to what the camera volunteers. Only the events that change something
    // reach us; the Rust side already dropped the focus chatter.
    useEffect(() => {
        const unlisten = api.onCameraEvent((event) => {
            switch (event.kind) {
                case "dialChanged":
                    if (busyRef.current) return;
                    void readAll().catch((cause) => setError(errorMessage(cause)));
                    break;
                case "frameRecorded":
                    setFrames((count) => count + 1);
                    break;
            }
        });
        // `listen` resolves once registered; dropping the promise would leak the
        // handler across a remount.
        return () => void unlisten.then((stop) => stop());
    }, [readAll]);

    async function changeDial(dial: Dial, raw: string) {
        setBusy(true);
        setError(null);
        try {
            await api.setExposure(dial, raw);
            // Read back rather than assuming the write took: cameras silently clamp
            // to a neighbouring value more often than you would like.
            setExposure(await api.exposure());
        } catch (cause) {
            setError(errorMessage(cause));
        } finally {
            setBusy(false);
        }
    }

    async function shoot() {
        setBusy(true);
        setError(null);
        try {
            await api.shoot(false);
            setBattery(await api.battery());
        } catch (cause) {
            setError(errorMessage(cause));
        } finally {
            setBusy(false);
        }
    }

    async function disconnect() {
        try {
            await api.disconnect();
        } catch (cause) {
            // Losing the camera is exactly when disconnect fails, and the user still
            // wants to get back to the connect screen.
            console.warn("disconnect failed", cause);
        }
        onDisconnected();
    }

    return (
        <div className="workspace">
            <div className="workspace__preview">
                <PreviewPane frame={frames} supported={info.pushesEvents} />
            </div>

            <div className="workspace__status">
                <CameraStatusBar info={info} capabilities={capabilities} exposure={exposure} battery={battery} frames={frames} busy={busy} onChangeDial={changeDial} onDisconnect={disconnect} />
                {error && (
                    <p className="notice notice--error" role="alert">
                        {error}
                    </p>
                )}
            </div>

            <div className="workspace__ramping">
                <RampingPanel info={info} busy={busy} onShoot={() => void shoot()} onRefresh={() => void refresh()} />
            </div>
        </div>
    );
}
