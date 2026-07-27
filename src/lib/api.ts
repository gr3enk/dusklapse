/**
 * The only place the frontend talks to Rust.
 *
 * Camera I/O deliberately never happens in the WebView: App Transport Security
 * blocks the plaintext HTTP that CCAPI speaks, and the frame timing of a
 * timelapse has no business depending on the JS event loop.
 */

import { invoke } from "@tauri-apps/api/core";
import type { BatteryStatus, CameraInfo, CameraTarget, Dial, ExposureCapabilities, ExposureSettings, Vendor } from "./types";

/** Serialized form of `CameraError`. Branch on `kind`, display `message`. */
export interface CameraError {
    kind: "notConnected" | "unsupportedVendor" | "transport" | "rejected" | "protocol" | "unavailable" | "valueNotSelectable";
    message: string;
}

export function isCameraError(error: unknown): error is CameraError {
    return typeof error === "object" && error !== null && "kind" in error && "message" in error && typeof (error as CameraError).message === "string";
}

/** Never surface a raw thrown value to the user; it may not be a string. */
export function errorMessage(error: unknown): string {
    if (isCameraError(error)) return error.message;
    if (error instanceof Error) return error.message;
    if (typeof error === "string") return error;
    return "Something went wrong talking to the camera.";
}

export const api = {
    connect: (target: CameraTarget) => invoke<CameraInfo>("camera_connect", { target }),

    disconnect: () => invoke<void>("camera_disconnect"),

    /** `null` when nothing is connected. */
    status: () => invoke<CameraInfo | null>("camera_status"),

    capabilities: () => invoke<ExposureCapabilities>("camera_capabilities"),

    exposure: () => invoke<ExposureSettings>("camera_exposure"),

    setExposure: (dial: Dial, value: string) => invoke<void>("camera_set_exposure", { dial, value }),

    /** Autofocus stays off by default - a body that refocuses between frames
     *  produces a sequence that pops. */
    shoot: (autofocus = false) => invoke<void>("camera_shoot", { autofocus }),

    battery: () => invoke<BatteryStatus | null>("camera_battery"),

    defaultPort: (vendor: Vendor) => invoke<number>("camera_default_port", { vendor }),
};
