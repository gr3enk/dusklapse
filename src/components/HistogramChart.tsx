import { useEffect, useRef } from "react";

import type { Histogram } from "../lib/types";

interface Props {
    histogram: Histogram;
}

/** One band per curve, drawn top to bottom in this order. */
const BANDS = [
    { key: "red", label: "R", stroke: "rgb(255 96 96)" },
    { key: "green", label: "G", stroke: "rgb(88 220 120)" },
    { key: "blue", label: "B", stroke: "rgb(96 156 255)" },
    { key: "luma", label: "L", stroke: "rgb(235 240 248)" },
] as const;

/** Vertical space between bands, in CSS pixels. */
const BAND_GAP = 3;

/**
 * Four separate plots - red, green, blue and weighted luma - over 256 tonal bins.
 *
 * Stacked vertically rather than side by side, because they share one x axis that
 * way: the same tone sits at the same horizontal position in all four, so "red peaks
 * further right than blue" is visible at a glance. A 2x2 grid would fit the same
 * plots in a squarer space but would break that alignment, which is the main thing
 * separate channels are read for.
 *
 * Canvas rather than SVG: 256 points times four curves is a thousand path segments
 * redrawn on every frame, and as SVG that would be a thousand DOM nodes for React to
 * reconcile several times a minute. One canvas rather than four also means the bands
 * cannot drift out of horizontal alignment with each other.
 */
export function HistogramChart({ histogram }: Props) {
    const canvas = useRef<HTMLCanvasElement | null>(null);

    useEffect(() => {
        const element = canvas.current;
        if (!element) return;

        const context = element.getContext("2d");
        if (!context) return;

        // Match the backing store to the device's pixel density, or the curves are
        // blurry on every phone and tablet made in the last decade.
        const ratio = window.devicePixelRatio || 1;
        const { width, height } = element.getBoundingClientRect();
        if (width === 0 || height === 0) return;

        element.width = Math.round(width * ratio);
        element.height = Math.round(height * ratio);
        context.setTransform(ratio, 0, 0, ratio, 0, 0);
        context.clearRect(0, 0, width, height);

        // One shared divisor across all four bands. Normalising each band to its own
        // maximum would draw every channel at full height and throw away the
        // comparison the separate plots exist for - you could no longer see which
        // channel actually has the most pixels at its peak.
        const ceiling = scaleCeiling(histogram);
        if (ceiling === 0) return;

        const bandHeight = (height - BAND_GAP * (BANDS.length - 1)) / BANDS.length;
        if (bandHeight <= 0) return;

        // Full-height, behind the bands: the guides are what tie the four x axes
        // together visually.
        drawGuides(context, width, height);

        BANDS.forEach(({ key, label, stroke }, index) => {
            const top = index * (bandHeight + BAND_GAP);
            drawBand(context, histogram[key], ceiling, { top, width, height: bandHeight }, stroke, label);
        });
    }, [histogram]);

    return <canvas className="histogram" ref={canvas} role="img" aria-label="Tone distribution of the latest frame as four plots: red, green, blue and luminance" />;
}

/**
 * Height to normalise against.
 *
 * The plain maximum is a trap: one clipped sky or a black frame edge puts a single
 * enormous spike in one bin and flattens everything else into the baseline. Clipping
 * to a high percentile keeps the spike visible - it runs off the top of its band,
 * which reads correctly as "lots of pixels here" - while the rest of the distribution
 * stays readable.
 */
function scaleCeiling(histogram: Histogram): number {
    const counts = [...histogram.red, ...histogram.green, ...histogram.blue, ...histogram.luma].filter((count) => count > 0).sort((a, b) => a - b);
    if (counts.length === 0) return 0;

    const index = Math.floor(counts.length * 0.99);
    return counts[Math.min(index, counts.length - 1)];
}

function drawGuides(context: CanvasRenderingContext2D, width: number, height: number) {
    context.strokeStyle = "rgb(255 255 255 / 0.1)";
    context.lineWidth = 1;
    // Quarter-tone marks. Enough to judge where a peak sits without becoming a grid
    // that competes with the curves.
    for (let step = 1; step < 4; step++) {
        const x = Math.round((width * step) / 4) + 0.5;
        context.beginPath();
        context.moveTo(x, 0);
        context.lineTo(x, height);
        context.stroke();
    }
}

interface Band {
    top: number;
    width: number;
    height: number;
}

function drawBand(context: CanvasRenderingContext2D, bins: number[], ceiling: number, band: Band, stroke: string, label: string) {
    const baseline = band.top + band.height;
    const step = band.width / (bins.length - 1);
    const y = (index: number) =>
        // Clamped: bins above the ceiling are drawn at the top of the band rather than
        // spilling into the band above.
        baseline - Math.min(bins[index] / ceiling, 1) * band.height;

    context.save();
    // Confine everything to this band so a clamped spike cannot bleed upward.
    context.beginPath();
    context.rect(0, band.top, band.width, band.height);
    context.clip();

    context.beginPath();
    context.moveTo(0, baseline);
    for (let index = 0; index < bins.length; index++) {
        context.lineTo(index * step, y(index));
    }
    context.lineTo(band.width, baseline);
    context.closePath();
    // A solid fill now that nothing overlaps - additive blending was only needed
    // while four curves shared one plot.
    context.fillStyle = withAlpha(stroke, 0.3);
    context.fill();

    context.beginPath();
    for (let index = 0; index < bins.length; index++) {
        const point = y(index);
        if (index === 0) context.moveTo(0, point);
        else context.lineTo(index * step, point);
    }
    context.strokeStyle = stroke;
    context.lineWidth = 1;
    context.stroke();

    context.restore();

    // Bottom-left of the band, where a histogram's shadows are and where a curve is
    // least likely to be tall enough to collide with it.
    context.fillStyle = withAlpha(stroke, 0.75);
    context.font = "600 8px -apple-system, system-ui, sans-serif";
    context.textAlign = "left";
    context.textBaseline = "bottom";
    context.fillText(label, 3, baseline - 1);
}

function withAlpha(colour: string, alpha: number): string {
    // The palette above is written as `rgb(r g b)`, so the alpha slot is appended.
    return colour.replace(")", ` / ${alpha})`);
}
