/**
 * The only place the frontend talks to Rust.
 *
 * Camera I/O deliberately never happens in the WebView: App Transport Security
 * blocks the plaintext HTTP that CCAPI speaks, and the frame timing of a
 * timelapse has no business depending on the JS event loop.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { CAMERA_EVENT, type BatteryStatus, type CameraEvent, type CameraInfo, type CameraTarget, type Dial, type ExposureCapabilities, type ExposureSettings, type PreviewInfo, type VendorProfile } from "./types";

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

    /** Every vendor the backend supports, each describing itself. */
    vendors: () => invoke<VendorProfile[]>("camera_vendors"),

    /**
     * Ask the camera for the newest frame's metadata and histogram.
     *
     * `null` when there is nothing new - the same frame is never fetched twice. Only
     * JPEGs ever get here; a RAW is identified and skipped on the camera side without
     * crossing the network. Call `previewImage` afterwards for the pixels.
     */
    preview: () => invoke<PreviewInfo | null>("camera_preview"),

    /**
     * The pixels of the frame `preview` last reported, as raw bytes.
     *
     * `null` before anything has been fetched. Separate from the metadata so the
     * image travels as binary and the histogram as JSON, instead of forcing one of
     * them into the wrong encoding.
     */
    previewImage: async (): Promise<ArrayBuffer | null> => {
        const bytes = await invoke<ArrayBuffer>("camera_preview_image");
        return bytes.byteLength > 0 ? bytes : null;
    },

    /**
     * Subscribe to what the camera reports on its own.
     *
     * Returns the unlisten function. Await it before dropping the subscription:
     * `listen` resolves after the channel is registered, and discarding the promise
     * leaks the handler.
     */
    onCameraEvent: (handler: (event: CameraEvent) => void) => listen<CameraEvent>(CAMERA_EVENT, (message) => handler(message.payload)),
};
