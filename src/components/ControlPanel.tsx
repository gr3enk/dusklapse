import { CrosshairIcon, SunriseIcon, SunsetIcon } from "lucide-react";

import type { Ramp } from "../hooks/useRamp";
import { DIALS, dialLimitLabel, stopsBetween, type CameraInfo, type Dial, type DialRamp, type ExposureCapabilities, type Luminance, type RampMode } from "../lib/types";
import { DialRampRow } from "./DialRampRow";
import { Button } from "./ui/Button";
import { Label } from "./ui/Label";
import { Notice } from "./ui/Notice";
import NumberSelector from "./ui/NumberSelector";
import { Panel } from "./ui/Panel";
import Toggle from "./ui/Toggle";

/** The reported brightness scale, matching `SCALE` in the Rust luminance module. */
const LUMINANCE_MIN = 0;
const LUMINANCE_MAX = 10000;
/**
 * One step of the reference.
 *
 * 100 units is roughly a thirtieth of a stop near mid-grey - fine enough to place a
 * reference deliberately, coarse enough that getting there does not take forty presses.
 */
const LUMINANCE_STEP = 100;

const MODES: { mode: RampMode; label: string; Icon: typeof SunsetIcon }[] = [
    { mode: "sunset", label: "Sunset", Icon: SunsetIcon },
    { mode: "sunrise", label: "Sunrise", Icon: SunriseIcon },
];

interface Props {
    info: CameraInfo;
    busy: boolean;
    ramp: Ramp;
    /** What the camera currently offers on each dial, for the limit dropdowns. */
    capabilities: ExposureCapabilities | null;
    /** Brightness of the frame on screen, or `null` before the first one. */
    frameLuminance: Luminance | null;
    onShoot: () => void;
    onRefresh: () => void;
}

/**
 * Where the holy-grail ramp is configured.
 *
 * Scrolls on its own rather than growing the page: in landscape it is a full-height column
 * next to the preview, and a ramp with keyframes will be taller than any screen. Its own
 * scroll container is what keeps the preview and the status strip fixed in place while you
 * work down a long list of settings.
 *
 * Holds no state of its own. The configuration lives in Rust and arrives through `useRamp`,
 * which is what lets it survive a WebView reload and lets the engine read the same values
 * the controls here are showing.
 */
export function ControlPanel({ info, busy, ramp, capabilities, frameLuminance, onShoot, onRefresh }: Props) {
    const { settings, update, useCurrentFrame, saving, error } = ramp;

    // Everything below needs a loaded configuration; disabling rather than hiding keeps the
    // panel from changing height the moment it arrives.
    const ready = settings !== null;
    const active = settings?.active ?? false;

    const deviation = settings && frameLuminance ? stopsBetween(frameLuminance, settings.reference) : null;

    // `Dial` and the settings share field names, which is what lets one handler serve all
    // three rows instead of three near-identical ones.
    const updateDial = (dial: Dial, next: DialRamp) => update({ [dial]: next });

    return (
        <Panel
            // `overscroll-contain` stops this list handing its scroll on to the page when it
            // reaches the end.
            className="flex h-full flex-col gap-4 overflow-y-auto overscroll-contain p-[0.9rem]"
            aria-label="Timelapse ramping"
        >
            <h2 className="m-0 text-[1.1rem] font-[650]">Ramping</h2>

            <div className="flex flex-wrap gap-[0.6rem]">
                {info.supportsRelease ? (
                    <Button variant="primary" onClick={onShoot} disabled={busy}>
                        {busy ? "Working…" : "Take a frame"}
                    </Button>
                ) : (
                    // Offering a button that is guaranteed to fail is worse than explaining
                    // why there is none.
                    <Notice className="flex-[1_1_16rem]">
                        This body takes no remote release over Wi-Fi. Frame timing comes from your intervalometer; Dusklapse ramps the exposure between frames.
                    </Notice>
                )}
                <Button onClick={onRefresh} disabled={busy}>
                    Re-read camera
                </Button>
            </div>

            <div className="space-y-4 pl-4 text-[0.9rem] text-text-muted">
                <h3 className="m-0 mb-4 font-semibold uppercase tracking-[0.06em]">Holy Grail</h3>

                <div className="flex items-center gap-2">
                    <Toggle checked={active} onChange={(next) => update({ active: next })} />
                    <span>Holy Grail active</span>
                </div>

                <div className="flex items-center gap-2">
                    {MODES.map(({ mode, label, Icon }) => (
                        <Button
                            key={mode}
                            variant={settings?.mode === mode ? "primary" : "secondary"}
                            onClick={() => update({ mode })}
                            className="w-32"
                            disabled={!active}
                            aria-pressed={settings?.mode === mode}
                        >
                            <Icon className="size-4" />
                            <span className="mr-1">{label}</span>
                        </Button>
                    ))}
                </div>

                <div className="flex flex-col gap-2">
                    <Label>Luminance reference</Label>
                    <div className="flex flex-wrap items-center gap-2">
                        <NumberSelector
                            label="luminance reference"
                            disabled={!active || saving}
                            value={settings?.reference.value ?? 0}
                            // Only `value` is edited here; the backend rebuilds the matching
                            // `linear` when it stores the reference, so the two halves cannot
                            // drift apart.
                            onChange={(value) => settings && update({ reference: { ...settings.reference, value } })}
                            step={LUMINANCE_STEP}
                            min={LUMINANCE_MIN}
                            max={LUMINANCE_MAX}
                        />
                        {/* Needs no value passed to it: the backend reads the brightness from
                            the frame it already measured, so what gets stored is provably
                            what was measured. Disabled until a frame exists to point at. */}
                        <Button
                            variant="secondary"
                            onClick={useCurrentFrame}
                            disabled={!ready || saving || frameLuminance === null}
                            title={frameLuminance === null ? "No frame measured yet" : "Set the reference to the frame on screen"}
                        >
                            <CrosshairIcon className="size-4" />
                            Use current
                        </Button>
                    </div>

                    {/* The reference is only meaningful next to where the sequence actually
                        is, and this is the number the engine will act on. */}
                    <p className="m-0 tabular-nums">
                        {frameLuminance === null
                            ? "No frame measured yet."
                            : deviation === null
                              ? `Frame at ${frameLuminance.value}, deviation unavailable.`
                              : `Frame at ${frameLuminance.value}, ${describeDeviation(deviation)}`}
                    </p>
                </div>

                {/* One row per dial. The label flips with the mode because the stored number
                    does not: the limit is always the far end of the ramp's travel, and which
                    end that is depends on which way the light is going. */}
                <div className="space-y-4">
                    {DIALS.map(({ id }) => (
                        <DialRampRow
                            key={id}
                            dial={id}
                            label={settings ? dialLimitLabel(id, settings.mode) : ""}
                            config={settings?.[id] ?? { enabled: false, limit: null }}
                            values={capabilities?.[id] ?? []}
                            rampActive={active}
                            busy={saving}
                            onChange={(next) => updateDial(id, next)}
                        />
                    ))}
                </div>

                {error && <Notice variant="error">{error}</Notice>}
            </div>
        </Panel>
    );
}

/**
 * Says which way the correction has to go, not just how far.
 *
 * A bare signed number reads as arithmetic; "over" and "under" read as what the ramp will
 * do about it.
 */
function describeDeviation(stops: number): string {
    const magnitude = Math.abs(stops);
    if (magnitude < 0.05) return "on target.";
    return `${magnitude.toFixed(2)} EV ${stops > 0 ? "over" : "under"} reference.`;
}
