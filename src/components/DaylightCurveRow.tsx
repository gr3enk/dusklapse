import { LocateFixedIcon } from "lucide-react";

import type { Sky } from "../hooks/useSky";
import {
    DAYLIGHT_SHAPES,
    SKY_PHASE_LABELS,
    type DaylightCurve,
    type DaylightShape,
    type Location,
    type RampMode,
} from "../lib/types";
import { Button } from "./ui/Button";
import { Label } from "./ui/Label";
import { Notice } from "./ui/Notice";
import NumberSelector from "./ui/NumberSelector";
import { TextField } from "./ui/TextField";
import Toggle from "./ui/Toggle";
import SplineTopDownIcon from "./icons/SplineTopDown";
import SplineBottomDownIcon from "./icons/SplineBottomDown";
import SplineTopUpIcon from "./icons/SplineTopUp";
import SplineBottomUpIcon from "./icons/SplineBottomUp";
import LinearDownIcon from "./icons/LinearDown";
import LinearUpIcon from "./icons/LinearUp";

/**
 * Factor bounds.
 *
 * 1.0 is "no difference between day and night", which is the second way to switch the curve
 * off. 8.0 is three stops, past which a night frame is dark enough that the ramp will have
 * given up its headroom holding it there.
 */
/**
 * Which icon stands for which shape, in each direction.
 *
 * The pairs are crossed on purpose. Each icon draws the curve as it runs in time, and a sunrise
 * runs the same shape while the reference climbs rather than falls - so "slow then fast" is drawn
 * falling at sunset and rising at sunrise. Mirroring the sunset icons instead would show the wrong
 * shape: `SplineTopUp` is fast-then-slow, not the rising twin of `SplineTopDown`.
 *
 * The label is deliberately not phrased as "darker" or "brighter": the same shape does both,
 * depending on which way the light is going.
 */
const SHAPES: Record<DaylightShape, { label: string; sunset: typeof LinearDownIcon; sunrise: typeof LinearDownIcon }> =
    {
        linear: { label: "Even throughout", sunset: LinearDownIcon, sunrise: LinearUpIcon },
        slowThenFast: {
            label: "Little at first, most towards the end",
            sunset: SplineTopDownIcon,
            sunrise: SplineBottomUpIcon,
        },
        fastThenSlow: {
            label: "Most at the start, tapering off",
            sunset: SplineBottomDownIcon,
            sunrise: SplineTopUpIcon,
        },
    };

const FACTOR_MIN = 1;
const FACTOR_MAX = 8;
/** A quarter of a ratio step is about an eighth of a stop near 2.0 - finer than anyone needs. */
const FACTOR_STEP = 0.5;

interface Props {
    config: DaylightCurve;
    /** Which way the light is going, which decides how the shapes are drawn. */
    mode: RampMode;
    sky: Sky;
    /** False while the ramp as a whole is disarmed. */
    rampActive: boolean;
    onChange: (next: DaylightCurve) => void;
}

/**
 * The daylight curve: let the sky decide how much darker the sequence gets.
 *
 * Off by default and easy to leave off, because it is wrong as often as it is right. Over a
 * lit city the sky stops setting the exposure long before astronomical night, and forcing the
 * reference down there just underexposes the lights.
 *
 * The coordinate fields stay even where the device can supply a position: a phone in a pocket
 * on the way to a viewpoint reports where the pocket is, and a tripod at a known spot is often
 * easier to type than to walk to.
 */
