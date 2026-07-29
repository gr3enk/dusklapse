import { describe, expect, it } from "vitest";

import { measuredInterval } from "../interval";

/** Timestamps from a list of gaps in seconds. */
function timeline(...gaps: number[]): number[] {
    const stamps = [1_000_000];
    for (const gap of gaps) stamps.push(stamps[stamps.length - 1] + gap * 1000);
    return stamps;
}

describe("measuredInterval", () => {
    it("has no answer before two shots", () => {
        expect(measuredInterval([])).toBeNull();
        expect(measuredInterval([1_000_000])).toBeNull();
    });

    it("reports a steady interval exactly", () => {
        expect(measuredInterval(timeline(7, 7, 7, 7))).toBe(7000);
    });

    it("ignores a single long gap from a dropped frame", () => {
        // The reason for a median. A reconnect or a missed frame leaves one gap at several times
        // the length, and a mean would carry it for the rest of the session.
        const withOutage = timeline(7, 7, 7, 120, 7, 7, 7);
        expect(measuredInterval(withOutage)).toBe(7000);

        const mean = (stamps: number[]) => (stamps[stamps.length - 1] - stamps[0]) / (stamps.length - 1);
        expect(mean(withOutage)).toBeGreaterThan(20_000);
    });

    it("follows a genuine change of interval rather than averaging over history", () => {
        // Only the recent window counts, so changing the intervalometer is reflected instead of
        // being diluted by an hour of the old setting.
        const changed = timeline(...Array(30).fill(7), ...Array(11).fill(2));
        expect(measuredInterval(changed)).toBe(2000);
    });

    it("takes the midpoint when the window has an even number of gaps", () => {
        expect(measuredInterval(timeline(4, 6))).toBe(5000);
    });
});
