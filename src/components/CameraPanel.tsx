import { useCallback, useEffect, useState } from "react";

import { api, errorMessage } from "../lib/api";
import { DIALS, type BatteryStatus, type CameraInfo, type Dial, type ExposureCapabilities, type ExposureSettings } from "../lib/types";

interface Props {
    info: CameraInfo;
    onDisconnected: () => void;
}

/**
 * Manual control over the connected camera.
 *
 * This is not the timelapse UI - it exists to prove the whole chain end to end
 * (dial lists, writes, shutter release) against a real body before any ramping
 * logic is layered on top.
 */
export function CameraPanel({ info, onDisconnected }: Props) {
    const [capabilities, setCapabilities] = useState<ExposureCapabilities | null>(null);
    const [exposure, setExposure] = useState<ExposureSettings | null>(null);
    const [battery, setBattery] = useState<BatteryStatus | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);

    const refresh = useCallback(async () => {
        setError(null);
        try {
            // Capability lists depend on the shooting mode, so they get re-read
            // alongside the current values rather than cached once at connect.
            const [nextCapabilities, nextExposure, nextBattery] = await Promise.all([api.capabilities(), api.exposure(), api.battery()]);
            setCapabilities(nextCapabilities);
            setExposure(nextExposure);
            setBattery(nextBattery);
        } catch (cause) {
            setError(errorMessage(cause));
        }
    }, []);

    useEffect(() => {
        void refresh();
    }, [refresh]);

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
                        {info.firmware && ` · fw ${info.firmware}`}
                    </p>
                </div>
                <div className="panel__actions">
                    {battery && <span className="badge">{batteryLabel(battery)}</span>}
                    <button className="button" type="button" onClick={disconnect}>
                        Disconnect
                    </button>
                </div>
            </header>

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

            <p className="panel__meta">{totalStops === null ? "Brightness unavailable - one dial is on bulb or auto." : `Brightness ${totalStops > 0 ? "+" : ""}${totalStops.toFixed(2)} EV`}</p>

            <div className="panel__buttons">
                <button className="button button--primary" type="button" onClick={() => void shoot()} disabled={busy}>
                    {busy ? "Working…" : "Take a frame"}
                </button>
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
