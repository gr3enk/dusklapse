import { useEffect, useState } from "react";
import { Alignment, Fit, Layout, useRive } from "@rive-app/react-webgl2";
import { ArrowLeftIcon, CircleQuestionMarkIcon } from "lucide-react";

import dusklapseSplash from "../assets/dusklapse_splash.riv?url";
import dusklapseSplashPoster from "../assets/dusklapse_splash.svg?url";
import { api, errorMessage } from "../lib/api";
import { loadLastTarget, saveLastTarget } from "../lib/lastTarget";
import type { CameraInfo, Vendor, VendorProfile } from "../lib/types";
import { cn } from "../lib/utils";
import NikonConnectionHelp from "./help/NikonConnectionHelp";
import { useLongPress } from "../hooks/useLongPress";
import { Button } from "./ui/Button";
import { HintStack } from "./ui/HintStack";
import { Label } from "./ui/Label";
import { Notice } from "./ui/Notice";
import { SegmentedControl } from "./ui/SegmentedControl";
import { TextField } from "./ui/TextField";

/**
 * How long the Connect button has to be held to reveal the simulator.
 *
 * Long enough that it cannot happen by accident - a slow tap or a button held while the app thinks
 * is nowhere near - and short enough to be bearable once you know.
 */
const UNLOCK_HOLD_MS = 7000;
const SPLASH_LAYOUT = new Layout({ fit: Fit.Cover, alignment: Alignment.BottomCenter });

interface Props {
    onConnected: (info: CameraInfo) => void;
    /** Whether the simulator is on offer. Held by the app, so it survives connecting once. */
    developerMode: boolean;
    onUnlockDeveloper: () => void;
}

