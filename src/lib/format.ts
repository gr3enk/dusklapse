/**
 * How the two durations everyone watches are written.
 *
 * Shared rather than local to the status strip, for the reason `measuredInterval` gives: the
 * planner shows the same interval and the same running time the strip does, and two spellings of
 * one number read as two different numbers.
 */

/** A tenth of a second below ten, whole seconds above - past that the decimal is noise. */
export function formatInterval(milliseconds: number): string {
    if (isNaN(milliseconds)) return "-";
    const seconds = milliseconds / 1000;
    return seconds < 10 ? `${seconds.toFixed(1)}s` : `${Math.round(seconds)}s`;
}

/** `m:ss` under an hour, `h:mm:ss` above. */
export function formatDuration(milliseconds: number): string {
    const total = Math.floor(milliseconds / 1000);
    const hours = Math.floor(total / 3600);
    const minutes = Math.floor((total % 3600) / 60);
    const seconds = total % 60;

    const pad = (value: number) => value.toString().padStart(2, "0");
    return hours > 0 ? `${hours}:${pad(minutes)}:${pad(seconds)}` : `${minutes}:${pad(seconds)}`;
}
