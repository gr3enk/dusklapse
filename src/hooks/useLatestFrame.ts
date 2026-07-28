import { useEffect, useRef, useState } from "react";

import { api, errorMessage } from "../lib/api";
import type { PreviewInfo } from "../lib/types";

export interface LatestFrame {
    /** Metadata and measurements of the newest frame, or `null` before the first one. */
    info: PreviewInfo | null;
    /** Blob URL for the image, or `null` while it is still crossing. */
    imageUrl: string | null;
    loading: boolean;
    error: string | null;
}

/**
 * The newest frame off the camera: its image, its histogram and its brightness.
 *
 * Lifted out of the pane that displays it because the measurements have more than one
 * reader. The preview draws the histogram, the ramp controls need the brightness to offer
 * "use this frame as the reference", and the status strip wants the deviation. State that
 * several siblings read cannot live inside one of them.
 *
 * Blob URLs are not garbage collected on their own, so each one is revoked when it is
 * replaced. A long sequence would otherwise accumulate a megabyte or two per frame until
 * the WebView is killed.
 */
export function useLatestFrame(frame: number): LatestFrame {
    const [info, setInfo] = useState<PreviewInfo | null>(null);
    const [imageUrl, setImageUrl] = useState<string | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    // Held in a ref, not state: the cleanup path must see the current value without
    // re-running the effect that fetched it.
    const objectUrl = useRef<string | null>(null);

    useEffect(() => {
        // Nothing has been shot yet in this session.
        if (frame === 0) return;

        let current = true;
        setLoading(true);
        setError(null);

        // Two steps by design: the metadata and histogram come back as JSON, the pixels as
        // binary. The metadata arrives first, so the curves and the brightness are usable
        // while the image is still crossing.
        (async () => {
            const next = await api.preview();
            // Null means the camera had nothing newer than what is on screen.
            if (!current || !next) return;
            setInfo(next);

            const bytes = await api.previewImage();
            // A newer frame landed while this transfer was in flight; that fetch owns the
            // display now.
            if (!current || !bytes) return;

            if (objectUrl.current) URL.revokeObjectURL(objectUrl.current);
            objectUrl.current = URL.createObjectURL(new Blob([bytes], { type: "image/jpeg" }));
            setImageUrl(objectUrl.current);
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

    // Release the last blob when the consumer goes away.
    useEffect(() => {
        return () => {
            if (objectUrl.current) URL.revokeObjectURL(objectUrl.current);
        };
    }, []);

    return { info, imageUrl, loading, error };
}
