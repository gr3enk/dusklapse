/**
 * What the sequence being shot will turn into once it is played back.
 *
 * The one piece of arithmetic in this app that is about the finished film rather than the camera:
 * frames are captured over hours and watched in seconds, and the ratio between those is the thing
 * nobody can do in their head while standing behind a tripod.
 */

import { formatDuration } from "./format";

/** How long `shots` frames run for at `fps`, in seconds. */
export function playbackSeconds(shots: number, fps: number): number {
    // A frame rate of zero cannot arrive through the dialog and is clamped in the backend, but
    // dividing by it here would put `Infinity` on screen rather than failing anywhere useful.
    if (fps <= 0) return 0;
    return shots / fps;
}

/**
 * A playback length, written at the precision that length deserves.
 *
 * Tenths below a minute, because that is where a timelapse usually lands and where the difference
 * between 8.0s and 9.5s is forty frames of shooting. Above a minute the tenth is noise and the
 * `m:ss` the rest of the app uses reads better.
 */
export function formatPlayback(seconds: number): string {
    if (seconds < 60) return `${seconds.toFixed(1)} s`;
    return formatDuration(seconds * 1000);
}
