import { useEffect, useState } from "react";

import { api, errorMessage } from "../lib/api";
import { ACCESS_POINT_HOST, VENDORS, type CameraInfo, type Vendor } from "../lib/types";

interface Props {
    onConnected: (info: CameraInfo) => void;
}

export function ConnectScreen({ onConnected }: Props) {
    const [vendor, setVendor] = useState<Vendor>("mock");
    const [host, setHost] = useState("");
    const [port, setPort] = useState("");
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);

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

    const selected = VENDORS.find((entry) => entry.id === vendor);

    return (
        <form className="connect" onSubmit={connect}>
            <header className="connect__header">
                <h1>Dusklapse</h1>
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
                {selected && <p className="field__hint">{selected.hint}</p>}
            </fieldset>

            {needsAddress && (
                <div className="field-row">
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
            )}

            <button className="button button--primary" type="submit" disabled={!canSubmit}>
                {busy ? "Connecting…" : "Connect"}
            </button>

            {error && (
                <p className="notice notice--error" role="alert">
                    {error}
                </p>
            )}

            <p className="connect__footnote">
                Let the camera host its own Wi-Fi and join that network with this device. On Nikon use <strong>connect to smart device</strong>, not connect to computer - the computer path wants to be
                paired with Nikon's transmitter utility and drops the connection as soon as you leave its pairing screen.
            </p>
        </form>
    );
}
