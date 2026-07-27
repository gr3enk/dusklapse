import { useEffect, useRef, useState } from "react";

import { api, errorMessage } from "../lib/api";

interface Props {
    /**
     * Bumped once per recorded frame. Changing it is what triggers a fetch - the
     * pane never polls, because a preview only exists after an exposure.
     */
    frame: number;
}

/**
 * Shows the JPEG from the most recent frame.
 *
 * The image arrives as raw bytes over IPC and is handed to the browser as a blob
 * URL. Blob URLs are not garbage collected on their own, so every one has to be
 * revoked when it is replaced - a long sequence would otherwise accumulate a
 * megabyte or two per frame until the WebView is killed.
 */
export function PreviewPane({ frame }: Props) {
    const [url, setUrl] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [loading, setLoading] = useState(false);
    // Held in a ref, not state: the cleanup path must see the current value without
    // re-running the effect that fetched it.
    const objectUrl = useRef<string | null>(null);

    function replace(next: string | null) {
        if (objectUrl.current) URL.revokeObjectURL(objectUrl.current);
        objectUrl.current = next;
        setUrl(next);
    }

    useEffect(() => {
        // Nothing has been shot yet in this session.
        if (frame === 0) return;

        let current = true;
        setLoading(true);
        setError(null);

        api.preview()
            .then((bytes) => {
                // A newer frame landed while this transfer was in flight; that fetch
                // owns the display now.
                if (!current) return;
                // Null means the camera had nothing newer than what is on screen.
                if (bytes) replace(URL.createObjectURL(new Blob([bytes], { type: "image/jpeg" })));
            })
            .catch((cause) => {
                if (current) setError(errorMessage(cause));
            })
            .finally(() => {
                if (current) setLoading(false);
            });

        return () => {
            current = false;
        };
    }, [frame]);

    // Release the last blob when the pane goes away.
    useEffect(() => {
        return () => {
            if (objectUrl.current) URL.revokeObjectURL(objectUrl.current);
        };
    }, []);

    return (
        <section className="preview">
            <div className="preview__frame">
                {url ? <img className="preview__image" src={url} alt={`Frame ${frame}`} /> : <p className="preview__placeholder">{frame === 0 ? "Waiting for the first frame…" : "No preview yet"}</p>}
                {loading && <span className="preview__badge">Loading…</span>}
            </div>
            {error && (
                <p className="notice notice--error" role="alert">
                    {error}
                </p>
            )}
        </section>
    );
}
