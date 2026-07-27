import { useCallback, useEffect, useRef, useState } from "react";

import { api, errorMessage } from "../lib/api";
import { PreviewPane } from "./PreviewPane";
import { DIALS, type BatteryStatus, type CameraInfo, type Dial, type ExposureCapabilities, type ExposureSettings } from "../lib/types";

interface Props {
    info: CameraInfo;
    onDisconnected: () => void;
}

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

/**
 * Manual control over the connected camera.
 *
 * This is not the timelapse UI - it exists to prove the whole chain end to end
 * (dial lists, writes) against a real body before any ramping logic is layered
 * on top.
 */
export function CameraPanel({ info, onDisconnected }: Props) {
    const [capabilities, setCapabilities] = useState<ExposureCapabilities | null>(null);
    const [exposure, setExposure] = useState<ExposureSettings | null>(null);
    const [battery, setBattery] = useState<BatteryStatus | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);
    const [lastRead, setLastRead] = useState<Date | null>(null);
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
        setLastRead(new Date());
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

    const totalStops = brightnessStops(exposure);

    return (
        <div className="panel">
            <header className="panel__header">
                <div>
                    <h1>{info.model}</h1>
                    <p className="panel__subtitle">
                        {info.manufacturer}
                        {info.apiVersion && ` · ${info.apiVersion}`}
                        {info.firmware && ` · ${info.firmware}`}
                        {info.serial && ` · #${info.serial}`}
                    </p>
                </div>
                <div className="panel__actions">
                    {info.pushesEvents && <span className="badge">{frames} frames</span>}
                    {battery && <span className="badge">{batteryLabel(battery)}</span>}
                    <button className="button" type="button" onClick={disconnect}>
                        Disconnect
                    </button>
                </div>
            </header>

            {info.pushesEvents && <PreviewPane frame={frames} />}

            <section className="dials">
                {DIALS.map(({ id, label }) => {
                    const values = capabilities?.[id] ?? [];
                    const current = exposure?.[id];
                    return (
                        <label className="dial" key={id}>
                            <span className="dial__label">{label}</span>
                            <select className="dial__select" value={current?.raw ?? ""} disabled={busy || values.length === 0} onChange={(event) => void changeDial(id, event.currentTarget.value)}>
                                {/* A dial we cannot read still needs a stable option to show. */}
                                {!current && <option value="">-</option>}
                                {values.map((value) => (
                                    <option key={value.raw} value={value.raw}>
                                        {value.label}
                                    </option>
                                ))}
                            </select>
                        </label>
                    );
                })}
            </section>

            <p className="panel__meta">
                {totalStops === null ? "Brightness unavailable - one dial is on bulb or auto." : `Brightness ${totalStops > 0 ? "+" : ""}${totalStops.toFixed(2)} EV`}
                {lastRead && <span className="panel__pulse"> · read {lastRead.toLocaleTimeString()}</span>}
            </p>

            <div className="panel__buttons">
                {info.supportsRelease ? (
                    <button className="button button--primary" type="button" onClick={() => void shoot()} disabled={busy}>
                        {busy ? "Working…" : "Take a frame"}
                    </button>
                ) : (
                    // Offering a button that is guaranteed to fail is worse than
                    // explaining why there is none.
                    <p className="notice notice--info">This body takes no remote release over Wi-Fi. Frame timing comes from your intervalometer; Dusklapse ramps the exposure between frames.</p>
                )}
                <button className="button" type="button" onClick={() => void refresh()} disabled={busy}>
                    Re-read camera
                </button>
            </div>

            {error && (
                <p className="notice notice--error" role="alert">
                    {error}
                </p>
            )}
        </div>
    );
}

/**
 * Total brightness of the current settings, in stops.
 *
 * `null` when any dial has no fixed brightness - the same rule the Rust side
 * applies. Better to show nothing than a number that is quietly wrong.
 */
function brightnessStops(exposure: ExposureSettings | null): number | null {
    if (!exposure) return null;
    const parts = [exposure.shutter, exposure.aperture, exposure.iso];
    let total = 0;
    for (const part of parts) {
        if (part?.stops == null) return null;
        total += part.stops;
    }
    return total;
}

function batteryLabel(battery: BatteryStatus): string {
    return battery.percent === null ? battery.label : `${battery.percent}%`;
}
