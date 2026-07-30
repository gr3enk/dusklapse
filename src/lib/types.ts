/**
 * Mirrors the serde types in `src-tauri/src/camera/model.rs`.
 *
 * Keep the two in sync by hand for now. Once the shape settles it is worth
 * generating this file from Rust (ts-rs or specta) - a silent drift here shows
 * up as `undefined` deep inside the UI, which is a miserable thing to debug.
 */

export type Vendor = "canon" | "nikon" | "sony" | "mock";

export type Dial = "shutter" | "aperture" | "iso";

export interface CameraTarget {
    vendor: Vendor;
    host: string;
    port: number;
}

export interface CameraInfo {
    vendor: Vendor;
    manufacturer: string;
    model: string;
    serial: string | null;
    firmware: string | null;
    apiVersion: string | null;
    /**
     * Whether the body accepts a remote shutter release. False for Nikon over
     * Wi-Fi, where frame timing has to come from an intervalometer.
     */
    supportsRelease: boolean;
    /**
     * Whether the camera announces its own changes. Decides how hard the UI has to
     * poll: a body that pushes needs only a slow heartbeat to notice it vanished.
     */
    pushesEvents: boolean;
}

/**
 * Something the camera reported unprompted, delivered over the `camera://event`
 * channel. Mirrors the Rust `CameraEvent`, which serializes with a `kind` tag.
 */
export type CameraEvent =
    /** One exposure finished. The signal a ramp advances on. */
    | { kind: "frameRecorded" }
    /** A dial was turned on the body itself. */
    | { kind: "dialChanged"; dial: Dial };

/** Tauri event name the Rust side emits on. */
export const CAMERA_EVENT = "camera://event";

/**
 * One selectable position on a dial.
 *
 * `raw` is the camera's own token and is what you send back on a write - never
 * build one from `label`. `stops` is the value's brightness contribution, and is
 * `null` for values with no fixed brightness (`bulb`, `auto`).
 */
export interface ExposureValue {
    raw: string;
    label: string;
    stops: number | null;
}

export interface ExposureCapabilities {
    shutter: ExposureValue[];
    aperture: ExposureValue[];
    iso: ExposureValue[];
}

export interface ExposureSettings {
    shutter: ExposureValue | null;
    aperture: ExposureValue | null;
    iso: ExposureValue | null;
}

/**
 * Tone distribution of a preview frame, 256 bins per curve.
 *
 * Four curves rather than one: luma answers "is the exposure where I want it",
 * the separate channels answer "is one channel clipping", and the second question
 * matters earlier - a red sunset blows red long before luma looks blown.
 */
export interface Histogram {
    red: number[];
    green: number[];
    blue: number[];
    luma: number[];
    /** Pixels counted. Fewer than the frame has: the decode is scaled down. */
    pixels: number;
}

/**
 * Scene brightness of a frame, the quantity a ramp regulates on.
 *
 * `value` is the readout, 0 to 10000 with mid-grey near 5000 - the same kind of number
 * qDslrDashboard shows. `linear` is the one to compute with: it is proportional to the
 * light that reached the sensor, so a ratio of two of them is an exposure difference in
 * stops. Deliberately not the same as the histogram's luma, which is gamma-encoded and
 * therefore not linear in stops.
 */
export interface Luminance {
    linear: number;
    value: number;
}

/**
 * Settings that shape a session rather than an exposure. Mirrors the Rust `AppSettings`.
 *
 * Owned by Rust, like the ramp: a WebView reload must not quietly undo a choice made an hour into
 * a sequence.
 */
export interface AppSettings {
    /** Transfer one frame in this many. 1 is every frame. */
    transferEvery: number;
}

/** Bounds enforced by the backend. Mirrors `TRANSFER_EVERY_MIN`/`MAX`. */
export const TRANSFER_EVERY_MIN = 1;
export const TRANSFER_EVERY_MAX = 30;

/** Which way the light is going. Mirrors the Rust `RampMode`. */
export type RampMode = "sunset" | "sunrise";

