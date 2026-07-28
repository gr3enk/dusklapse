import type { LatestFrame } from "../hooks/useLatestFrame";
import { HistogramChart } from "./HistogramChart";
import { Label } from "./ui/Label";
import { Notice } from "./ui/Notice";

interface Props {
    frame: LatestFrame;
    /** How many frames the camera has reported. Only used for the placeholder wording. */
    count: number;
    /** Whether this camera reports new frames at all. Only changes the wording. */
    supported: boolean;
}

/**
 * Shows the JPEG from the most recent frame, with its histogram and brightness over it.
 *
 * Purely a view now: the fetching lives in `useLatestFrame`, because the measurements it
 * produces are read by the ramp controls as well as by this pane.
 */
export function PreviewPane({ frame, count, supported }: Props) {
    const { info, imageUrl, loading, error } = frame;

    return (
        <section className="flex h-full min-h-0 flex-col gap-2" aria-label="Latest frame">
            {/* Fills the grid cell rather than imposing an aspect ratio: the cell's shape
                already differs between orientations, and a fixed ratio would fight it. Black
                rather than the surface colour, because this is a photograph and a grey
                surround biases how you judge its exposure. */}
            <div className="relative flex min-h-0 flex-1 items-center justify-center overflow-hidden rounded-card border border-border bg-black">
                {imageUrl ? (
                    // Contain, never cover - a cropped preview would hide exactly the blown
                    // highlights you are checking for.
                    <img className="h-full w-full object-contain" src={imageUrl} alt={`Frame ${count}`} />
                ) : (
                    <p className="m-0 p-4 text-center text-[0.9rem] text-text-muted">{placeholder(supported, count)}</p>
                )}

                {loading && <span className="absolute top-[0.6rem] right-[0.6rem] rounded-full bg-black/60 px-[0.5rem] py-[0.2rem] text-[0.75rem]">Loading…</span>}

                {/* Overlaid rather than placed beside the image: the two are read together,
                    and giving the histogram its own row would take height from the frame it
                    describes. Bottom-left, where a photograph carries least of its subject. */}
                {info?.analysis && (
                    <div className="pointer-events-none absolute bottom-[0.6rem] left-[0.6rem] flex h-[min(11.5rem,42%)] w-[min(18rem,50%)] flex-col gap-[0.3rem] rounded-lg border border-white/15 bg-black/55 p-[0.35rem] backdrop-blur-sm">
                        <div className="flex items-baseline justify-between gap-2">
                            <Label className="text-[0.6rem] tracking-[0.08em]">Luminance</Label>
                            {/* The number the ramp is regulated against, so it is the one you
                                glance at. Tabular figures keep it from jittering sideways. */}
                            <span className="text-base font-[650] leading-none tabular-nums text-text">{info.analysis.luminance.value}</span>
                        </div>
                        <HistogramChart histogram={info.analysis.histogram} />
                    </div>
                )}

                {info && <span className="pointer-events-none absolute bottom-[0.6rem] right-[0.6rem] rounded-full bg-black/55 px-[0.5rem] py-[0.2rem] text-[0.7rem] tabular-nums text-text-muted">{info.filename}</span>}
            </div>

            {error && <Notice variant="error">{error}</Notice>}
        </section>
    );
}

function placeholder(supported: boolean, count: number): string {
    // Distinguishing these matters: "waiting" invites you to keep waiting, and on a body
    // that will never send a frame that would be a lie.
    if (!supported) return "This camera does not report new frames.";
    return count === 0 ? "Waiting for the first frame…" : "No preview yet";
}
