import { ChartAreaIcon, FileChartLineIcon } from "lucide-react";
import { useState } from "react";

import type { LatestFrame } from "../hooks/useLatestFrame";
import type { Shot } from "../hooks/useShotHistory";
import { HistogramChart } from "./HistogramChart";
import { ShotHistoryChart, type HistoryMode } from "./ShotHistoryChart";
import { Button } from "./ui/Button";
import { Label } from "./ui/Label";
import { Notice } from "./ui/Notice";
import { cn } from "../lib/utils";

/** What each history reading is called, and what the numbers under it mean. */
const HISTORY_MODES: Record<HistoryMode, { title: string; hint: string; next: HistoryMode }> = {
    exposure: { title: "Dials", hint: "EV from start", next: "luminance" },
    luminance: { title: "Luminance", hint: "measured vs target", next: "exposure" },
};

interface Props {
    frame: LatestFrame;
    /** How many frames the camera has reported. Only used for the placeholder wording. */
    count: number;
    /** Whether this camera reports new frames at all. Only changes the wording. */
    supported: boolean;
    /** Every frame so far, for the history overlay. */
    history: Shot[];
    /** Transfer one frame in this many, for the countdown to the next transfer. */
    transferEvery: number;
}

/**
 * Shows the JPEG from the most recent frame, with its histogram and brightness over it.
 *
 * Purely a view now: the fetching lives in `useLatestFrame`, because the measurements it
 * produces are read by the ramp controls as well as by this pane.
 */
