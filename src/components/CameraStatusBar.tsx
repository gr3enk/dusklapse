import { DIALS, type BatteryStatus, type CameraInfo, type Dial, type ExposureCapabilities, type ExposureSettings } from "../lib/types";

interface Props {
    info: CameraInfo;
    capabilities: ExposureCapabilities | null;
    exposure: ExposureSettings | null;
    battery: BatteryStatus | null;
    frames: number;
    busy: boolean;
    onChangeDial: (dial: Dial, raw: string) => void;
    onDisconnect: () => void;
}

/**
 * What the camera is set to right now, in one strip.
 *
 * Deliberately dense and short: in landscape it sits under the preview with only
 * the height its content needs, so everything here has to read at a glance. The
 * dials stay editable rather than read-only - changing exposure is the whole point
 * of the app, and forcing a detour through another screen for it would be silly.
 */
export function CameraStatusBar({ info, capabilities, exposure, battery, frames, busy, onChangeDial, onDisconnect }: Props) {
    const totalStops = brightnessStops(exposure);

    return (
        <section className="status" aria-label="Camera status">
            <div className="status__dials">
                {DIALS.map(({ id, label }) => {
                    const values = capabilities?.[id] ?? [];
                    const current = exposure?.[id];
                    return (
                        <label className="dial" key={id}>
                            <span className="dial__label">{label}</span>
                            <select className="dial__select" value={current?.raw ?? ""} disabled={busy || values.length === 0} onChange={(event) => onChangeDial(id, event.currentTarget.value)}>
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
            </div>

            {/* One wrapper so these two wrap together. Left to themselves the readouts
                stay beside the dials and the identity drops alone onto a second line,
                which reads as a mistake rather than as a layout. */}
            <div className="status__meta">
                <div className="status__readouts">
                    <span className="status__ev" title="Total brightness of the current settings">
                        {totalStops === null ? "-- EV" : `${totalStops > 0 ? "+" : ""}${totalStops.toFixed(2)} EV`}
                    </span>
                    {info.pushesEvents && <span className="badge">{frames} frames</span>}
                    {battery && <span className="badge">{battery.percent === null ? battery.label : `${battery.percent}%`}</span>}
                </div>

                <div className="status__identity">
                    <span className="status__model" title={[info.manufacturer, info.firmware, info.serial && `#${info.serial}`].filter(Boolean).join(" · ")}>
                        {info.model}
                    </span>
                    <button className="button button--compact" type="button" onClick={onDisconnect}>
                        Disconnect
                    </button>
                </div>
            </div>
        </section>
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
