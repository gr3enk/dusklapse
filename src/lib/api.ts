/**
 * The only place the frontend talks to Rust.
 *
 * Camera I/O deliberately never happens in the WebView: App Transport Security
 * blocks the plaintext HTTP that CCAPI speaks, and the frame timing of a
 * timelapse has no business depending on the JS event loop.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
    CAMERA_EVENT,
    type AppSettings,
    type BatteryStatus,
    type CameraEvent,
    type CameraInfo,
    type CameraTarget,
    type Dial,
    type ExposureCapabilities,
    type ExposureSettings,
    type PreviewInfo,
    type RampOutcome,
    type RampSettings,
    type SkyState,
    type VendorProfile,
} from "./types";

/** Serialized form of `CameraError`. Branch on `kind`, display `message`. */
export interface CameraError {
    kind:
        | "notConnected"
        | "unsupportedVendor"
        | "transport"
        | "rejected"
        | "busy"
        | "protocol"
        | "unavailable"
        | "valueNotSelectable";
    message: string;
}

export function isCameraError(error: unknown): error is CameraError {
    return (
        typeof error === "object" &&
        error !== null &&
        "kind" in error &&
        "message" in error &&
        typeof (error as CameraError).message === "string"
    );
}

/**
 * Whether this failure means the camera is no longer reachable.
 *
 * The two kinds that a lost link produces: `transport` when a socket died under an operation,
 * `notConnected` when the session had already gone. Everything else - a value the body refused, a
 * protocol surprise - leaves the connection intact and would only be confused by an offer to
 * reconnect.
 */
export function isConnectionLost(error: unknown): boolean {
    return isCameraError(error) && (error.kind === "transport" || error.kind === "notConnected");
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

    /**
     * Attach again to the camera already in the session.
     *
     * Takes no address - the backend reuses the one it connected to. Deliberately manual: when a
     * camera drops its access point, retrying reaches nothing, and only the person holding the
     * tablet knows when the network is back.
     */
    reconnect: () => invoke<CameraInfo>("camera_reconnect"),

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

    /** What the ramp is currently aiming for. */
    rampSettings: () => invoke<RampSettings>("ramp_settings"),

    /**
     * Replace the whole ramp configuration.
     *
     * Returns what was stored, so a caller never has to assume its write landed - and a
     * value the backend clamped comes straight back rather than drifting out of sync.
     */
    rampConfigure: (settings: RampSettings) => invoke<RampSettings>("ramp_configure", { settings }),

    /**
     * Point the reference at the brightness of the frame on screen.
     *
     * Takes no argument on purpose: the backend reads the value from the frame it already
     * cached, so the reference is provably the number that was measured rather than one
     * that made a round trip through the UI. `null` when nothing has been analysed yet.
     */
    rampReferenceFromLatestFrame: () => invoke<RampSettings | null>("ramp_reference_from_latest_frame"),

    /**
     * Aim the reference at the frame on screen, but only if nobody has aimed it yet.
     *
     * For the first frame after connecting. `null` when the reference was already set - by hand or
     * by a session still under way - which is what makes it safe to call on every connect.
     */
    rampPrimeReference: () => invoke<RampSettings | null>("ramp_prime_reference"),

    /**
     * Correct the exposure for the frame on screen, if it needs it.
     *
     * Takes no argument: the backend reads the brightness from the frame it already measured,
     * decides the correction and applies it. `null` when there was nothing to decide - no
     * frame yet, or the ramp disarmed.
     */
    rampApply: () => invoke<RampOutcome | null>("ramp_apply"),

    /**
     * Where the sun is and what the daylight curve is doing about the reference.
     *
     * `null` when the curve is off or has no position yet. Separate from `rampSettings`
     * because this answer changes on its own: settings move only when someone moves them,
     * the sky moves whether or not anyone is watching.
     */
    rampSky: () => invoke<SkyState | null>("ramp_sky"),

    /** The secondary settings, as stored by the backend. */
    settings: () => invoke<AppSettings>("settings_get"),

    /**
     * Replace the secondary settings.
     *
     * Returns what was stored, so a clamped value comes straight back rather than leaving the UI
     * showing a number that is not in force.
     */
    setSettings: (value: AppSettings) => invoke<AppSettings>("settings_set", { value }),

    /**
     * Whether this build can ask the device for its position.
     *
     * False on desktop, where the geolocation plugin is not compiled in. The UI hides the
     * "use my location" button rather than offering one that is certain to fail.
     */
    hasGeolocation: () => invoke<boolean>("platform_has_geolocation"),

    /**
     * Subscribe to what the camera reports on its own.
     *
     * Returns the unlisten function. Await it before dropping the subscription:
     * `listen` resolves after the channel is registered, and discarding the promise
     * leaks the handler.
     */
    onCameraEvent: (handler: (event: CameraEvent) => void) =>
        listen<CameraEvent>(CAMERA_EVENT, (message) => handler(message.payload)),
};