export function PreviewPane({ frame, count, supported, history, transferEvery }: Props) {
    const { info, imageUrl, loading, error } = frame;

    // View state, so it lives here rather than in the backend: which overlay someone wants to see
    // says nothing about the sequence and should not survive into it.
    const [showHistogram, setShowHistogram] = useState(true);
    const [showHistory, setShowHistory] = useState(false);
    const [showChannels, setShowChannels] = useState(true);
    const [historyMode, setHistoryMode] = useState<HistoryMode>("exposure");
    const reading = HISTORY_MODES[historyMode];

    // Where this shot sits in the transfer cycle. 1 is the one that transfers, so it counts up to
    // the setting and the next frame after that starts over.
    const positionInCycle = count === 0 ? 0 : ((count - 1) % transferEvery) + 1;

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

                {/* Both in one row: the countdown is a standing readout and the transfer notice is
                    momentary, and they answer the same question - is anything coming. */}
                <div className="pointer-events-none absolute top-[0.6rem] right-[0.6rem] flex items-center gap-[0.4rem]">
                    {loading && <span className="rounded-full bg-black/60 px-[0.5rem] py-[0.2rem] text-[0.75rem]">Loading…</span>}
                    {/* Left out at 1 of 1, where it would read "1/1" forever and say nothing. */}
                    {transferEvery > 1 && positionInCycle > 0 && (
                        <span
                            className={cn(
                                "rounded-full bg-black/60 px-[0.5rem] py-[0.2rem] text-[0.75rem] tabular-nums",
                                // The frame that actually crossed is worth marking, so the two
                                // states of the cycle are distinguishable at a glance.
                                positionInCycle === 1 ? "text-text" : "text-text-muted",
                            )}
                            title={`Shot ${positionInCycle} of every ${transferEvery}. Frame 1 of each group is the one transferred.`}
                        >
                            {positionInCycle}/{transferEvery}
                        </span>
                    )}
                </div>

                {/* Top-left, opposite the histogram, so the two never overlap however the cell is
                    shaped. Interactive unlike the histogram - the whole panel is the button that
                    switches readings, which is a bigger target than any control would be and
                    needs no icon competing with the curves. */}
                {showHistory && history.length > 0 && (
                    <button
                        type="button"
                        onClick={() => setHistoryMode(reading.next)}
                        aria-label={`${reading.title} history. Tap to show ${HISTORY_MODES[reading.next].title}.`}
                        className={cn(
                            "absolute top-[0.6rem] left-[0.6rem] flex h-[min(11.5rem,42%)] w-[min(18rem,50%)] flex-col gap-[0.3rem]",
                            "rounded-lg border border-white/15 bg-black/55 p-[0.35rem] text-left backdrop-blur-sm",
                            "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
                        )}
                    >
                        <div className="flex items-baseline justify-between gap-2">
                            <Label className="text-[0.6rem] tracking-[0.08em]">{reading.title}</Label>
                            {/* Says which reading this is without a legend, and doubles as the
                                hint that there is another one behind it. */}
                            <span className="text-[0.6rem] leading-none text-text-muted">{reading.hint}</span>
                        </div>
                        <ShotHistoryChart shots={history} mode={historyMode} />
                    </button>
                )}

                {/* Overlaid rather than placed beside the image: the two are read together,
                    and giving the histogram its own row would take height from the frame it
                    describes. Bottom-left, where a photograph carries least of its subject. */}
                {/* Interactive for the same reason as the history panel: the whole surface is the
                    button, so nothing has to sit on top of the curves. */}
                {showHistogram && info?.analysis && (
                    <button
                        type="button"
                        onClick={() => setShowChannels((shown) => !shown)}
                        aria-label={showChannels ? "Showing colour channels. Tap for luminance only." : "Showing luminance only. Tap for colour channels."}
                        className={cn(
                            "absolute bottom-[0.6rem] left-[0.6rem] flex h-[min(11.5rem,42%)] w-[min(18rem,50%)] flex-col gap-[0.3rem]",
                            "rounded-lg border border-white/15 bg-black/55 p-[0.35rem] text-left backdrop-blur-sm",
                            "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
                        )}
                    >
                        <div className="flex items-baseline justify-between gap-2">
                            {/* The suffix is the only thing on screen that says there is another
                                reading behind this one. */}
                            <Label className="text-[0.6rem] tracking-[0.08em]">Luminance {showChannels ? "· RGB" : "· L"}</Label>
                            {/* The number the ramp is regulated against, so it is the one you
                                glance at. Tabular figures keep it from jittering sideways. */}
                            <span className="text-base font-[650] leading-none tabular-nums text-text">{info.analysis.luminance.value}</span>
                        </div>
                        <HistogramChart histogram={info.analysis.histogram} showChannels={showChannels} />
                    </button>
                )}

                {/* Beside the filename because that corner is already the pane's own furniture
                    rather than part of the photograph. */}
                <div className="absolute bottom-[0.6rem] right-[0.6rem] flex items-center gap-[0.4rem]">
                    {info && <span className="pointer-events-none rounded-full bg-black/55 px-2 py-[0.2rem] text-[0.7rem] tabular-nums text-text-muted">{info.filename}</span>}
                    <OverlayToggle label="history charts" shown={showHistory} onToggle={() => setShowHistory((shown) => !shown)} Icon={FileChartLineIcon} />
                    <OverlayToggle label="histogram" shown={showHistogram} onToggle={() => setShowHistogram((shown) => !shown)} Icon={ChartAreaIcon} />
                </div>
            </div>

            {error && <Notice variant="error">{error}</Notice>}
        </section>
    );
}

/**
 * Show or hide one overlay.
 *
 * `aria-pressed` rather than a changed label: it is one control with two states, and screen
 * readers announce the state from that. The dimming is what says the same thing visually.
 */
function OverlayToggle({ label, shown, onToggle, Icon }: { label: string; shown: boolean; onToggle: () => void; Icon: typeof ChartAreaIcon }) {
    return (
        <Button
            variant="icon"
            onClick={onToggle}
            aria-pressed={shown}
            aria-label={`${shown ? "Hide" : "Show"} ${label}`}
            title={`${shown ? "Hide" : "Show"} ${label}`}
            // Smaller than a full tap target on purpose: this sits over the photograph, and the
            // 44pt square the variant gives it would cover a corner of the frame.
            className={cn("min-h-0 p-0 rounded-full bg-accent/10 backdrop-blur-sm text-accent", !shown && "text-text bg-text/10")}
        >
            <Icon className="size-4" />
        </Button>
    );
}

function placeholder(supported: boolean, count: number): string {
    // Distinguishing these matters: "waiting" invites you to keep waiting, and on a body
    // that will never send a frame that would be a lie.
    if (!supported) return "This camera does not report new frames.";
    return count === 0 ? "Waiting for the first frame…" : "No preview yet";
}
