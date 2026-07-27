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
}

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

export interface BatteryStatus {
    percent: number | null;
    label: string;
}

export const VENDORS: { id: Vendor; label: string; hint: string }[] = [
    { id: "canon", label: "Canon", hint: "CCAPI over HTTP - must be unlocked per body" },
    { id: "nikon", label: "Nikon", hint: "PTP-IP - not implemented yet" },
    { id: "sony", label: "Sony", hint: "PTP-IP - not implemented yet" },
    { id: "mock", label: "Simulator", hint: "Fake camera running in-process" },
];

export const DIALS: { id: Dial; label: string }[] = [
    { id: "shutter", label: "Shutter" },
    { id: "aperture", label: "Aperture" },
    { id: "iso", label: "ISO" },
];
