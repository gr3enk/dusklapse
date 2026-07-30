import { useCallback, useEffect, useRef, useState } from "react";

import { api, errorMessage, isConnectionLost } from "../lib/api";
import type { BatteryStatus, CameraInfo, Dial, ExposureCapabilities, ExposureSettings } from "../lib/types";
import { CameraStatusBar } from "./CameraStatusBar";
import { PreviewPane } from "./PreviewPane";
import { Notice } from "./ui/Notice";
import { Button } from "./ui/Button";
import { cn } from "../lib/utils";
import { ControlPanel } from "./ControlPanel";
import { useLatestFrame } from "../hooks/useLatestFrame";
import { useRamp } from "../hooks/useRamp";
import { useAutoRamp } from "../hooks/useAutoRamp";
import { useSky } from "../hooks/useSky";
import { useShotHistory } from "../hooks/useShotHistory";
import { usePrimeReference } from "../hooks/usePrimeReference";
import { useShotClock } from "../hooks/useShotClock";
import { SettingsDialog } from "./SettingsDialog";
import { ConfirmDialog } from "./ui/ConfirmDialog";
import { measuredInterval } from "../lib/interval";
import { transferShot } from "../lib/transfer";
import { useSettings } from "../hooks/useSettings";

/**
 * How often to re-read a camera that has to be asked.
 *
 * Canon has no push channel, so this is the only way its display stays honest
 * when someone turns a ring on the body.
 */
const POLL_INTERVAL_MS = 2000;

/**
 * How often to re-read a camera that announces its own changes.
 *
 * Not for freshness - events cover that, instantly, instead of the up-to-two
 * seconds of lag polling gave us. This is purely so a camera that vanished
 * (switched off, out of range) gets noticed within a reasonable time rather than
 * whenever someone next touches a control.
 */
const HEARTBEAT_INTERVAL_MS = 20000;

interface Props {
    info: CameraInfo;
    onDisconnected: () => void;
}

/**
 * Owns the camera session state and arranges the three working panes.
 *
 * The arrangement itself is CSS, not JavaScript: one grid whose placement is reshuffled
 * by a `landscape:` variant, which is a media query. Rotating the device therefore
 * relayouts without a re-render and without a resize listener to get wrong - and none of
 * the three components below has to know which orientation it is in.
 */
