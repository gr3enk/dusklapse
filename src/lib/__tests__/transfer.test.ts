import { describe, expect, it } from "vitest";

import { cyclePosition, transferShot } from "../transfer";

describe("transferShot", () => {
    it("transfers every frame at 1", () => {
        expect([1, 2, 3, 4, 5].map((count) => transferShot(count, 1))).toEqual([1, 2, 3, 4, 5]);
    });

    it("transfers the first frame and then every nth", () => {
        // The first frame of a session must always cross, or a sequence opens with no measurement
        // and the ramp has nothing to judge.
        const shots = [1, 2, 3, 4, 5, 6, 7, 8].map((count) => transferShot(count, 3));
        expect(shots).toEqual([1, 1, 1, 4, 4, 4, 7, 7]);
    });

    it("changes exactly once per cycle, which is what skips the fetch", () => {
        // The value is a React effect key: it changing is the fetch. Counting distinct values is
        // therefore counting network round trips.
        const seen = new Set([...Array(30).keys()].map((index) => transferShot(index + 1, 5)));
        expect([...seen].sort((a, b) => a - b)).toEqual([1, 6, 11, 16, 21, 26]);
    });

    it("has nothing to transfer before the first frame", () => {
        expect(transferShot(0, 3)).toBe(0);
    });

    it("survives a nonsensical setting rather than dividing by zero", () => {
        // The backend clamps this, but a stale value in flight must not produce NaN on screen.
        expect(transferShot(4, 0)).toBe(4);
        expect(transferShot(4, -2)).toBe(4);
    });
});

describe("cyclePosition", () => {
    it("counts 1..n and starts over on the transferred frame", () => {
        const positions = [1, 2, 3, 4, 5, 6, 7].map((count) => cyclePosition(count, 3));
        expect(positions).toEqual([1, 2, 3, 1, 2, 3, 1]);
    });

    it("marks position 1 exactly when a transfer happens", () => {
        // The readout and the behaviour have to agree - this is the whole reason the two live in
        // one module.
        for (let count = 1; count <= 40; count++) {
            const transferred = transferShot(count, 4) === count;
            expect(cyclePosition(count, 4) === 1).toBe(transferred);
        }
    });

    it("is zero before the first frame", () => {
        expect(cyclePosition(0, 3)).toBe(0);
    });
});
