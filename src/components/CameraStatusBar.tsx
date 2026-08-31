import {
    ClipboardClockIcon,
    ClockIcon,
    ImageIcon,
    RotateCwFadingClockIcon,
    ScrollTextIcon,
    SettingsIcon,
    UnplugIcon,
} from "lucide-react";

import { useElapsed } from "../hooks/useElapsed";
import { formatDuration, formatInterval } from "../lib/format";
import type { ShotClock } from "../hooks/useShotClock";
import { measuredInterval } from "../lib/interval";
import {
    DIALS,
    type BatteryStatus,
    type CameraInfo,
    type Dial,
    type ExposureCapabilities,
    type ExposureSettings,
    type RampSettings,
} from "../lib/types";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import { Panel } from "./ui/Panel";
import { Select } from "./ui/Select";
import DynamicBatteryIcon from "./ui/DynamicBatteryIcon";
import { ExposureMeter } from "./ExposureMeter";
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
    /** Stops the last frame sat from the target, for the exposure meter. */
    deviation: number | null;
    onChangeDial: (dial: Dial, raw: string) => void;
    onOpenSettings: () => void;
    onOpenPlanner: () => void;
    onOpenLogs: () => void;
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
export function CameraStatusBar({
    info,
    capabilities,
    exposure,
    battery,
    busy,
    ramp,
    lastRamped,
    clock,
    deviation,
    onChangeDial,
    onOpenSettings,
    onOpenPlanner,
    onOpenLogs,
    onDisconnect,
}: Props) {
    // Measured, never configured: the intervalometer owns the timing and this app only watches it.
    // A number typed in here could disagree with what the camera is actually doing, and then it
    // would be worse than no number at all.
    const interval = measuredInterval(clock.recent);
    const running = useElapsed(clock.firstAt);

    return (
        // Wrapping rather than a fixed pair of columns: on a phone the dials alone are as
        // much as a line will hold, and halving the width for a second column is what
        // squeezed the captions into each other and pushed the buttons off the edge.
        //
        // `@container` so the pieces below size themselves against this strip rather than
        // against the window. In landscape the strip is a fraction of the width, and a
        // viewport breakpoint would call that case wide.
        <Panel
            className="@container flex flex-wrap items-center gap-x-4 gap-y-2 px-3 py-[0.6rem]"
            aria-label="Camera status"
        >
            {/* A floor rather than a share: below about this width the dials stop being
                readable, so they take the whole line and the meta group drops beneath
                them. Capped above, so on a wide strip the leftover goes to that group. */}
            <div className="flex min-w-0 max-w-104 flex-[1_1_17rem] gap-2">
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

            {/* Dropped on a narrow strip rather than truncated: one letter and an ellipsis
                identifies nothing, and the model is not something you consult mid-sequence.
                Everything else in here is, so it is what keeps the room. */}
            <span
                className="hidden min-w-0 flex-[1_1_auto] truncate text-right font-semibold @md:inline"
                title={[info.manufacturer, info.firmware, info.serial && `#${info.serial}`].filter(Boolean).join(" · ")}
            >
                {info.model}
            </span>

            {/* The meter against the readouts, with the buttons on the same line. They are
                all furniture rather than exposure, and giving the buttons a line of their
                own cost a row of height that the preview wants back. */}
            <div className="flex w-full min-w-0 flex-wrap items-center gap-2">
                <ExposureMeter stops={deviation} />

                <div className="flex flex-wrap items-center gap-2">
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
                        <Badge
                            className={cn(
                                "flex items-center gap-1",
                                battery.percent && battery.percent <= 15 && "text-danger",
                            )}
                        >
                            {battery.percent === null ? battery.label : `${battery.percent}%`}{" "}
                            <DynamicBatteryIcon className="size-5" value={battery.percent ?? -1} />
                        </Badge>
                    )}
                </div>

                {/* Pushed to the far end of whichever line they land on, so they stay in the
                    same corner whether or not the readouts left room beside them. */}
                <div className="ml-auto flex items-center gap-2">
                    <Button size="compact" onClick={onOpenSettings} aria-label="Settings" title="Settings">
                        <SettingsIcon className="size-4" />
                    </Button>
                    <Button size="compact" onClick={onOpenPlanner} aria-label="Planner" title="Planner">
                        <ClipboardClockIcon className="size-4" />
                    </Button>
                    <Button size="compact" onClick={onOpenLogs} aria-label="Logs" title="Logs">
                        <ScrollTextIcon className="size-4" />
                    </Button>
                    <Button size="compact" onClick={onDisconnect} aria-label="Disconnect" title="Disconnect">
                        <UnplugIcon className="size-4" />
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
    return (
        <span
            role="img"
            aria-label={DOT_LABELS[state]}
            title={DOT_LABELS[state]}
            className={cn("size-1.5 shrink-0 rounded-full", DOT_COLOURS[state])}
        />
    );
}
