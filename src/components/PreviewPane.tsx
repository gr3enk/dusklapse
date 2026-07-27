import { useEffect, useRef, useState } from "react";

import { api, errorMessage } from "../lib/api";
import type { PreviewInfo } from "../lib/types";
import { HistogramChart } from "./HistogramChart";

interface Props {
    /**
     * Bumped once per recorded frame. Changing it is what triggers a fetch - the
     * pane never polls, because a preview only exists after an exposure.
     */
    frame: number;
    /** Whether this camera reports new frames at all. Only changes the wording. */
    supported: boolean;
}

/**
 * Shows the JPEG from the most recent frame.
 *
 * The image arrives as raw bytes over IPC and is handed to the browser as a blob
 * URL. Blob URLs are not garbage collected on their own, so every one has to be
 * revoked when it is replaced - a long sequence would otherwise accumulate a
 * megabyte or two per frame until the WebView is killed.
 */
export function PreviewPane({ frame, supported }: Props) {
    const [url, setUrl] = useState<string | null>(null);
    const [info, setInfo] = useState<PreviewInfo | null>(null);
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

        // Two steps by design: the metadata and histogram come back as JSON, the
        // pixels as binary. The metadata arrives first, so the curves can already be
        // drawn while the image is still crossing.
        (async () => {
            const next = await api.preview();
            // Null means the camera had nothing newer than what is on screen.
            if (!current || !next) return;
            setInfo(next);

            const bytes = await api.previewImage();
            // A newer frame landed while this transfer was in flight; that fetch owns
            // the display now.
            if (!current || !bytes) return;
            replace(URL.createObjectURL(new Blob([bytes], { type: "image/jpeg" })));
        })()
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
        <section className="preview" aria-label="Latest frame">
            <div className="preview__frame">
                {url ? <img className="preview__image" src={url} alt={`Frame ${frame}`} /> : <p className="preview__placeholder">{placeholder(supported, frame)}</p>}
                {loading && <span className="preview__badge">Loading…</span>}
                {/* Overlaid rather than placed beside the image: the two are read
                    together, and giving the histogram its own row would take height
                    from the frame it describes. */}
                {info?.analysis && (
                    <div className="preview__histogram">
                        {/* Above the curves and larger than anything else here: this is
                            the number the ramp is regulated against, so it is the one
                            you glance at, not something to hunt for. */}
                        <div className="preview__luminance">
                            <span className="preview__luminance-label">Luminance</span>
                            <span className="preview__luminance-value">{info.analysis.luminance.value}</span>
                        </div>
                        <HistogramChart histogram={info.analysis.histogram} />
                    </div>
                )}
                {info && <span className="preview__caption">{info.filename}</span>}
            </div>
            {error && (
                <p className="notice notice--error" role="alert">
                    {error}
                </p>
            )}
        </section>
    );
}

function placeholder(supported: boolean, frame: number): string {
    // Distinguishing these matters: "waiting" invites you to keep waiting, and on a
    // body that will never send a frame that would be a lie.
    if (!supported) return "This camera does not report new frames.";
    return frame === 0 ? "Waiting for the first frame…" : "No preview yet";
}