export function DaylightCurveRow({ config, mode, sky, rampActive, onChange }: Props) {
    const disabled = !rampActive || !config.enabled;

    const setLocation = (patch: Partial<Location>) => {
        // A half-typed coordinate is not a position. Keep whatever is there until both halves
        // parse, rather than writing a location that is half zero.
        const next = { latitude: config.location?.latitude ?? 0, longitude: config.location?.longitude ?? 0, ...patch };
        onChange({ ...config, location: next });
    };

    const useMyLocation = async () => {
        const location = await sky.locate();
        if (location) onChange({ ...config, location });
    };

    return (
        <div className="flex min-w-0 flex-col gap-2">
            {/* Switch beside the name, the way the dial rows carry theirs, and the sentence
                on its own line under both: sharing a line with the switch left it three words
                wide in a landscape column. */}
            <div className="flex min-w-0 items-center justify-between gap-2">
                <span className="truncate">Auto Luminance</span>
                <Toggle
                    disabled={!rampActive}
                    checked={config.enabled}
                    onChange={(enabled) => onChange({ ...config, enabled })}
                />
            </div>
            <p className="m-0 text-sm opacity-60">Darken the reference as the sun goes down</p>

            <div className="flex flex-col gap-2 pt-1">
                <div className="flex flex-wrap items-end gap-2">
                    <TextField
                        label="Latitude"
                        // Wide enough for a coordinate. Sharing the row equally instead left
                        // both fields a few characters wide on a phone.
                        fieldClassName="flex-[1_1_8rem]"
                        type="number"
                        inputMode="decimal"
                        step="0.0001"
                        min={-90}
                        max={90}
                        disabled={disabled}
                        value={config.location?.latitude ?? ""}
                        onChange={(event) => {
                            const latitude = Number(event.currentTarget.value);
                            if (Number.isFinite(latitude)) setLocation({ latitude });
                        }}
                    />
                    <TextField
                        label="Longitude"
                        fieldClassName="flex-[1_1_8rem]"
                        type="number"
                        inputMode="decimal"
                        step="0.0001"
                        min={-180}
                        max={180}
                        disabled={disabled}
                        value={config.location?.longitude ?? ""}
                        onChange={(event) => {
                            const longitude = Number(event.currentTarget.value);
                            if (Number.isFinite(longitude)) setLocation({ longitude });
                        }}
                    />
                    {/* Hidden rather than disabled where there is no plugin to call: a button that
                cannot ever work is worse than no button. */}
                    {sky.canLocate && (
                        <Button
                            variant="secondary"
                            className="shrink-0"
                            onClick={useMyLocation}
                            disabled={disabled || sky.locating}
                        >
                            <LocateFixedIcon className="size-4" />
                            {/* {sky.locating ? "Locating…" : "Use my location"} */}
                        </Button>
                    )}
                </div>
                <label className={"flex min-w-0 flex-col gap-1"}>
                    <Label>Factor</Label>
                    <div className="flex flex-wrap items-center gap-2">
                        <NumberSelector
                            label="night darkening factor"
                            // The three shape buttons below cannot be drawn any narrower, so
                            // this is the half that gives, and drops onto its own line first.
                            className="flex-[1_1_9rem]"
                            disabled={disabled}
                            value={config.factor}
                            // Rounded because repeatedly adding 0.25 in binary floating point drifts, and
                            // a factor of 2.2500000000000004 would be shown as it is stored.
                            onChange={(factor) => onChange({ ...config, factor: Math.round(factor * 100) / 100 })}
                            step={FACTOR_STEP}
                            min={FACTOR_MIN}
                            max={FACTOR_MAX}
                        />
                        {/* `aria-pressed` rather than radio roles, the same as `SegmentedControl`:
                        these act immediately rather than being a form field to submit. The label
                        carries the whole meaning, since the icons are the only thing on screen. */}
                        <div
                            className="flex shrink-0 items-center gap-2"
                            role="group"
                            aria-label="Shape of the darkening"
                        >
                            {DAYLIGHT_SHAPES.map((shape) => {
                                const { label, sunset, sunrise } = SHAPES[shape];
                                const Icon = mode === "sunset" ? sunset : sunrise;
                                const selected = config.shape === shape;
                                return (
                                    <Button
                                        key={shape}
                                        variant={selected ? "primary" : "secondary"}
                                        aria-pressed={selected}
                                        aria-label={label}
                                        title={label}
                                        disabled={disabled}
                                        onClick={() => onChange({ ...config, shape })}
                                    >
                                        <Icon className="size-4" />
                                    </Button>
                                );
                            })}
                        </div>
                    </div>
                </label>
            </div>

            <div className="flex flex-col gap-2 pt-1">
                <p className="m-0 text-sm tabular-nums opacity-60">
                    {config.factor <= 1
                        ? "No darkening - same brightness all night."
                        : `${config.factor.toFixed(2)}× darker at night, which is ${Math.log2(config.factor).toFixed(2)} EV.`}
                </p>
                {sky.state && (
                    <div className="pt0.5 text-sm tabular-nums opacity-60">
                        <p className="m-0">
                            {SKY_PHASE_LABELS[sky.state.phase]} · sun {sky.state.elevationDegrees.toFixed(1)}° ·{" "}
                            {Math.round(sky.state.daylight * 100)}% daylight
                        </p>
                        <p className="m-0">
                            Target {sky.state.effectiveReference.value}
                            {sky.state.offsetStops < -0.005 &&
                                ` (${sky.state.offsetStops.toFixed(2)} EV below your reference)`}
                        </p>
                    </div>
                )}
            </div>

            {config.enabled && !config.location && (
                <Notice>The curve needs a position before it can work out where the sun is.</Notice>
            )}
            {sky.error && <Notice variant="error">{sky.error}</Notice>}
        </div>
    );
}