/**
 * What the ramp is aiming for. Mirrors the Rust `RampSettings`.
 *
 * Owned by Rust, not by React: a WebView reload must not lose the reference mid-sequence,
 * and the engine that will consume it runs on that side. The frontend keeps a rendering
 * copy and writes through.
 */
/**
 * How far the ramp may take one dial, and whether it may touch it at all.
 *
 * One limit rather than two, because there is only ever one: the ramp travels in the
 * direction the light is going, and the limit is the far end of that travel. At sunset the
 * scene darkens and exposure opens up, so it reads as the longest shutter, highest ISO,
 * widest aperture; at sunrise everything runs the other way and the same field is the
 * shortest, lowest, smallest. Naming it is this layer's job - see `dialLimitLabel`.
 */
export interface DialRamp {
    enabled: boolean;
    /** A `raw` token the camera itself reported, or `null` until one is chosen. */
    limit: string | null;
}

/** Where the camera is, in WGS84 degrees. Mirrors the Rust `Location`. */
export interface Location {
    latitude: number;
    longitude: number;
}

/**
 * Darkens the reference as the sky darkens. Mirrors the Rust `DaylightCurve`.
 *
 * Without it a sunset ramp holds one brightness into the night, and the result looks like an
 * evenly lit day that happens to contain stars. Optional because over a lit city the sky stops
 * setting the exposure long before astronomical night.
 */
export interface DaylightCurve {
    enabled: boolean;
    /** How much darker night should look than day, as a brightness ratio. 2.0 is one stop. */
    factor: number;
    /** `null` until a position is known, which leaves the curve inert however it is set. */
    location: Location | null;
    /** How the darkening is spread across the event. */
    shape: DaylightShape;
    /** How far below the horizon the sun must be before the sequence counts as fully dark. */
    twilight: TwilightBand;
}

/**
 * Where the daylight fraction reaches zero. Mirrors the Rust `TwilightBand`.
 *
 * Changes *when* rather than *how much*: floored at astronomical dusk the curve runs for hours
 * and, at northern latitudes in summer, never finishes at all; floored at civil dusk it is done
 * within the hour.
 */
export type TwilightBand = "civil" | "nautical" | "astronomical";

/** The bands, with the elevation each ends at. */
export const TWILIGHT_BANDS: { value: TwilightBand; label: string; degrees: number }[] = [
    { value: "civil", label: "Civil", degrees: -6 },
    { value: "nautical", label: "Nautical", degrees: -12 },
    { value: "astronomical", label: "Astronomical", degrees: -18 },
];

/**
 * How the darkening is distributed across the event. Mirrors the Rust `DaylightShape`.
 *
 * All three share the same endpoints - full daylight leaves the reference alone, full night
 * applies the whole factor - and differ only in where the change is concentrated.
 *
 * Named for how they run **in time**, and they mean the same in both modes: the shape is applied
 * to the progress through the event, which counts from full day at sunset and from full night at
 * sunrise. A sunrise gives back what a sunset accumulated, in the same order.
 */
export type DaylightShape = "linear" | "slowThenFast" | "fastThenSlow";

/** The three shapes in the order they are offered. */
export const DAYLIGHT_SHAPES: DaylightShape[] = ["linear", "slowThenFast", "fastThenSlow"];

/** What the sky is doing, in the vocabulary photographers use. Mirrors the Rust `SkyPhase`. */
export type SkyPhase = "day" | "goldenHour" | "blueHour" | "nauticalTwilight" | "astronomicalTwilight" | "night";

/** What the sky is doing now and what the curve is doing about it. Mirrors `SkyState`. */
export interface SkyState {
    elevationDegrees: number;
    phase: SkyPhase;
    /** 1.0 in daylight, 0.0 at astronomical night. */
    daylight: number;
    /** The reference the ramp is actually aiming at, after the curve. */
    effectiveReference: Luminance;
    /** Stops the curve has taken off the stored reference. Zero or negative. */
    offsetStops: number;
}

