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
