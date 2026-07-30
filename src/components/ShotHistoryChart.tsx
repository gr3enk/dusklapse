import { useEffect, useRef } from "react";

import type { Shot } from "../hooks/useShotHistory";

/** Which set of curves is on screen. */
export type HistoryMode = "exposure" | "luminance";

interface Series {
    label: string;
    stroke: string;
    /** `null` where the shot has no value for this curve, which leaves a gap rather than a dive. */
    value: (shot: Shot) => number | null;
}

/**
 * The dials, plotted as change rather than as absolute stops.
 *
 * Absolute stop positions would put the three curves in unrelated parts of the axis - a shutter
 * sits near -5, an aperture near -2.6, an ISO near +3 - and the chart would be three flat lines
 * at three heights. Relative to where each started, they share an origin and the shape of the
 * ramp becomes the thing you see.
 */
const EXPOSURE_SERIES: Series[] = [
    { label: "S", stroke: "rgb(255 196 96)", value: (shot) => shot.shutter },
    { label: "A", stroke: "rgb(120 220 255)", value: (shot) => shot.aperture },
    { label: "I", stroke: "rgb(200 150 255)", value: (shot) => shot.iso },
];

const LUMINANCE_SERIES: Series[] = [
    { label: "L", stroke: "rgb(235 240 248)", value: (shot) => shot.luminance },
    { label: "R", stroke: "rgb(112 96 255)", value: (shot) => shot.reference },
    { label: "A", stroke: "rgb(96 172 243)", value: (shot) => shot.effectiveReference },
];

interface Props {
    shots: Shot[];
    mode: HistoryMode;
}

/**
 * How the ramp has behaved over the whole sequence, in one of two readings.
 *
 * The point of it is the shape over time, which no single-frame readout can show: whether the
 * dials are stepping evenly or one of them is doing all the work, and whether the measured
 * brightness is tracking the reference or slowly parting from it.
 *
 * Two charts sharing one canvas rather than two stacked plots, because the pane has room for one
 * and both want the full width. Which is on screen is the caller's state - see the click handler
 * in the pane - so this component stays a drawing of what it is handed.
 */
export function ShotHistoryChart({ shots, mode }: Props) {
    const canvas = useRef<HTMLCanvasElement | null>(null);

    useEffect(() => {
        const element = canvas.current;
        if (!element) return;
        const context = element.getContext("2d");
        if (!context) return;

        // Match the backing store to the device's pixel density, or every line is soft.
        const ratio = window.devicePixelRatio || 1;
        const { width, height } = element.getBoundingClientRect();
        if (width === 0 || height === 0) return;

        element.width = Math.round(width * ratio);
        element.height = Math.round(height * ratio);
        context.setTransform(ratio, 0, 0, ratio, 0, 0);
        context.clearRect(0, 0, width, height);

        const series = mode === "exposure" ? EXPOSURE_SERIES : LUMINANCE_SERIES;
        // Relative to the first value for the dials, absolute for the luminance scale: change is
        // what makes three dials comparable, and brightness is already on one common scale.
        const points = series.map((entry) => samples(shots, entry, mode === "exposure"));

        const range = extent(points);
        if (!range) return;

        drawGuides(context, width, height, range);

        points.forEach((values, index) => {
            drawCurve(context, values, range, width, height, series[index].stroke);
        });

        drawLegend(context, series, width);
    }, [shots, mode]);

    return (
        <canvas
            className="block min-h-0 w-full flex-1"
            ref={canvas}
            role="img"
            aria-label={
                mode === "exposure"
                    ? "Change in shutter, aperture and ISO across the sequence so far, in stops"
                    : "Measured brightness, the reference, and the reference after the daylight curve, across the sequence so far"
            }
        />
    );
}

/** One curve's values, optionally rebased so it starts at zero. */
function samples(shots: Shot[], series: Series, relative: boolean): (number | null)[] {
    const raw = shots.map(series.value);
    if (!relative) return raw;

    // The first value that exists, not the first shot: a sequence that began on bulb would
    // otherwise rebase the whole curve against nothing.
    const first = raw.find((value) => value !== null);
    if (first === undefined || first === null) return raw;
    return raw.map((value) => (value === null ? null : value - first));
}

interface Extent {
    min: number;
    max: number;
}

/**
 * The vertical range to draw in.
 *
 * Shared across all three curves - normalising each to its own range would make a dial that never
 * moved look as busy as one that ramped four stops.
 */
function extent(points: (number | null)[][]): Extent | null {
    const values = points.flat().filter((value): value is number => value !== null);
    if (values.length === 0) return null;

    const min = Math.min(...values);
    const max = Math.max(...values);

    // A flat sequence has no range at all, and dividing by it would put every point on one edge
    // or produce NaN. Open it out so the line lands in the middle instead.
    if (max - min < 1e-6) {
        const padding = Math.max(Math.abs(max) * 0.1, 1);
        return { min: min - padding, max: max + padding };
    }

    const padding = (max - min) * 0.08;
    return { min: min - padding, max: max + padding };
}

function drawGuides(context: CanvasRenderingContext2D, width: number, height: number, range: Extent) {
    context.strokeStyle = "rgb(255 255 255 / 0.1)";
    context.lineWidth = 1;
    for (let step = 1; step < 4; step++) {
        const y = Math.round((height * step) / 4) + 0.5;
        context.beginPath();
        context.moveTo(0, y);
        context.lineTo(width, y);
        context.stroke();
    }

    // Zero is the line the dial curves are read against, so it is drawn where it actually falls
    // rather than assumed to be the middle.
    if (range.min < 0 && range.max > 0) {
        const y = Math.round(height - ((0 - range.min) / (range.max - range.min)) * height) + 0.5;
        context.strokeStyle = "rgb(255 255 255 / 0.25)";
        context.beginPath();
        context.moveTo(0, y);
        context.lineTo(width, y);
        context.stroke();
    }
}

function drawCurve(
    context: CanvasRenderingContext2D,
    values: (number | null)[],
    range: Extent,
    width: number,
    height: number,
    stroke: string,
) {
    // A single shot has no line to draw, so it gets a dot - otherwise the first frame of every
    // session looks like a chart that is broken.
    const step = values.length > 1 ? width / (values.length - 1) : 0;
    const y = (value: number) => height - ((value - range.min) / (range.max - range.min)) * height;

    context.strokeStyle = stroke;
    context.lineWidth = 1.25;
    context.beginPath();

    let drawing = false;
    values.forEach((value, index) => {
        if (value === null) {
            // Break the path: joining across a gap would draw a line through values that were
            // never measured.
            drawing = false;
            return;
        }
        const point = [index * step, y(value)] as const;
        if (drawing) context.lineTo(...point);
        else context.moveTo(...point);
        drawing = true;
    });
    context.stroke();

    if (values.length === 1 && values[0] !== null) {
        context.fillStyle = stroke;
        context.beginPath();
        context.arc(0, y(values[0]), 1.5, 0, Math.PI * 2);
        context.fill();
    }
}

/** Single letters along the top, since there is no room for words and no room for a key. */
function drawLegend(context: CanvasRenderingContext2D, series: Series[], width: number) {
    context.font = "600 8px -apple-system, system-ui, sans-serif";
    context.textBaseline = "top";
    context.textAlign = "right";

    let right = width - 3;
    for (const entry of [...series].reverse()) {
        context.fillStyle = entry.stroke;
        context.fillText(entry.label, right, 1);
        right -= context.measureText(entry.label).width + 5;
    }
}
