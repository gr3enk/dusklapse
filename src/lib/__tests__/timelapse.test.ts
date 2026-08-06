import { describe, expect, it } from "vitest";

import { formatPlayback, playbackSeconds } from "../timelapse";

describe("playbackSeconds", () => {
    it("turns a frame count into a running time", () => {
        expect(playbackSeconds(250, 25)).toBe(10);
        expect(playbackSeconds(1440, 24)).toBe(60);
    });

    it("does not round the answer", () => {
        // 8.4s, not 8s: the tenths are what the readout shows below a minute.
        expect(playbackSeconds(210, 25)).toBeCloseTo(8.4);
    });

    it("has nothing to run before the first frame", () => {
        expect(playbackSeconds(0, 25)).toBe(0);
    });

    /**
     * The backend clamps this away and no control can produce it, but the division is here and
     * `Infinity` on screen would be worse than a zero.
     */
    it("survives a frame rate of zero", () => {
        expect(playbackSeconds(500, 0)).toBe(0);
    });
});

describe("formatPlayback", () => {
    it("keeps the tenth below a minute", () => {
        expect(formatPlayback(8.4)).toBe("8.4 s");
        expect(formatPlayback(0)).toBe("0.0 s");
    });

    it("switches to minutes and seconds above one", () => {
        expect(formatPlayback(60)).toBe("1:00");
        expect(formatPlayback(154)).toBe("2:34");
    });

    it("carries hours when a sequence gets that long", () => {
        expect(formatPlayback(3725)).toBe("1:02:05");
    });
});
