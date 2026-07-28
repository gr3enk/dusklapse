import { ClassValue } from "clsx";
import { BatteryFullIcon, BatteryIcon, BatteryLowIcon, BatteryMediumIcon, BatteryWarningIcon } from "lucide-react";

export default function DynamicBatteryIcon({ className, value }: { className?: ClassValue; value: number }) {
    if (value === -1) return <BatteryWarningIcon className={className as string} />;
    if (value >= 85) return <BatteryFullIcon className={className as string} />;
    if (value >= 50) return <BatteryMediumIcon className={className as string} />;
    if (value >= 15) return <BatteryLowIcon className={className as string} />;
    return <BatteryIcon className={className as string} />;
}