export function CameraPanel({ info, onDisconnected }: Props) {
    const [capabilities, setCapabilities] = useState<ExposureCapabilities | null>(null);
    const [exposure, setExposure] = useState<ExposureSettings | null>(null);
    const [battery, setBattery] = useState<BatteryStatus | null>(null);
    // The kind is kept alongside the text, not thrown away by `errorMessage`: a lost link is the
    // one failure with something to offer beyond an apology.
    const [error, setError] = useState<{ message: string; lost: boolean } | null>(null);
    const [reconnecting, setReconnecting] = useState(false);

    const report = useCallback((cause: unknown) => {
        setError({ message: errorMessage(cause), lost: isConnectionLost(cause) });
    }, []);
    const [busy, setBusy] = useState(false);
    // Counted from CaptureComplete, so one per exposure rather than one per file.
    // Counts and times every shot the camera reports, transferred or not, so the readouts stay
    // honest when transfers are thinned.
    const clock = useShotClock();
    const frames = clock.count;

    // Secondary settings, stored in Rust so a WebView reload cannot undo them mid-sequence.
    const [settingsOpen, setSettingsOpen] = useState(false);
    // Asked before disconnecting, because the button sits beside the ones used constantly and a
    // stray tap ends the session for good: `camera_reconnect` reuses the address of the session it
    // finds, and a deliberate disconnect is the one thing that removes it.
    const [confirmingDisconnect, setConfirmingDisconnect] = useState(false);
    const settings = useSettings();
    // Every frame until the stored value has loaded: thinning transfers nobody asked to thin would
    // silently drop measurements for the first seconds of a session.
    const transferEvery = settings.value?.transferEvery ?? 1;

    // Two concerns, two hooks, composed here rather than unpacked into this component.
    // The frame's measurements have several readers - the preview draws them, the ramp
    // controls need the brightness - so they cannot live inside the pane that shows them.
    // Changes only when a frame is due a transfer, so the fetch effect does not even run for the
    // ones in between.
    const frame = useLatestFrame(transferShot(frames, transferEvery));
    const ramp = useRamp();
    // Where the sun is. Read here rather than inside the controls because the deviation readout
    // has to measure against the target the engine is actually holding, not the stored one.
    const sky = useSky(ramp.settings);

    // Read by the poll timer, which must not restart every time `busy` flips.
    const busyRef = useRef(false);
    busyRef.current = busy;

    const readAll = useCallback(async () => {
        // Capability lists depend on the shooting mode and the attached lens, so
        // they get re-read rather than cached once at connect.
        const [nextCapabilities, nextExposure, nextBattery] = await Promise.all([
            api.capabilities(),
            api.exposure(),
            api.battery(),
        ]);
        setCapabilities(nextCapabilities);
        setExposure(nextExposure);
        setBattery(nextBattery);
    }, []);

    // Anchors the reference on the first frame of the session, if nobody has aimed it yet.
    const prime = usePrimeReference(frame.info, ramp.adopt);

    // Corrects the exposure once per measured frame. Only runs while the ramp is armed. Declared
    // after `readAll` because it takes it: the ramp writes to the camera from Rust, so the status
    // bar has no other way of learning that a dial moved.
    //
    // Held back until the opening anchor has settled. Otherwise the first frame would be judged
    // against the default reference - a number nobody chose - and the camera would be moved to
    // chase it, moments before that reference was replaced anyway.
    const autoRamp = useAutoRamp(frame.info, (ramp.settings?.active ?? false) && prime.settled, readAll);
    // One sample per frame, for the history overlay. Declared after the exposure state it reads so
    // the value it records is the one the frame was actually taken with.
    const history = useShotHistory(frame.info, exposure, ramp.settings, sky.state);

    const refresh = useCallback(async () => {
        setError(null);
        try {
            await readAll();
        } catch (cause) {
            report(cause);
        }
    }, [readAll, report]);

    useEffect(() => {
        void refresh();
    }, [refresh]);

    useEffect(() => {
        const every = info.pushesEvents ? HEARTBEAT_INTERVAL_MS : POLL_INTERVAL_MS;
        const timer = setInterval(() => {
            // A write is already in flight; queueing reads behind it would only
            // make the UI feel sluggish.
            if (busyRef.current) return;
            void readAll().catch((cause) => report(cause));
        }, every);
        return () => clearInterval(timer);
    }, [readAll, report, info.pushesEvents]);

    // React to what the camera volunteers. Only the events that change something
    // reach us; the Rust side already dropped the focus chatter.
    useEffect(() => {
        const unlisten = api.onCameraEvent((event) => {
            switch (event.kind) {
                case "dialChanged":
                    if (busyRef.current) return;
                    void readAll().catch((cause) => report(cause));
                    break;
                case "frameRecorded":
                    clock.record();
                    break;
            }
        });
        // `listen` resolves once registered; dropping the promise would leak the
        // handler across a remount.
        return () => void unlisten.then((stop) => stop());
    }, [readAll, report, clock.record, clock]);

    async function changeDial(dial: Dial, raw: string) {
        setBusy(true);
        setError(null);
        try {
            await api.setExposure(dial, raw);
            // Read back rather than assuming the write took: cameras silently clamp
            // to a neighbouring value more often than you would like.
            setExposure(await api.exposure());
        } catch (cause) {
            report(cause);
        } finally {
            setBusy(false);
        }
    }

    async function shoot() {
        setBusy(true);
        setError(null);
        try {
            await api.shoot(false);
            setBattery(await api.battery());
        } catch (cause) {
            report(cause);
        } finally {
            setBusy(false);
        }
    }

    /**
     * Attach to the same camera again.
     *
     * Manual on purpose. A camera that has switched its access point off cannot be reached by
     * retrying, so the useful moment to try is the one after someone has put the network back -
     * and only they know when that is.
     */
    async function reconnect() {
        setReconnecting(true);
        setError(null);
        try {
            await api.reconnect();
            // Read straight away rather than waiting for the poll: the dials may well have been
            // turned while the link was down.
            await readAll();
        } catch (cause) {
            report(cause);
        } finally {
            setReconnecting(false);
        }
    }

    async function disconnect() {
        try {
            await api.disconnect();
        } catch (cause) {
            // Losing the camera is exactly when disconnect fails, and the user still
            // wants to get back to the connect screen.
            console.warn("disconnect failed", cause);
        }
        onDisconnected();
    }

    return (
        // Portrait stacks preview, status, ramping in DOM order; landscape places them
        // explicitly, with ramping as a full-height column beside the other two.
        //
        // `minmax(0,...)` on every flexible track is load-bearing: grid tracks default to a
        // minimum of auto, so a long ramping list or a wide image would push its track past
        // the viewport instead of scrolling inside it.
        <div
            className={cn(
                "mx-auto grid min-h-0 w-full max-w-7xl flex-1 gap-3",
                "grid-cols-[minmax(0,1fr)] grid-rows-[minmax(0,0.9fr)_auto_minmax(0,1.1fr)]",
                "landscape:grid-cols-[minmax(0,3fr)_minmax(0,5fr)] landscape:grid-rows-[minmax(0,1fr)_auto]",
            )}
        >
            <div className="min-h-0 landscape:col-start-2 landscape:row-start-1">
                <PreviewPane
                    frame={frame}
                    count={frames}
                    supported={info.pushesEvents}
                    history={history}
                    transferEvery={transferEvery}
                />
            </div>

            <div className="flex flex-col gap-2 landscape:col-start-2 landscape:row-start-2">
                <CameraStatusBar
                    info={info}
                    capabilities={capabilities}
                    exposure={exposure}
                    battery={battery}
                    busy={busy}
                    ramp={ramp.settings}
                    // Only a change that actually reached the camera counts. A planned move the
                    // body refused would otherwise mark a dial the ramp never managed to turn.
                    lastRamped={autoRamp.outcome?.change?.applied ? autoRamp.outcome.change.dial : null}
                    clock={clock}
                    onChangeDial={changeDial}
                    onOpenSettings={() => setSettingsOpen(true)}
                    onDisconnect={() => setConfirmingDisconnect(true)}
                />
                {prime.error && <Notice variant="error">Could not set the opening reference: {prime.error}</Notice>}
                {error && (
                    <Notice variant="error" className="flex flex-wrap items-center justify-between gap-2">
                        <span>{error.message}</span>
                        {/* Only where it can do something. Offering it after a value the body
                            refused would suggest the connection was the problem. */}
                        {error.lost && (
                            <Button
                                variant="danger"
                                size="compact"
                                onClick={() => void reconnect()}
                                disabled={reconnecting}
                            >
                                {reconnecting ? "Reconnecting…" : "Reconnect"}
                            </Button>
                        )}
                    </Notice>
                )}
            </div>

            <div className="min-h-0 landscape:col-start-1 landscape:row-start-1 landscape:row-span-2">
                <ControlPanel
                    info={info}
                    busy={busy}
                    ramp={ramp}
                    autoRamp={autoRamp}
                    sky={sky}
                    capabilities={capabilities}
                    frameLuminance={frame.info?.analysis?.luminance ?? null}
                    onShoot={() => void shoot()}
                    onRefresh={() => void refresh()}
                />
            </div>

            <SettingsDialog
                open={settingsOpen}
                onClose={() => setSettingsOpen(false)}
                settings={settings}
                ramp={ramp}
                intervalMs={measuredInterval(clock.recent)}
            />

            {/* Names what is lost and what is not. "Are you sure?" tells nobody anything, and the
                answer here is not obvious: the settings live in the backend and survive, the
                counters and the charts live in this screen and do not. */}
            <ConfirmDialog
                open={confirmingDisconnect}
                title="Disconnect the camera?"
                confirmLabel="Disconnect"
                destructive
                onCancel={() => setConfirmingDisconnect(false)}
                onConfirm={() => {
                    setConfirmingDisconnect(false);
                    void disconnect();
                }}
            >
                <p className="m-0">The shot count, the running time and the history charts are kept by this screen and will be lost.</p>
                <p className="m-0 pt-2">Your ramp settings and the transfer setting are stored by the app and will still be there.</p>
            </ConfirmDialog>
        </div>
    );
}
