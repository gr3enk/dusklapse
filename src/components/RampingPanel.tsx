import type { CameraInfo } from "../lib/types";
import { Button } from "./ui/Button";
import { Notice } from "./ui/Notice";
import { Panel } from "./ui/Panel";

interface Props {
    info: CameraInfo;
    busy: boolean;
    onShoot: () => void;
    onRefresh: () => void;
}

/**
 * Where the holy-grail ramp will be configured.
 *
 * Scrolls on its own rather than growing the page: in landscape it is a full-height column
 * next to the preview, and a ramp with keyframes will be taller than any screen. Its own
 * scroll container is what keeps the preview and the status strip fixed in place while you
 * work down a long list of settings.
 *
 * The ramping controls themselves are not built yet. What is here are the actions that
 * genuinely work today; the rest is named so the shape of the screen is settled before the
 * engine lands behind it.
 */
export function RampingPanel({ info, busy, onShoot, onRefresh }: Props) {
    return (
        <Panel
            // `overscroll-contain` stops this list handing its scroll on to the page when
            // it reaches the end.
            className="flex h-full flex-col gap-4 overflow-y-auto overscroll-contain p-[0.9rem]"
            aria-label="Timelapse ramping"
        >
            <h2 className="m-0 text-[1.1rem] font-[650]">Ramping</h2>

            <div className="flex flex-wrap gap-[0.6rem]">
                {info.supportsRelease ? (
                    <Button variant="primary" onClick={onShoot} disabled={busy}>
                        {busy ? "Working…" : "Take a frame"}
                    </Button>
                ) : (
                    // Offering a button that is guaranteed to fail is worse than explaining
                    // why there is none.
                    <Notice className="flex-[1_1_16rem]">
                        This body takes no remote release over Wi-Fi. Frame timing comes from your intervalometer; Dusklapse ramps the exposure between frames.
                    </Notice>
                )}
                <Button onClick={onRefresh} disabled={busy}>
                    Re-read camera
                </Button>
            </div>

            <div className="text-[0.9rem] text-text-muted">
                <h3 className="m-0 mb-[0.4rem] text-[0.75rem] font-semibold uppercase tracking-[0.06em]">Not built yet</h3>
                <ul className="m-0 list-disc space-y-1 pl-[1.1rem]">
                    <li>Exposure keyframes over the sequence</li>
                    <li>Auto ramping from the preview histogram</li>
                    <li>Which dials the ramp may move, and in what order</li>
                    <li>Interval and frame count</li>
                </ul>
            </div>
        </Panel>
    );
}
