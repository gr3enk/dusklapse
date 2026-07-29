/**
 * Which frames of a sequence get fetched, when transfers are thinned to one in n.
 *
 * Extracted from the two components that need it. They were each doing the same modulo from
 * opposite ends - one to decide what to fetch, one to display the countdown - and two spellings of
 * one rule is how the display and the behaviour come to disagree.
 */

/**
 * The number of the newest shot that is due a transfer, or 0 before anything has been shot.
 *
 * Counts 1, 1+n, 1+2n… so the first frame of a session always crosses. Passing this to the fetch
 * rather than the raw count is what skips the request entirely for the frames in between: the
 * image never leaves the camera, which is the whole point.
 */
export function transferShot(count: number, every: number): number {
    if (count <= 0) return 0;
    const step = Math.max(1, Math.floor(every));
    return count - ((count - 1) % step);
}

/**
 * Where this shot sits in the transfer cycle, from 1 to n. 0 before anything has been shot.
 *
 * 1 is the frame that transfers, so the readout counts up to n and the next frame starts over.
 */
export function cyclePosition(count: number, every: number): number {
    if (count <= 0) return 0;
    const step = Math.max(1, Math.floor(every));
    return ((count - 1) % step) + 1;
}
