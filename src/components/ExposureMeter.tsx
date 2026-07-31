import { cn } from "../lib/utils";

/**
 * How far the scale runs either side of centre, in stops.
 *
 * Two, the same as a camera's own meter. A ramp that is more than two stops out has a problem the
 * scale cannot express anyway, and a wider range would squeeze the part that is read constantly -
 * the tenths around zero - into nothing.
 */
const RANGE = 2;

/**
 * The finest correction the ramp can make, and therefore the width of "on target".
 *
 * A third of a stop is one notch on the dials this app drives. Inside that there is nothing the
 * ramp could do about the difference even if it wanted to, so it is not worth alarming anyone.
 */
const ONE_NOTCH = 1 / 3;

interface Props {
    /** Stops the frame is from the target. Positive is brighter. `null` before the first frame. */
    stops: number | null;
    className?: string;
}

/**
 * A camera's exposure meter: how far the last frame sat from the target.
 *
 * The same number the control panel prints as "0.83 EV over", drawn instead of spelled out. On the
 * working screen it is the one reading looked at continuously, and a needle is read at a glance
 * where a signed decimal has to be parsed.
 *
 * The scale stays put with no frame yet rather than appearing later - a strip this dense should not
 * change height the moment a sequence starts.
 */
export function ExposureMeter({ stops, className }: Props) {
    // Clamped so the needle stays on the scale. It sits on the end mark, which reads as "at least
    // this far" - the number beside it says how much further.
    const position = stops === null ? null : (Math.max(-RANGE, Math.min(RANGE, stops)) / RANGE) * 50 + 50;

    const magnitude = stops === null ? 0 : Math.abs(stops);
    const tone =
        stops === null
            ? "text-text-muted"
            : magnitude <= ONE_NOTCH
              ? "text-alert-success"
              : magnitude <= 1
                ? "text-alert-warning"
                : "text-alert-error";

    return (
        <div
            className={cn("flex min-w-0 flex-col items-center gap-[0.15rem]", className)}
            role="meter"
            aria-valuemin={-RANGE}
            aria-valuemax={RANGE}
            aria-valuenow={stops ?? 0}
            aria-valuetext={
                stops === null
                    ? "No frame measured yet"
                    : `${stops > 0 ? "+" : ""}${stops.toFixed(2)} EV from the target`
            }
            title="How far the last frame sat from the luminance reference"
        >
            <div className="flex items-baseline gap-1 text-[0.7rem] leading-none text-text-muted">
                <span aria-hidden>−</span>
                <span className={cn("tabular-nums font-semibold", tone)}>
                    {stops === null ? "– –" : `${stops > 0 ? "+" : ""}${stops.toFixed(2)} EV`}
                </span>
                <span aria-hidden>+</span>
            </div>

            <div className="relative h-[0.7rem] w-full min-w-[6rem]" aria-hidden>
                {/* Whole stops tall, thirds short: thirds are the grid the dials actually move on,
                    so they are the marks a correction lands between. */}
                <div className="absolute inset-0 flex items-end justify-between">
                    {ticks().map((tick) => (
                        <span
                            key={tick.at}
                            className={cn(
                                "w-px bg-border",
                                tick.major ? "h-full" : "h-1/2",
                                tick.at === 0 && "bg-text-muted",
                            )}
                        />
                    ))}
                </div>

                {position !== null && (
                    // Translated by half its own width so the needle is centred on its value rather
                    // than starting there - at the ends of the scale that is the difference between
                    // sitting on the mark and hanging off it.
                    <span
                        className={cn(
                            "absolute bottom-0 h-full w-[2px] -translate-x-1/2 rounded-full bg-current",
                            tone,
                        )}
                        style={{ left: `${position}%` }}
                    />
                )}
            </div>
        </div>
    );
}

/** One mark per third of a stop across the scale, the whole stops taller. */
function ticks(): { at: number; major: boolean }[] {
    const marks: { at: number; major: boolean }[] = [];
    for (let step = -RANGE * 3; step <= RANGE * 3; step++) {
        const at = step / 3;
        marks.push({ at, major: Number.isInteger(at) });
    }
    return marks;
}
