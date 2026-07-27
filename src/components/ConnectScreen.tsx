import { useEffect, useRef, useState } from "react";

import backgroundPoster from "../assets/background-poster.jpg";
import backgroundVideo from "../assets/background-loop.mp4";
import { api, errorMessage } from "../lib/api";
import { ACCESS_POINT_HOST, VENDORS, type CameraInfo, type Vendor } from "../lib/types";
import { ArrowLeftIcon, CircleQuestionMarkIcon } from "lucide-react";
import NikonConnectionHelp from "./help/NikonConnectionHelp";
interface Props {
    onConnected: (info: CameraInfo) => void;
}

export function ConnectScreen({ onConnected }: Props) {
    const [vendor, setVendor] = useState<Vendor>("mock");
    const [showConnectionHelp, setShowConnectionHelp] = useState(false);
    const [host, setHost] = useState("");
    const [port, setPort] = useState("");
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const video = useRef<HTMLVideoElement | null>(null);

    // The port lives in Rust so there is one source of truth for it.
    useEffect(() => {
        let current = true;
        api.defaultPort(vendor)
            .then((value) => {
                if (current) setPort(String(value));
            })
            .catch(() => {
                /* Leave whatever is in the field; the user can type one. */
            });
        return () => {
            current = false;
        };
    }, [vendor]);

    // Prefill the address a camera takes when it hosts its own network. That is the
    // mode that works in the field, and there is only ever one device on it, so
    // making someone type the address would be friction for nothing.
    useEffect(() => {
        setHost(ACCESS_POINT_HOST[vendor] ?? "");
    }, [vendor]);

    // iOS grants autoplay only to muted inline video, and even then refuses it in a few
    // states (low power mode among them). A refusal is not a problem worth surfacing:
    // the poster frame stays, which is a perfectly good still backdrop.
    useEffect(() => {
        video.current?.play().catch(() => {
            /* Poster frame it is. */
        });
    }, []);

    const needsAddress = vendor !== "mock";
    const canSubmit = !busy && (!needsAddress || host.trim().length > 0);

    async function connect(event: React.FormEvent) {
        event.preventDefault();
        setBusy(true);
        setError(null);
        try {
            const info = await api.connect({
                vendor,
                host: needsAddress ? host.trim() : "mock",
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
        <div className="landing">
            <div className="landing__backdrop" aria-hidden="true">
                {/* All four attributes are load-bearing on iOS: without `muted` autoplay
                    is refused outright, and without `playsInline` the video is taken
                    fullscreen instead of staying in the layout. */}
                <video className="landing__video" ref={video} src={backgroundVideo} poster={backgroundPoster} autoPlay muted loop playsInline preload="auto" tabIndex={-1} />
                {/* Fades the footage into the page colour so the controls below sit on
                    flat background rather than on moving footage, which would make them
                    hard to read and harder to ignore. */}
                <div className="landing__scrim" />
            </div>

            <form className="landing__content connect" onSubmit={connect}>
                <header className="connect__header">
                    {/* A mask, not an <img>: the file's strokes are hardcoded black and
                        would vanish on this background. Masking discards its colours and
                        paints the shape in the current text colour, so the SVG stays the
                        single source and the theme decides how it looks. */}
                    <span className="logo" role="img" aria-label="Dusklapse" />
                    <p>Connect to a camera on your network.</p>
                </header>

                <fieldset className="field">
                    <legend>Camera</legend>
                    <div className="segmented">
                        {VENDORS.map((entry) => (
                            <button key={entry.id} type="button" className="segmented__option" aria-pressed={vendor === entry.id} onClick={() => setVendor(entry.id)}>
                                {entry.label}
                            </button>
                        ))}
                    </div>
                    {/* Every hint is rendered, all in the same grid cell, with the
                        inactive ones hidden. The box therefore always has the height of
                        the longest one and picking a camera cannot resize it. A
                        min-height in `em` would need a magic number and would still be
                        wrong at some width, because how many lines a hint wraps to
                        depends on the screen. */}
                    <div className="field__hint hint-stack">
                        {VENDORS.map((entry) => (
                            <span key={entry.id} className="hint-stack__item" aria-hidden={entry.id !== vendor} data-active={entry.id === vendor}>
                                {entry.hint}
                            </span>
                        ))}
                    </div>
                </fieldset>

                {/* Kept in the layout even when it has nothing to show. The content is
                    anchored to the bottom of the screen, so a row that comes and goes
                    shoves the logo and everything above it up and down on every change
                    of camera. Reserving the space costs nothing and holds the page
                    still. */}
                <div className="field-row" aria-hidden={!needsAddress} data-reserved={!needsAddress}>
                    <label className="field">
                        <span className="field__label">Address</span>
                        <input
                            value={host}
                            onChange={(event) => setHost(event.currentTarget.value)}
                            placeholder="192.168.1.42"
                            inputMode="decimal"
                            autoCapitalize="off"
                            autoCorrect="off"
                            spellCheck={false}
                            enterKeyHint="go"
                        />
                    </label>
                    <label className="field field--narrow">
                        <span className="field__label">Port</span>
                        <input value={port} onChange={(event) => setPort(event.currentTarget.value)} inputMode="numeric" />
                    </label>
                </div>

                <div className="flex gap-2">
                    <button style={{ flex: 1 }} className="button button--primary" type="submit" disabled={!canSubmit}>
                        {busy ? "Connecting…" : "Connect"}
                    </button>
                    <button className="button button--primary button--icon" type="button" onClick={() => setShowConnectionHelp(true)}>
                        <CircleQuestionMarkIcon />
                    </button>
                </div>

                {error && (
                    <p className="notice notice--error" role="alert">
                        {error}
                    </p>
                )}

                <p className="connect__footnote">
                    Let the camera host its own Wi-Fi and join that network with this device. On Nikon use <strong>connect to smart device</strong>, not connect to computer - the computer path wants
                    to be paired with Nikon's transmitter utility and drops the connection as soon as you leave its pairing screen.
                </p>
            </form>
        </div>
    );
}

const CONNECTION_HELP_COMPONENTS = {
    nikon: NikonConnectionHelp,
} as const;

function ConnectionHelp({ vendor, onClose }: { vendor: Vendor; onClose: () => void }) {
    const ConnectionHelpComponent = CONNECTION_HELP_COMPONENTS[vendor as keyof typeof CONNECTION_HELP_COMPONENTS];
    return (
        <div className="absolute inset-0 p-16 justify-start items-center">
            <div className="flex justify-between items-center">
                <button className="flex items-center gap-2" onClick={onClose}>
                    <ArrowLeftIcon />
                    Back
                </button>
                <h2>Connection Help</h2>
            </div>
            <div className="py-8">
                <ConnectionHelpComponent />
            </div>
        </div>
    );
}
