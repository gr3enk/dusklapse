import type { DialRamp, Dial, ExposureValue } from "../lib/types";
import { Select } from "./ui/Select";
import Toggle from "./ui/Toggle";

interface Props {
    dial: Dial;
    label: string;
    config: DialRamp;
    /** What the camera currently offers on this dial. */
    values: ExposureValue[];
    /** False while the ramp as a whole is disarmed. */
    rampActive: boolean;
    busy: boolean;
    onChange: (next: DialRamp) => void;
}

/**
 * One dial's ramp settings: whether it may be moved, and how far.
 *
 * The dropdown offers only values with a fixed brightness. `bulb` and `auto` have none, so a
 * ramp cannot reason about them and `nearest` already refuses to pick them - offering one as
 * a limit would be offering a setting the engine must then ignore.
 *
 * The select stays mounted and disabled rather than hidden when the dial is off, so the panel
 * does not change height every time a toggle is flipped.
 */
export function DialRampRow({ label, config, values, rampActive, busy, onChange }: Props) {
    const usable = values.filter((value) => value.stops !== null);
    const disabled = !rampActive || !config.enabled || busy;

    return (
        <div className="flex flex-col gap-2">
            <span>{label}</span>
            <div className="flex items-center gap-2">
                <Toggle disabled={!rampActive} checked={config.enabled} onChange={(enabled) => onChange({ ...config, enabled })} />

                <Select
                    label={label}
                    // The label is already above the toggle; repeating it over the select would
                    // say the same thing twice.
                    hideLabel
                    value={config.limit ?? undefined}
                    emptyLabel="Not set"
                    allowEmpty
                    className="flex-1"
                    disabled={disabled || usable.length === 0}
                    onChange={(event) => onChange({ ...config, limit: event.currentTarget.value || null })}
                    options={usable.map((value) => ({ value: value.raw, label: value.label }))}
                />
            </div>
        </div>
    );
}
