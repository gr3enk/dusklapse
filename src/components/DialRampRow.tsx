import { MinusIcon, PlusIcon } from "lucide-react";
import type { DialRamp, Dial, ExposureValue } from "../lib/types";
import { Button } from "./ui/Button";
import { Select } from "./ui/Select";
import Toggle from "./ui/Toggle";
import { useCallback } from "react";

interface Props {
    dial: Dial;
    label: string;
    config: DialRamp;
    /** What the camera currently offers on this dial. */
    values: ExposureValue[];
    /** False while the ramp as a whole is disarmed. */
    rampActive: boolean;
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
export function DialRampRow({ label, config, values, rampActive, onChange }: Props) {
    const usable = values.filter((value) => value.stops !== null);
    const disabled = !rampActive || !config.enabled;

    const handleChangeLimit = useCallback(
        (direction: "up" | "down") => {
            if (config.limit) {
                const currentIndex = values.findIndex((value) => value.raw === config.limit);
                const newIndex = direction === "up" ? currentIndex + 1 : currentIndex - 1;
                const newLimit = values[newIndex]?.raw;
                onChange({ ...config, limit: newLimit });
            } else {
                const newLimit = values[0]?.raw;
                onChange({ ...config, limit: newLimit });
            }
        },
        [config.limit, values, onChange],
    );

    return (
        <div className="flex min-w-0 flex-col gap-2">
            {/* The switch sits with the caption it governs rather than in the row of
                controls. The caption's line has width to spare at every size; the controls'
                line has none to spare in a landscape column on a phone, and the switch was
                taking the width the dropdown needs to show which value is set. */}
            <div className="flex min-w-0 items-center justify-between gap-2">
                <span className="truncate">{label}</span>
                <Toggle
                    disabled={!rampActive}
                    checked={config.enabled}
                    onChange={(enabled) => onChange({ ...config, enabled })}
                />
            </div>
            {/* The dropdown is the one part that may shrink; a nudge button drawn smaller
                than its tap target is not one you can hit. */}
            <div className="flex min-w-0 items-center gap-2">
                <Button
                    disabled={disabled}
                    onClick={() => handleChangeLimit("down")}
                    variant="secondary"
                    size="compact"
                    className="shrink-0"
                >
                    <MinusIcon className="size-4" />
                </Button>
                <Select
                    label={label}
                    // The caption on the line above already names this; repeating it over the
                    // select would say the same thing twice.
                    hideLabel
                    value={config.limit ?? undefined}
                    emptyLabel="Not set"
                    allowEmpty
                    className="w-full"
                    fieldClassName="min-w-0 flex-1"
                    disabled={disabled || usable.length === 0}
                    onChange={(event) => onChange({ ...config, limit: event.currentTarget.value || null })}
                    options={usable.map((value) => ({ value: value.raw, label: value.label }))}
                />
                <Button
                    disabled={disabled}
                    onClick={() => handleChangeLimit("up")}
                    variant="secondary"
                    size="compact"
                    className="shrink-0"
                >
                    <PlusIcon className="size-4" />
                </Button>
            </div>
        </div>
    );
}
