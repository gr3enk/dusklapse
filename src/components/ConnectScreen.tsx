import { useEffect, useRef, useState } from "react";
import { ArrowLeftIcon, CircleQuestionMarkIcon } from "lucide-react";

import backgroundPoster from "../assets/background-poster.jpg";
import backgroundVideo from "../assets/background-loop.mp4";
import { api, errorMessage } from "../lib/api";
import type { CameraInfo, Vendor, VendorProfile } from "../lib/types";
import { cn } from "../lib/utils";
import NikonConnectionHelp from "./help/NikonConnectionHelp";
import { Button } from "./ui/Button";
import { HintStack } from "./ui/HintStack";
import { Label } from "./ui/Label";
import { Notice } from "./ui/Notice";
import { SegmentedControl } from "./ui/SegmentedControl";
import { TextField } from "./ui/TextField";

interface Props {
    onConnected: (info: CameraInfo) => void;
}

export function ConnectScreen({ onConnected }: Props) {
    const [profiles, setProfiles] = useState<VendorProfile[]>([]);
    const [vendor, setVendor] = useState<Vendor>("mock");
    const [showConnectionHelp, setShowConnectionHelp] = useState(false);
    const [host, setHost] = useState("");
    const [port, setPort] = useState("");
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const video = useRef<HTMLVideoElement | null>(null);

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

    // iOS grants autoplay only to muted inline video, and even then refuses it in a few
    // states (low power mode among them). A refusal is not a problem worth surfacing: the
    // poster frame stays, which is a perfectly good still backdrop.
    useEffect(() => {
        video.current?.play().catch(() => {
            /* Poster frame it is. */
        });
    }, []);

    const selected = profiles.find((profile) => profile.vendor === vendor);
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
        setHost(selected.accessPointHost ?? "");
        setPort(String(selected.defaultPort));
    }, [selected]);

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
        // Controls at the bottom, footage above them. Anchoring to the bottom rather than
        // reserving a fixed slice means the layout works at any height without a second set
        // of rules for short screens.
        <div className="relative flex min-h-dvh w-full flex-col justify-end self-stretch overflow-hidden">
            <div className="pointer-events-none absolute inset-0 z-0" aria-hidden="true">
                {/* All four attributes are load-bearing on iOS: without `muted` autoplay is
                    refused outright, and without `playsInline` the video is taken fullscreen
                    instead of staying in the layout.

                    Hidden for anyone who asked the system for less motion; the poster frame
                    behind it stands in. */}
                <video
                    className="h-full w-full object-cover motion-reduce:hidden"
                    ref={video}
                    src={backgroundVideo}
                    poster={backgroundPoster}
                    autoPlay
                    muted
                    loop
                    playsInline
                    preload="auto"
                    tabIndex={-1}
                />
                <div className="scrim absolute inset-0" />
            </div>

            <form
                className={cn(
                    "relative z-10 flex w-full max-w-104 flex-col gap-5 self-center",
                    "pt-8 pr-[calc(var(--spacing-safe-r)+1.25rem)] pb-[calc(var(--spacing-safe-b)+2rem)] pl-[calc(var(--spacing-safe-l)+1.25rem)]",
                )}
                onSubmit={connect}
            >
                <header className="text-center">
                    {/* Centred as a block. A centred mark over left-aligned text looks like an
                        accident; centring both makes it a header, with the fields left-aligned
                        below as usual. */}
                    <span className="logo mx-auto block w-[min(8.5rem,40%)]" role="img" aria-label="Dusklapse" />
                    <p className="mt-3 mb-0 text-text-muted">Connect to a camera on your network.</p>
                </header>

                <fieldset className="m-0 flex min-w-0 flex-col gap-2 border-0 p-0">
                    <Label asLegend>Camera</Label>
                    <SegmentedControl aria-label="Camera" value={vendor} onChange={setVendor} options={profiles.map((profile) => ({ value: profile.vendor, label: profile.label }))} />
                    <HintStack active={vendor} items={profiles.map((profile) => ({ key: profile.vendor, content: profile.summary }))} />
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
                    <TextField label="Port" fieldClassName="flex-[0_0_6.5rem]" value={port} onChange={(event) => setPort(event.currentTarget.value)} inputMode="numeric" />
                </div>

                <div className="flex gap-2">
                    <Button variant="primary" type="submit" className="flex-1" disabled={!canSubmit}>
                        {busy ? "Connecting…" : "Connect"}
                    </Button>
                    <Button variant="icon" aria-label="Connection help" onClick={() => setShowConnectionHelp(true)}>
                        <CircleQuestionMarkIcon />
                    </Button>
                </div>

                {error && <Notice variant="error">{error}</Notice>}
            </form>
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
                "flex w-full flex-col gap-6 self-stretch",
                "pt-[calc(var(--spacing-safe-t)+1.5rem)] pr-[calc(var(--spacing-safe-r)+1.5rem)] pb-[calc(var(--spacing-safe-b)+1.5rem)] pl-[calc(var(--spacing-safe-l)+1.5rem)]",
            )}
        >
            <div className="flex items-center justify-between gap-4">
                <Button size="compact" onClick={onClose}>
                    <ArrowLeftIcon className="size-4" />
                    Back
                </Button>
                <h2 className="m-0 text-[1.1rem] font-[650]">Connection help</h2>
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">{Help ? <Help /> : <Notice>There is nothing to set up for this camera beyond picking it.</Notice>}</div>
        </div>
    );
}
