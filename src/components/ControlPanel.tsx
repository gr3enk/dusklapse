import { CrosshairIcon, SunriseIcon, SunsetIcon } from "lucide-react";

import type { AutoRamp } from "../hooks/useAutoRamp";
import type { Ramp } from "../hooks/useRamp";
import type { Sky } from "../hooks/useSky";
import { DIALS, dialLimitLabel, stopsBetween, type CameraInfo, type Dial, type DialRamp, type ExposureCapabilities, type Luminance, type RampMode, type RampOutcome, type Blocked } from "../lib/types";
import { DaylightCurveRow } from "./DaylightCurveRow";
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
    /** What the ramp last did, for the readout at the bottom. */
    autoRamp: AutoRamp;
    /** Where the sun is, for the daylight curve and the deviation readout. */
    sky: Sky;
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
export function ControlPanel({ info, busy, ramp, autoRamp, sky, capabilities, frameLuminance, onShoot, onRefresh }: Props) {
    const { settings, update, useCurrentFrame, saving, error } = ramp;

    // Everything below needs a loaded configuration; disabling rather than hiding keeps the
    // panel from changing height the moment it arrives.
    const ready = settings !== null;
    const active = settings?.active ?? false;

    // The target the engine is actually holding: the daylight curve may have walked it below
    // the stored reference. Measuring against the stored one instead would have the readout
    // saying "on target" while the ramp went on correcting.
    const target = sky.state?.effectiveReference ?? settings?.reference ?? null;
    const deviation = target && frameLuminance ? stopsBetween(frameLuminance, target) : null;

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
            <h2 className="m-0 text-[1.1rem] font-[650]">Controls</h2>

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

            <div className="pl-4 text-[0.9rem] text-text-muted">
                <div className="flex items-center gap-2">
                    <h3 className="m-0 mb-1 font-semibold uppercase tracking-[0.06em]">Holy Grail</h3>
                    <Toggle checked={active} onChange={(next) => update({ active: next })} />
                </div>
                <div className="grid portrait:grid-cols-2 landscape:grid-cols-1 portrait:gap-x-8 gap-y-4 pt-4">
                    <div className="flex items-center gap-2 portrait:col-1">
                        {MODES.map(({ mode, label, Icon }) => (
                            <Button
                                key={mode}
                                variant={settings?.mode === mode ? "primary" : "secondary"}
                                onClick={() => update({ mode })}
                                className="w-full"
                                disabled={!active}
                                aria-pressed={settings?.mode === mode}
                            >
                                <Icon className="size-4" />
                                <span className="mr-1">{label}</span>
                            </Button>
                        ))}
                    </div>

                    <div className="flex flex-col gap-2 portrait:col-1 portrait:row-span-2">
                        <Label>Luminance reference</Label>
                        <div className="grid grid-cols-2 flex-wrap items-center gap-2 w-full">
                            <NumberSelector
                                label="luminance reference"
                                className="w-full"
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
                                disabled={!active || !ready || saving || frameLuminance === null}
                                className="w-full"
                                title={frameLuminance === null ? "No frame measured yet" : "Set the reference to the frame on screen"}
                            >
                                <CrosshairIcon className="size-4" />
                                Use current
                            </Button>
                        </div>

                        {/* The reference is only meaningful next to where the sequence actually
                        is, and this is the number the engine will act on. */}
                        <p className="m-0 tabular-nums opacity-60 text-sm">
                            {frameLuminance === null
                                ? "No frame measured yet."
                                : deviation === null
                                  ? `Frame at ${frameLuminance.value}, deviation unavailable.`
                                  : `Frame at ${frameLuminance.value}, ${describeDeviation(deviation)}`}
                            {/* Named outright when the curve has moved the goalposts, so the
                            deviation above is measured against a number that is on screen. */}
                            {sky.state && sky.state.offsetStops < -0.005 && <span className="block">Aiming at {sky.state.effectiveReference.value} while the sky is this dark.</span>}
                        </p>
                    </div>

                    {/* One row per dial. The label flips with the mode because the stored number
                    does not: the limit is always the far end of the ramp's travel, and which
                    end that is depends on which way the light is going. */}
                    {/* Placed after the reference and before the dials: it changes what the
                    reference means, which is the thing directly above it. */}
                    <div className="portrait:col-1 portrait:row-span-4">
                        <DaylightCurveRow
                            config={settings?.daylight ?? { enabled: false, factor: 2, location: null }}
                            sky={sky}
                            rampActive={active}
                            busy={saving}
                            onChange={(daylight) => update({ daylight })}
                        />
                    </div>

                    <div className="portrait:col-2 portrait:row-start-1 portrait:row-span-4">
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
                </div>

                <div className="py-4">
                    {/* What the ramp actually did, and the one thing that has to be visible before
                    it is too late: running out of headroom. */}
                    {autoRamp.outcome && <RampReadout outcome={autoRamp.outcome} capabilities={capabilities} />}

                    {error && <Notice variant="error">{error}</Notice>}
                    {autoRamp.error && <Notice variant="error">{autoRamp.error}</Notice>}
                </div>
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

/**
 * What the ramp did about the last frame.
 *
 * When nothing could move, this names the dial and the limit rather than reporting a number of
 * stops. "Out of headroom by 3.31 EV" is true and useless; "ISO is at its limit of 1250" is
 * something you can act on.
 */
function RampReadout({ outcome, capabilities }: { outcome: RampOutcome; capabilities: ExposureCapabilities | null }) {
    // A raw token is the camera's own vocabulary, not something to show. Resolve it back to
    // the label the dial displays.
    const labelFor = (dial: Dial, raw: string) => capabilities?.[dial].find((value) => value.raw === raw)?.label ?? raw;

    // A dial that was switched off was switched off on purpose, so it is not news. Everything
    // else in this list means the ramp could not do what was asked.
    const off = outcome.blocked.filter((entry) => entry.reason.kind === "disabled");
    const stuck = outcome.blocked.filter((entry) => entry.reason.kind !== "disabled");

    return (
        <div className="space-y-2">
            {outcome.change?.applied && (
                <p className="m-0 tabular-nums">
                    {DIAL_LABELS[outcome.change.dial]} {labelFor(outcome.change.dial, outcome.change.from)} → {labelFor(outcome.change.dial, outcome.change.to)} (
                    {outcome.change.gainedStops > 0 ? "+" : ""}
                    {outcome.change.gainedStops.toFixed(2)} EV)
                </p>
            )}

            {/* Only a genuinely stuck dial is worth an alarm. */}
            {stuck.length > 0 && (
                <Notice variant="error">
                    <span className="block">Nothing left to adjust, so the sequence will keep drifting from here.</span>
                    {stuck.map((entry) => (
                        <span key={entry.dial} className="block">
                            {DIAL_LABELS[entry.dial]}: {describeBlocked(entry.reason, (raw) => labelFor(entry.dial, raw))}
                        </span>
                    ))}
                    {off.length > 0 && <span className="block">Ramping is switched off for {off.map((entry) => DIAL_LABELS[entry.dial]).join(" and ")}.</span>}
                </Notice>
            )}

            {outcome.failed && <Notice variant="error">The camera refused the change: {outcome.failed}</Notice>}
        </div>
    );
}

function describeBlocked(reason: Blocked, label: (raw: string) => string): string {
    switch (reason.kind) {
        case "disabled":
            return "ramping is switched off.";
        case "atLimit":
            return `already at its limit of ${label(reason.limit)}.`;
        case "endOfRange":
            return "the camera offers nothing further in this direction.";
        case "noStopPosition":
            return "it is on bulb or auto, which the ramp cannot reason about.";
        case "limitUnavailable":
            return `its limit of ${label(reason.limit)} is not offered by the camera right now.`;
    }
}

/** Dial names for the readout, so it does not repeat the label helper's mode-dependent ones. */
const DIAL_LABELS: Record<Dial, string> = {
    shutter: "Shutter",
    aperture: "Aperture",
    iso: "ISO",
};
