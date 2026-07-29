import { ClockIcon, ImageIcon, RotateCwFadingClockIcon, SettingsIcon } from "lucide-react";

import { useElapsed } from "../hooks/useElapsed";
import type { ShotClock } from "../hooks/useShotClock";
import { measuredInterval } from "../lib/interval";
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
    busy: boolean;
    /** The ramp configuration, for the per-dial markers. `null` until it has loaded. */
    ramp: RampSettings | null;
    /** The dial the ramp moved most recently, or `null` if it has not moved one. */
    lastRamped: Dial | null;
    /**
     * Every shot the camera has reported, for the count, the interval and the running time.
     *
     * Deliberately not the transferred-frame history: with transfers thinned to one in n, that
     * would report n times the real interval and undercount the sequence.
     */
    clock: ShotClock;
    onChangeDial: (dial: Dial, raw: string) => void;
    onOpenSettings: () => void;
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
export function CameraStatusBar({ info, capabilities, exposure, battery, busy, ramp, lastRamped, clock, onChangeDial, onOpenSettings, onDisconnect }: Props) {
    // Measured, never configured: the intervalometer owns the timing and this app only watches it.
    // A number typed in here could disagree with what the camera is actually doing, and then it
    // would be worse than no number at all.
    const interval = measuredInterval(clock.recent);
    const running = useElapsed(clock.firstAt);

    return (
        <Panel style={{ gridTemplateColumns: "1fr 1fr", gridTemplateRows: "1fr auto" }} className="grid gap-x-4 gap-y-2 px-3 py-[0.6rem]" aria-label="Camera status">
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
                    <span className="truncate font-semibold" title={[info.manufacturer, info.firmware, info.serial && `#${info.serial}`].filter(Boolean).join(" · ")}>
                        {info.model}
                    </span>
                    <Button size="compact" onClick={onOpenSettings} aria-label="Settings" title="Settings">
                        <SettingsIcon className="size-4" />
                    </Button>
                    <Button size="compact" onClick={onDisconnect}>
                        Disconnect
                    </Button>
                </div>
            </div>
            <div className="flex w-full justify-end min-w-0 items-center gap-2 col-span-2">
                {running !== null && (
                    <Badge className="flex items-center gap-1" title="Time since the first frame">
                        {formatDuration(running)} <ClockIcon className="size-5" />
                    </Badge>
                )}
                {info.pushesEvents && (
                    <Badge className="flex items-center gap-1">
                        {clock.count} <ImageIcon className="size-5" />
                    </Badge>
                )}
                {interval !== null && (
                    <Badge className="flex items-center gap-1" title="Interval measured between the last frames">
                        {formatInterval(interval)} <RotateCwFadingClockIcon className="size-5" />
                    </Badge>
                )}
                {battery && (
                    <Badge className={cn("flex items-center gap-1", battery.percent && battery.percent <= 15 && "text-danger")}>
                        {battery.percent === null ? battery.label : `${battery.percent}%`} <DynamicBatteryIcon className="size-5" value={battery.percent ?? -1} />
                    </Badge>
                )}
            </div>
        </Panel>
    );
}

/** A tenth of a second below ten, whole seconds above - past that the decimal is noise. */
function formatInterval(milliseconds: number): string {
    if (isNaN(milliseconds)) return "-";
    const seconds = milliseconds / 1000;
    return seconds < 10 ? `${seconds.toFixed(1)}s` : `${Math.round(seconds)}s`;
}

/** `m:ss` under an hour, `h:mm:ss` above. */
function formatDuration(milliseconds: number): string {
    const total = Math.floor(milliseconds / 1000);
    const hours = Math.floor(total / 3600);
    const minutes = Math.floor((total % 3600) / 60);
    const seconds = total % 60;

    const pad = (value: number) => value.toString().padStart(2, "0");
    return hours > 0 ? `${hours}:${pad(minutes)}:${pad(seconds)}` : `${minutes}:${pad(seconds)}`;
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
