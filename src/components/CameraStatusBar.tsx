import { ImageIcon } from "lucide-react";
import { DIALS, type BatteryStatus, type CameraInfo, type Dial, type ExposureCapabilities, type ExposureSettings, type RampSettings } from "../lib/types";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import { Panel } from "./ui/Panel";
import { Select } from "./ui/Select";
import DynamicBatteryIcon from "./ui/DynamicBatteryIcon";
import { cn } from "../lib/utils";

interface Props {
    info: CameraInfo;
    capabilities: ExposureCapabilities | null;
    exposure: ExposureSettings | null;
    battery: BatteryStatus | null;
    frames: number;
    busy: boolean;
    /** The ramp configuration, for the per-dial markers. `null` until it has loaded. */
    ramp: RampSettings | null;
    /** The dial the ramp moved most recently, or `null` if it has not moved one. */
    lastRamped: Dial | null;
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
export function CameraStatusBar({ info, capabilities, exposure, battery, frames, busy, ramp, lastRamped, onChangeDial, onDisconnect }: Props) {
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
                            labelAdornment={<RampDot state={rampState(id, ramp, lastRamped)} />}
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
                    {info.pushesEvents && (
                        <Badge className="flex items-center gap-1">
                            {frames} <ImageIcon className="size-5" />
                        </Badge>
                    )}
                    {battery && (
                        <Badge className={cn("flex items-center gap-1", battery.percent && battery.percent <= 15 && "text-danger")}>
                            {battery.percent === null ? battery.label : `${battery.percent}%`} <DynamicBatteryIcon className="size-5" value={battery.percent ?? -1} />
                        </Badge>
                    )}
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

/** What the ramp is doing to one dial, as far as this strip is concerned. */
type RampState = "off" | "armed" | "justMoved";

/**
 * Which of the three states a dial is in.
 *
 * A disarmed ramp reads as off on every dial rather than as armed-but-idle: nothing is being
 * ramped, and a marker saying otherwise would be a promise the engine is not keeping.
 */
function rampState(dial: Dial, ramp: RampSettings | null, lastRamped: Dial | null): RampState {
    if (!ramp?.active || !ramp[dial].enabled) return "off";
    return lastRamped === dial ? "justMoved" : "armed";
}

const DOT_COLOURS: Record<RampState, string> = {
    off: "bg-border",
    armed: "bg-text",
    justMoved: "bg-alert-info",
};

const DOT_LABELS: Record<RampState, string> = {
    off: "ramping off",
    armed: "ramping on",
    justMoved: "ramped most recently",
};

/**
 * A dot next to a dial's caption saying whether the ramp may touch it, and which one it moved last.
 *
 * Carries its own text equivalent: the whole message here is a colour, which is exactly the kind
 * of thing that reaches nobody using a screen reader and nobody who cannot separate the three
 * shades. `role="img"` with a name is enough - there is nothing inside it to read.
 */
function RampDot({ state }: { state: RampState }) {
    return <span role="img" aria-label={DOT_LABELS[state]} title={DOT_LABELS[state]} className={cn("size-1.5 shrink-0 rounded-full", DOT_COLOURS[state])} />;
}
