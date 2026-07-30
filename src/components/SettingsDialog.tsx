import type { Ramp } from "../hooks/useRamp";
import type { Settings } from "../hooks/useSettings";
import { TRANSFER_EVERY_MAX, TRANSFER_EVERY_MIN, TWILIGHT_BANDS, type TwilightBand } from "../lib/types";
import { Label } from "./ui/Label";
import { Modal } from "./ui/Modal";
import { Notice } from "./ui/Notice";
import NumberSelector from "./ui/NumberSelector";
import { SegmentedControl } from "./ui/SegmentedControl";

interface Props {
    open: boolean;
    onClose: () => void;
    /** The stored settings and the write-through that changes them. */
    settings: Settings;
    /**
     * The ramp configuration, for the twilight band.
     *
     * The band belongs to the daylight curve and is stored with it, but it is set once for a place
     * and a season rather than tuned per shoot - so it lives here rather than taking up room in
     * the working screen beside the factor and the shape.
     */
    ramp: Ramp;
    /** Measured interval between shots in milliseconds, for the readout. `null` before two shots. */
    intervalMs: number | null;
}

/**
 * Secondary settings - the ones that shape a session rather than an exposure.
 *
 * Behind a button and a modal on purpose: the working screen has room for the controls used every
 * few minutes and nothing else, and on a phone in portrait that is already tight.
 */
export function SettingsDialog({ open, onClose, settings, ramp, intervalMs }: Props) {
    const { value, update, saving, error } = settings;
    // Disabled rather than hidden until the stored value arrives, so the dialog does not change
    // height the moment it does.
    const transferEvery = value?.transferEvery ?? 1;
    const band = ramp.settings?.daylight.twilight ?? "astronomical";

    return (
        <Modal open={open} onClose={onClose} title="Settings">
            <section className="flex flex-col gap-2" aria-label="Frame transfer">
                <Label>Transfer one frame in</Label>

                <NumberSelector
                    label="frames per transfer"
                    value={transferEvery}
                    onChange={(transferEvery) => update({ transferEvery })}
                    disabled={value === null || saving}
                    min={TRANSFER_EVERY_MIN}
                    max={TRANSFER_EVERY_MAX}
                    step={1}
                />

                <p className="m-0 text-sm tabular-nums opacity-60">{describeTransfer(transferEvery, intervalMs)}</p>

                <p className="m-0 text-sm opacity-60">
                    Every frame still counts towards the shot total and the interval. Only transferred frames are
                    measured, so the luminance reading and the ramp step on those.
                </p>

                {error && <Notice variant="error">{error}</Notice>}
            </section>

            <section className="flex flex-col gap-2" aria-label="Twilight">
                <Label>Fully dark / light at</Label>

                <SegmentedControl
                    aria-label="Twilight band"
                    options={TWILIGHT_BANDS.map(({ value, label }) => ({ value, label }))}
                    value={band}
                    onChange={(twilight) =>
                        ramp.settings && ramp.update({ daylight: { ...ramp.settings.daylight, twilight } })
                    }
                />

                <p className="m-0 text-sm tabular-nums opacity-60">{describeBand(band)}</p>

                <p className="m-0 text-sm opacity-60">
                    This decides <em>when</em> Auto Luminance finishes, not how far it goes. Above about 49° north the
                    sun does not reach −18° at all around midsummer, so a curve floored at astronomical dusk never
                    applies the whole factor in those weeks.
                </p>

                {ramp.error && <Notice variant="error">{ramp.error}</Notice>}
            </section>
        </Modal>
    );
}

/**
 * What the setting costs and what it buys, in the units on screen.
 *
 * The number alone does not say the thing that matters, which is how long the ramp now goes
 * between corrections - and at a short interval that can be fine while at a long one it is not.
 */
function describeTransfer(every: number, intervalMs: number | null): string {
    const cadence =
        every === 1
            ? "Every frame is transferred."
            : `Every ${ordinal(every)} frame is transferred, so ${every - 1} in ${every} never cross the network.`;

    if (intervalMs === null) return cadence;

    const seconds = (intervalMs * every) / 1000;
    const measured = seconds < 10 ? seconds.toFixed(1) : Math.round(seconds).toString();
    return `${cadence} A measurement every ${measured}s at the current interval.`;
}

/** What the chosen band means in the units an almanac uses. */
function describeBand(band: TwilightBand): string {
    const entry = TWILIGHT_BANDS.find((option) => option.value === band);
    if (!entry) return "";
    return `${entry.label} twilight: the sun ${entry.degrees}° below the horizon.`;
}

function ordinal(value: number): string {
    if (value === 2) return "2nd";
    if (value === 3) return "3rd";
    return `${value}th`;
}