export interface RampSettings {
    /** Whether the ramp may move the camera at all. Separate from having a reference. */
    active: boolean;
    mode: RampMode;
    reference: Luminance;
    shutter: DialRamp;
    aperture: DialRamp;
    iso: DialRamp;
    daylight: DaylightCurve;
}

/** What each sky phase is called on screen. */
export const SKY_PHASE_LABELS: Record<SkyPhase, string> = {
    day: "Day",
    goldenHour: "Golden hour",
    blueHour: "Blue hour",
    nauticalTwilight: "Nautical twilight",
    astronomicalTwilight: "Astronomical twilight",
    night: "Night",
};

/**
 * What the limit on a dial is called, given which way the light is going.
 *
 * The stored value is one number; only its name changes with the mode. Deriving the name
 * here rather than storing both readings is what keeps them from ever disagreeing.
 */
export function dialLimitLabel(dial: Dial, mode: RampMode): string {
    const sunset = mode === "sunset";
    switch (dial) {
        case "shutter":
            return sunset ? "Longest exposure" : "Shortest exposure";
        case "iso":
            return sunset ? "Max ISO" : "Min ISO";
        case "aperture":
            return sunset ? "Widest aperture" : "Smallest aperture";
    }
}

/** One dial move the ramp made, or tried to. */
export interface AppliedChange {
    dial: Dial;
    from: string;
    to: string;
    gainedStops: number;
    applied: boolean;
}

/** Why a dial could not be moved. Mirrors the Rust `Blocked`. */
export type Blocked =
    /** Its toggle is off. */
    | { kind: "disabled" }
    /** Already sitting on the limit that was set for it. */
    | { kind: "atLimit"; limit: string }
    /** The camera offers nothing further in this direction. */
    | { kind: "endOfRange" }
    /** On bulb or auto, which has no stop position. */
    | { kind: "noStopPosition" }
    /** The limit that was set is no longer in the camera's list. */
    | { kind: "limitUnavailable"; limit: string };

export interface BlockedDial {
    dial: Dial;
    reason: Blocked;
}

/** What the ramp did about one frame. */
export interface RampOutcome {
    deviationStops: number;
    /**
     * The move that was made, if any.
     *
     * At most one per frame: the ramp steps rather than jumping, because a dial moving several
     * stops between two frames is a visible flash in the finished sequence.
     */
    change: AppliedChange | null;
    /** Why each dial could not be used, when none of them could. */
    blocked: BlockedDial[];
    /** Set when the camera refused the move. */
    failed: string | null;
}

/** Everything a frame is measured for. Present together or not at all. */
export interface FrameAnalysis {
    histogram: Histogram;
    luminance: Luminance;
}

/** A frame's metadata, without its pixels. */
export interface PreviewInfo {
    filename: string;
    width: number;
    height: number;
    /** Size of the image the follow-up fetch will return. */
    bytes: number;
    /** `null` when the JPEG could not be decoded; the image is still shown. */
    analysis: FrameAnalysis | null;
}

/** Stops between two brightness readings. Positive means `frame` is brighter. */
export function stopsBetween(frame: Luminance, reference: Luminance): number | null {
    if (frame.linear <= 0 || reference.linear <= 0) return null;
    return Math.log2(frame.linear / reference.linear);
}

export interface BatteryStatus {
    percent: number | null;
    label: string;
}

/**
 * What a vendor is, before anything is connected to it. Mirrors the Rust
 * `VendorProfile`.
 *
 * Loaded from the backend rather than declared here. Labels, hints, default ports and
 * access point addresses are facts about a camera protocol, so they live next to the
 * code that speaks it; a copy in TypeScript would be a second source of truth that
 * drifts. Adding a vendor therefore touches no frontend file at all.
 */
export interface VendorProfile {
    vendor: Vendor;
    label: string;
    summary: string;
    defaultPort: number;
    accessPointHost: string | null;
    needsAddress: boolean;
    implemented: boolean;
}

export const DIALS: { id: Dial; label: string }[] = [
    { id: "shutter", label: "Shutter" },
    { id: "aperture", label: "Aperture" },
    { id: "iso", label: "ISO" },
];
