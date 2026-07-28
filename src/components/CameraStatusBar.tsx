import { DIALS, type BatteryStatus, type CameraInfo, type Dial, type ExposureCapabilities, type ExposureSettings } from "../lib/types";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import { Panel } from "./ui/Panel";
import { Select } from "./ui/Select";

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
 * Deliberately dense and short: in landscape it sits under the preview with only the
 * height its content needs, so everything here has to read at a glance. The dials stay
 * editable rather than read-only - changing exposure is the whole point of the app, and
 * a detour through another screen for it would be silly.
 */
export function CameraStatusBar({ info, capabilities, exposure, battery, frames, busy, onChangeDial, onDisconnect }: Props) {
    const totalStops = brightnessStops(exposure);

    return (
        <Panel className="flex flex-wrap items-end gap-x-4 gap-y-2 px-3 py-[0.6rem]" aria-label="Camera status">
            {/* Takes the width its content needs and no more, so what is left over goes to
                the meta group beside it. Given room to grow, the dials would claim all of
                it and force the meta group onto a line of its own. */}
            <div className="flex min-w-0 max-w-104 flex-auto gap-2">
                {DIALS.map(({ id, label }) => {
                    const values = capabilities?.[id] ?? [];
                    const current = exposure?.[id];
                    return (
                        <Select
                            key={id}
                            label={label}
                            value={current?.raw}
                            disabled={busy || values.length === 0}
                            onChange={(event) => onChangeDial(id, event.currentTarget.value)}
                            options={values.map((value) => ({ value: value.raw, label: value.label }))}
                        />
                    );
                })}
            </div>

            {/* One wrapper so these two wrap together. Left to themselves the readouts stay
                beside the dials and the identity drops alone onto a second line, which
                reads as a mistake rather than as a layout. */}
            <div className="flex min-w-0 flex-auto flex-wrap items-center justify-end gap-x-3 gap-y-2">
                <div className="flex min-w-0 items-center gap-2">
                    <span className="tabular-nums text-text-muted" title="Total brightness of the current settings">
                        {totalStops === null ? "-- EV" : `${totalStops > 0 ? "+" : ""}${totalStops.toFixed(2)} EV`}
                    </span>
                    {info.pushesEvents && <Badge>{frames} frames</Badge>}
                    {battery && <Badge>{battery.percent === null ? battery.label : `${battery.percent}%`}</Badge>}
                </div>

                <div className="flex min-w-0 items-center gap-2">
                    <span className="truncate font-semibold" title={[info.manufacturer, info.firmware, info.serial && `#${info.serial}`].filter(Boolean).join(" · ")}>
                        {info.model}
                    </span>
                    <Button size="compact" onClick={onDisconnect}>
                        Disconnect
                    </Button>
                </div>
            </div>
        </Panel>
    );
}

/**
 * Total brightness of the current settings, in stops.
 *
 * `null` when any dial has no fixed brightness - the same rule the Rust side applies.
 * Better to show nothing than a number that is quietly wrong.
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