export function ConnectScreen({ onConnected, developerMode, onUnlockDeveloper }: Props) {
    const [profiles, setProfiles] = useState<VendorProfile[]>([]);
    // The camera this app last reached, so a second session starts where the first left off. Read
    // once, when the screen is built: after that the field belongs to whoever is typing in it.
    //
    // Nikon is the fallback rather than a preference - it is the vendor with the longest-standing
    // backend, and something has to be selected before anything has been connected.
    const [vendor, setVendor] = useState<Vendor>(() => loadLastTarget()?.vendor ?? "nikon");
    const [showConnectionHelp, setShowConnectionHelp] = useState(false);
    const [host, setHost] = useState("");
    const [port, setPort] = useState("");
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);

    // The list of cameras comes from the backend, where each vendor describes itself.
    // Nothing here knows which vendors exist.
    useEffect(() => {
        let current = true;
        api.vendors()
            .then((loaded) => {
                if (current) setProfiles(loaded);
            })
            .catch((cause) => {
                if (current) setError(errorMessage(cause));
            });
        return () => {
            current = false;
        };
    }, []);

    // The simulator is described by the registry like any other vendor, and hidden here rather
    // than there - so the entry that appears once it is unlocked is the same one, built the same
    // way, and nothing has to be kept in step.
    const offered = profiles.filter((profile) => developerMode || !profile.developerOnly);
    const selected = profiles.find((profile) => profile.vendor === vendor);

    // Seven seconds on the Connect button. Nothing hints at it and nothing shows progress - a hint
    // would defeat the point, and nobody holds a button that long by accident. Unlocking also
    // selects the simulator: the commonest reason to perform this is to use it.
    //
    // Named rather than found by `developerOnly`, which used to identify the simulator on its own.
    // It no longer does: an unproven backend is hidden the same way, so that search would land on
    // whichever the registry happens to list first.
    const unlock = useLongPress(() => {
        const simulator = profiles.find((profile) => profile.vendor === "mock");
        if (!simulator) return;
        onUnlockDeveloper();
        setVendor(simulator.vendor);
    }, UNLOCK_HOLD_MS);
    // Read off the profile rather than special-cased here. "The simulator has no address"
    // is a fact about the simulator, and it belongs with the simulator.
    const needsAddress = selected?.needsAddress ?? false;
    const canSubmit = !busy && selected?.implemented === true && (!needsAddress || host.trim().length > 0);

    // Prefill the address a camera takes when it hosts its own network, and the port it
    // listens on. Access point mode is the mode that works in the field, and there is
    // exactly one device on that network, so typing the address would be friction for
    // nothing.
    useEffect(() => {
        if (!selected) return;

        // The remembered address wins for the camera it belongs to. Switching to a *different*
        // vendor is a fresh choice and gets that vendor's own defaults - carrying one camera's
        // address over to another would be worse than an empty field.
        const remembered = loadLastTarget();
        if (remembered?.vendor === selected.vendor) {
            setHost(remembered.host);
            setPort(remembered.port);
            return;
        }

        setHost(selected.accessPointHost ?? "");
        setPort(String(selected.defaultPort));
    }, [selected]);

    // A remembered vendor that is no longer on offer must not leave the screen showing a camera
    // nobody can select - the simulator after a restart, or any vendor hidden again later.
    useEffect(() => {
        if (profiles.length === 0) return;
        const available = profiles.filter((profile) => developerMode || !profile.developerOnly);
        if (available.length === 0 || available.some((profile) => profile.vendor === vendor)) return;
        setVendor(available[0].vendor);
    }, [profiles, developerMode, vendor]);

    async function connect(event: React.FormEvent) {
        event.preventDefault();
        setBusy(true);
        setError(null);
        try {
            const info = await api.connect({
                vendor,
                // Nothing to send when the camera is not on a network.
                host: needsAddress ? host.trim() : "",
                port: Number(port) || 0,
            });
            // Only after the camera actually answered. Remembering an address that was never
            // reached would hand the next session the same wrong guess.
            saveLastTarget({ vendor, host: host.trim(), port });
            onConnected(info);
        } catch (cause) {
            setError(errorMessage(cause));
        } finally {
            setBusy(false);
        }
    }

    if (showConnectionHelp) {
        return <ConnectionHelp vendor={vendor} onClose={() => setShowConnectionHelp(false)} />;
    }

    return (
        <div className="grid h-dvh w-full grid-rows-2 [@media(max-height:500px)]:grid-rows-1 self-stretch overflow-hidden">
            <div
                className="pointer-events-none min-h-0 overflow-hidden [@media(max-height:500px)]:hidden"
                aria-hidden="true"
            >
                <SplashAnimation />
            </div>

            <div className="flex min-h-0 items-center justify-center overflow-y-auto">
                <form
                    className={cn(
                        "flex w-full max-w-full md:max-w-104 flex-col gap-5",
                        "pt-8 pr-[calc(var(--spacing-safe-r)+1.25rem)] pb-[calc(var(--spacing-safe-b)+2rem)] pl-[calc(var(--spacing-safe-l)+1.25rem)]",
                    )}
                    onSubmit={connect}
                >
                    <header className="text-center">
                        {/* Centred as a block. A centred mark over left-aligned text looks like an
                        accident; centring both makes it a header, with the fields left-aligned
                        below as usual. */}
                        <span className="logo mx-auto block w-[min(6rem,40%)]" role="img" aria-label="Dusklapse" />
                        <p className="mt-3 mb-0 text-text-muted">Connect to a camera on your network.</p>
                    </header>

                    <fieldset className="m-0 flex min-w-0 flex-col gap-2 border-0 p-0">
                        <Label asLegend>Camera</Label>
                        <SegmentedControl
                            aria-label="Camera"
                            value={vendor}
                            onChange={setVendor}
                            options={offered.map((profile) => ({ value: profile.vendor, label: profile.label }))}
                        />
                        <HintStack
                            active={vendor}
                            items={offered.map((profile) => ({ key: profile.vendor, content: profile.summary }))}
                        />
                    </fieldset>

                    {/* Kept in the layout even when it has nothing to show. The content is
                    anchored to the bottom of the screen, so a row that comes and goes shoves
                    the logo and everything above it up and down on every change of camera.
                    Reserving the space costs nothing and holds the page still. */}
                    <div className={cn("flex gap-3", !needsAddress && "invisible")} aria-hidden={!needsAddress}>
                        <TextField
                            label="Address"
                            value={host}
                            onChange={(event) => setHost(event.currentTarget.value)}
                            placeholder="192.168.1.42"
                            inputMode="decimal"
                            autoCapitalize="off"
                            autoCorrect="off"
                            spellCheck={false}
                            enterKeyHint="go"
                        />
                        <TextField
                            label="Port"
                            fieldClassName="flex-[0_0_6.5rem]"
                            value={port}
                            onChange={(event) => setPort(event.currentTarget.value)}
                            inputMode="numeric"
                        />
                    </div>

                    <div className="flex gap-2">
                        {/* The handlers sit on the wrapper rather than the button, and the button stops
                        taking pointer events while it is disabled - otherwise the gesture would be
                        unavailable exactly when it is most wanted, on a vendor with no address
                        filled in. Only this button: elsewhere a disabled button still has to show
                        the `title` that explains why. */}
                        <span className="flex flex-1" {...unlock}>
                            <Button
                                variant="primary"
                                type="submit"
                                className="w-full disabled:pointer-events-none"
                                disabled={!canSubmit}
                            >
                                {busy ? "Connecting…" : "Connect"}
                            </Button>
                        </span>
                        <Button variant="icon" aria-label="Connection help" onClick={() => setShowConnectionHelp(true)}>
                            <CircleQuestionMarkIcon />
                        </Button>
                    </div>

                    {/* {developerMode && (
                        <Notice>
                            Simulator unlocked. It runs in-process and needs no camera, and is gone again when the app
                            restarts.
                        </Notice>
                    )} */}
                    {error && <Notice variant="error">{error}</Notice>}
                </form>
            </div>
        </div>
    );
}

