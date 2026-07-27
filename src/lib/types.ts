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

export interface BatteryStatus {
    percent: number | null;
    label: string;
}

export const VENDORS: { id: Vendor; label: string; hint: string }[] = [
    { id: "canon", label: "Canon", hint: "CCAPI over HTTP - must be unlocked per body" },
    {
        id: "nikon",
        // Measured on a Z 6: the "connect to computer" path demands pairing with
        // Nikon's Wireless Transmitter Utility and tears the session down the
        // moment the camera leaves its pairing screen. The smart-device path in
        // access point mode has no such gate, and the camera stays fully usable.
        label: "Nikon",
        hint: "PTP-IP - use 'connect to smart device' and join the camera's own Wi-Fi",
    },
    { id: "sony", label: "Sony", hint: "PTP-IP - not implemented yet" },
    { id: "mock", label: "Simulator", hint: "Fake camera running in-process" },
];

export const DIALS: { id: Dial; label: string }[] = [
    { id: "shutter", label: "Shutter" },
    { id: "aperture", label: "Aperture" },
    { id: "iso", label: "ISO" },
];

/**
 * Address a camera takes when it hosts its own network.
 *
 * Offered as a preset because access point mode is the mode that works in the
 * field, and there is exactly one device on that network - typing an address is
 * pure friction. Nikon bodies answer here; Canon's differs, hence per vendor.
 */
export const ACCESS_POINT_HOST: Partial<Record<Vendor, string>> = {
    nikon: "192.168.1.1",
};
