import { useEffect, useRef, useState } from "react";

import { api, errorMessage } from "../lib/api";
import type { PreviewInfo } from "../lib/types";
import { HistogramChart } from "./HistogramChart";
import { Label } from "./ui/Label";
import { Notice } from "./ui/Notice";

interface Props {
    /**
     * Bumped once per recorded frame. Changing it is what triggers a fetch - the pane never
     * polls, because a preview only exists after an exposure.
     */
    frame: number;
    /** Whether this camera reports new frames at all. Only changes the wording. */
    supported: boolean;
}

/**
 * Shows the JPEG from the most recent frame, with its histogram and brightness over it.
 *
 * The image arrives as raw bytes over IPC and is handed to the browser as a blob URL. Blob
 * URLs are not garbage collected on their own, so every one has to be revoked when it is
 * replaced - a long sequence would otherwise accumulate a megabyte or two per frame until
 * the WebView is killed.
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

        // Two steps by design: the metadata and histogram come back as JSON, the pixels as
        // binary. The metadata arrives first, so the curves can already be drawn while the
        // image is still crossing.
        (async () => {
            const next = await api.preview();
            // Null means the camera had nothing newer than what is on screen.
            if (!current || !next) return;
            setInfo(next);

            const bytes = await api.previewImage();
            // A newer frame landed while this transfer was in flight; that fetch owns the
            // display now.
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
        <section className="flex h-full min-h-0 flex-col gap-2" aria-label="Latest frame">
            {/* Fills the grid cell rather than imposing an aspect ratio: the cell's shape
                already differs between orientations, and a fixed ratio would fight it. Black
                rather than the surface colour, because this is a photograph and a grey
                surround biases how you judge its exposure. */}
            <div className="relative flex min-h-0 flex-1 items-center justify-center overflow-hidden rounded-card border border-border bg-black">
                {url ? (
                    // Contain, never cover - a cropped preview would hide exactly the blown
                    // highlights you are checking for.
                    <img className="h-full w-full object-contain" src={url} alt={`Frame ${frame}`} />
                ) : (
                    <p className="m-0 p-4 text-center text-[0.9rem] text-text-muted">{placeholder(supported, frame)}</p>
                )}

                {loading && <span className="absolute top-[0.6rem] right-[0.6rem] rounded-full bg-black/60 px-2 py-[0.2rem] text-[0.75rem]">Loading…</span>}

                {/* Overlaid rather than placed beside the image: the two are read together,
                    and giving the histogram its own row would take height from the frame it
                    describes. Bottom-left, where a photograph carries least of its subject. */}
                {info?.analysis && (
                    <div className="pointer-events-none absolute bottom-[0.6rem] left-[0.6rem] flex h-[min(11.5rem,42%)] w-[min(18rem,50%)] flex-col gap-[0.3rem] rounded-lg border border-white/15 bg-black/55 p-[0.35rem] backdrop-blur-sm">
                        <div className="flex items-baseline justify-between gap-2">
                            <Label className="text-[0.6rem] tracking-[0.08em]">Luminance</Label>
                            {/* The number the ramp is regulated against, so it is the one you
                                glance at. Tabular figures keep it from jittering sideways. */}
                            <span className="text-base font-[650] leading-none tabular-nums text-text">{info.analysis.luminance.value}</span>
                        </div>
                        <HistogramChart histogram={info.analysis.histogram} />
                    </div>
                )}

                {info && (
                    <span className="pointer-events-none absolute bottom-[0.6rem] right-[0.6rem] rounded-full bg-black/55 px-2 py-[0.2rem] text-[0.7rem] tabular-nums text-text-muted">
                        {info.filename}
                    </span>
                )}
            </div>

            {error && <Notice variant="error">{error}</Notice>}
        </section>
    );
}

function placeholder(supported: boolean, frame: number): string {
    // Distinguishing these matters: "waiting" invites you to keep waiting, and on a body
    // that will never send a frame that would be a lie.
    if (!supported) return "This camera does not report new frames.";
    return frame === 0 ? "Waiting for the first frame…" : "No preview yet";
}
