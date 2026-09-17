import { describe, expect, it } from "vitest";

import { normalizeAddress } from "../address";

describe("normalizeAddress", () => {
    /** What a German iPhone's numeric keypad produces, since it offers no period. */
    it("reads the keypad's comma as the separator it stands in for", () => {
        expect(normalizeAddress("192,168,1,2")).toBe("192.168.1.2");
    });

    it("leaves an address that already has periods alone", () => {
        expect(normalizeAddress("192.168.1.2")).toBe("192.168.1.2");
    });

    /** Both separators reach the same place, so a half-corrected address still connects. */
    it("accepts the two mixed", () => {
        expect(normalizeAddress("192.168,1,2")).toBe("192.168.1.2");
    });

    /** Pasted addresses arrive with whatever was around them. */
    it("drops surrounding whitespace", () => {
        expect(normalizeAddress("  192.168.1.2\n")).toBe("192.168.1.2");
    });

    it("has nothing to do to an empty field", () => {
        expect(normalizeAddress("")).toBe("");
    });
});
