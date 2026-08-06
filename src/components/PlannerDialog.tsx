import type { ReactNode } from "react";

import type { Settings } from "../hooks/useSettings";
import { formatInterval } from "../lib/format";
import { formatPlayback, playbackSeconds } from "../lib/timelapse";
import { FPS_OPTIONS } from "../lib/types";
import { Label } from "./ui/Label";
import { Modal } from "./ui/Modal";
import { Notice } from "./ui/Notice";
import { SegmentedControl } from "./ui/SegmentedControl";

interface Props {
    open: boolean;
    onClose: () => void;
    /** Every shot the camera has reported, transferred or not. */
    shots: number;
    /** Measured interval between shots in milliseconds. `null` before there are two to compare. */
    intervalMs: number | null;
    /** Whether this camera reports its frames at all. A body that does not leaves `shots` at zero. */
    reportsFrames: boolean;
    /** The stored settings, for the playback rate. */
    settings: Settings;
}

/**
 * What the sequence being shot will become.
 *
 * The two numbers at the top are the same ones the status strip carries, repeated here because
 * they are the inputs to everything below - a running time that did not show what it was computed
 * from would be a number to trust rather than to check.
 */
export function PlannerDialog({ open, onClose, shots, intervalMs, reportsFrames, settings }: Props) {
    const { value, update, error } = settings;
    // Disabled rather than hidden until the stored value arrives, so the dialog does not change
    // height the moment it does.
    const fps = value?.fps ?? 25;

    return (
        <Modal open={open} onClose={onClose} title="Planner">
            <section className="grid grid-cols-2 gap-2" aria-label="Sequence so far">
                <Stat label="Frames" value={reportsFrames ? shots.toString() : "–"} />
                <Stat label="Interval" value={intervalMs === null ? "–" : formatInterval(intervalMs)} />
            </section>

            {!reportsFrames && <Notice>This camera does not report new frames, so there is nothing to count.</Notice>}

            <section className="flex flex-col gap-2" aria-label="Playback rate">
                <Label>Play back at</Label>

                <SegmentedControl
                    aria-label="Frames per second"
                    options={FPS_OPTIONS.map((rate) => ({ value: rate.toString(), label: `${rate}` }))}
                    value={fps.toString()}
                    onChange={(rate) => update({ fps: Number(rate) })}
                />

                <p className="m-0 text-sm opacity-60">
                    Frames per second in the finished film. Nothing about the shoot changes - no camera setting and no
                    ramp reads this.
                </p>
            </section>

            <section className="flex flex-col gap-2" aria-label="Playback length">
                <Label>Runs for</Label>

                <p className="m-0 text-[2rem] leading-none font-[650] tabular-nums">
                    {reportsFrames && shots > 0 ? formatPlayback(playbackSeconds(shots, fps)) : "–"}
                </p>

                <p className="m-0 text-sm tabular-nums opacity-60">{describe(shots, fps, reportsFrames)}</p>
            </section>

            {error && <Notice variant="error">{error}</Notice>}
        </Modal>
    );
}

/** One reading with its caption, sized to be read across a tripod rather than at arm's length. */
function Stat({ label, value }: { label: string; value: ReactNode }) {
    return (
        <div className="flex flex-col gap-1 rounded-card border border-border bg-surface px-3 py-2">
            <Label>{label}</Label>
            <span className="text-[1.35rem] leading-none font-[650] tabular-nums">{value}</span>
        </div>
    );
}

/** The sum written out, so the big number above can be checked rather than believed. */
function describe(shots: number, fps: number, reportsFrames: boolean): string {
    if (!reportsFrames) return "A frame count is needed to work out a length.";
    if (shots === 0) return `Nothing shot yet. ${fps} frames will make the first second.`;
    return `${shots} frame${shots === 1 ? "" : "s"} at ${fps} fps.`;
}