function SplashAnimation() {
    const [loaded, setLoaded] = useState(false);
    const { RiveComponent } = useRive({
        src: dusklapseSplash,
        stateMachine: "main",
        layout: SPLASH_LAYOUT,
        autoplay: true,
        onLoad: () => setLoaded(true),
    });

    return (
        <div className="relative h-full w-full overflow-hidden">
            <RiveComponent className="h-full w-full" />
            {!loaded && (
                <img
                    className="absolute inset-0 h-full w-full object-cover object-bottom"
                    src={dusklapseSplashPoster}
                    alt=""
                />
            )}
        </div>
    );
}

/**
 * Per-vendor connection instructions.
 *
 * The frontend counterpart to the vendor strategies in Rust: React components cannot come
 * from the backend, so this is the one place the UI is allowed to know vendor names. A
 * vendor with nothing to explain simply has no entry.
 */
const CONNECTION_HELP_COMPONENTS: Partial<Record<Vendor, () => React.ReactElement>> = {
    nikon: NikonConnectionHelp,
};

function ConnectionHelp({ vendor, onClose }: { vendor: Vendor; onClose: () => void }) {
    const Help = CONNECTION_HELP_COMPONENTS[vendor];

    return (
        <div
            className={cn(
                "flex w-full absolute inset-0 flex-col gap-2 self-stretch",
                "pt-[calc(var(--spacing-safe-t)+1.5rem)] pr-[calc(var(--spacing-safe-r)+1.5rem)] pb-[calc(var(--spacing-safe-b)+1.5rem)] pl-[calc(var(--spacing-safe-l)+1.5rem)]",
            )}
        >
            <div className="flex items-center justify-between gap-4">
                <Button size="compact" onClick={onClose}>
                    <ArrowLeftIcon className="size-4" />
                    Back
                </Button>
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto border-t border-border">
                {Help ? <Help /> : <Notice>There is nothing to set up for this camera beyond picking it.</Notice>}
            </div>
        </div>
    );
}
