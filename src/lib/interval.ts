/** How many recent gaps to measure over. */
const WINDOW = 10;

/**
 * The interval between shots in milliseconds, or `null` before there are two to compare.
 *
 * The median of the recent gaps rather than the mean. A dropped frame, a reconnect, or a moment
 * where the app fell behind leaves one gap at twice the length, and a mean carries that outlier
 * for the rest of the session - a 7s interval would read as 8.4s and never settle back. A median
 * ignores it.
 *
 * Shared rather than local to the status strip: the settings dialog needs the same number to say
 * how often a thinned transfer actually produces a measurement, and two implementations of one
 * statistic would eventually disagree.
 */
export function measuredInterval(timestamps: number[]): number | null {
    if (timestamps.length < 2) return null;

    const recent = timestamps.slice(-(WINDOW + 1));
    const gaps = recent.slice(1).map((at, index) => at - recent[index]);

    const sorted = [...gaps].sort((a, b) => a - b);
    const middle = Math.floor(sorted.length / 2);
    return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
}
