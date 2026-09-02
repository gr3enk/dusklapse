import { beforeEach, describe, expect, it } from "vitest";

import { loadLastTarget, saveLastTarget } from "../lastTarget";

/** Enough of the `Storage` interface for these tests, with the failure modes real ones have. */
function memoryStorage(initial: Record<string, string> = {}): Storage {
    const entries = new Map(Object.entries(initial));
    return {
        get length() {
            return entries.size;
        },
        clear: () => entries.clear(),
        getItem: (key: string) => entries.get(key) ?? null,
        key: (index: number) => [...entries.keys()][index] ?? null,
        removeItem: (key: string) => void entries.delete(key),
        setItem: (key: string, value: string) => void entries.set(key, value),
    };
}

/** A device that refuses to store anything, which is what a private window looks like. */
function refusingStorage(): Storage {
    return {
        ...memoryStorage(),
        setItem: () => {
            throw new Error("quota exceeded");
        },
        getItem: () => {
            throw new Error("access denied");
        },
    };
}

describe("lastTarget", () => {
    let storage: Storage;

    beforeEach(() => {
        storage = memoryStorage();
    });

    it("offers the camera that was last reached", () => {
        saveLastTarget({ vendor: "canon", host: "192.168.1.2", port: "8080" }, storage);

        expect(loadLastTarget(storage)).toEqual({
            vendor: "canon",
            host: "192.168.1.2",
            port: "8080",
        });
    });

    it("has nothing to offer before anything has been connected", () => {
        expect(loadLastTarget(storage)).toBeNull();
    });

    it("replaces the previous camera rather than accumulating", () => {
        saveLastTarget({ vendor: "nikon", host: "192.168.1.1", port: "15740" }, storage);
        saveLastTarget({ vendor: "sony", host: "192.168.122.1", port: "15740" }, storage);

        expect(loadLastTarget(storage)?.vendor).toBe("sony");
    });

    /**
     * This crosses app versions: what was written months ago has to be checked, not trusted. A
     * half-shaped record would otherwise put the connect screen into a state nothing can describe.
     */
    it("ignores a stored value that is not a camera", () => {
        for (const junk of ["not json at all", "null", "42", '{"vendor":"nikon"}', "{}", '"nikon"']) {
            expect(loadLastTarget(memoryStorage({ "dusklapse.lastTarget": junk }))).toBeNull();
        }
    });

    /**
     * A device that will not store anything still has to draw a connect screen. Both directions
     * swallow the failure: there is nothing useful to tell anyone about it.
     */
    it("survives storage that throws in both directions", () => {
        const refusing = refusingStorage();

        expect(() => saveLastTarget({ vendor: "canon", host: "192.168.1.2", port: "8080" }, refusing)).not.toThrow();
        expect(loadLastTarget(refusing)).toBeNull();
    });

    /** Some embedded contexts have no storage at all rather than a broken one. */
    it("survives having no storage", () => {
        expect(loadLastTarget(undefined)).toBeNull();
        expect(() => saveLastTarget({ vendor: "nikon", host: "192.168.1.1", port: "15740" }, undefined)).not.toThrow();
    });
});
