import type { CameraInfo } from "../lib/types";

interface Props {
    info: CameraInfo;
    busy: boolean;
    onShoot: () => void;
    onRefresh: () => void;
}

/**
 * Where the holy-grail ramp will be configured.
 *
 * Scrolls on its own rather than growing the page: in landscape it is a full-height
 * column next to the preview, and a ramp with keyframes will be taller than any
 * screen. Its own scroll container is what keeps the preview and the status strip
 * fixed in place while you work down a long list of settings.
 *
 * The ramping controls themselves are not built yet. What is here are the actions
 * that genuinely work today; the rest is named so the shape of the screen is
 * settled before the engine lands behind it.
 */
export function RampingPanel({ info, busy, onShoot, onRefresh }: Props) {
    return (
        <section className="ramping" aria-label="Timelapse ramping">
            <header className="ramping__header">
                <h2>Ramping</h2>
            </header>

            <div className="ramping__actions">
                {info.supportsRelease ? (
                    <button className="button button--primary" type="button" onClick={onShoot} disabled={busy}>
                        {busy ? "Working…" : "Take a frame"}
                    </button>
                ) : (
                    // Offering a button that is guaranteed to fail is worse than
                    // explaining why there is none.
                    <p className="notice notice--info">This body takes no remote release over Wi-Fi. Frame timing comes from your intervalometer; Dusklapse ramps the exposure between frames.</p>
                )}
                <button className="button" type="button" onClick={onRefresh} disabled={busy}>
                    Re-read camera
                </button>
            </div>

            <div className="ramping__pending">
                <h3>Not built yet</h3>
                <ul>
                    <li>Exposure keyframes over the sequence</li>
                    <li>Auto ramping from the preview histogram</li>
                    <li>Which dials the ramp may move, and in what order</li>
                    <li>Interval and frame count</li>
                </ul>
            </div>
        </section>
    );
}
