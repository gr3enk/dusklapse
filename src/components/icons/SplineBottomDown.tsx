import { createLucideIcon } from "lucide-react";

const __iconNode: Parameters<typeof createLucideIcon>[1] = [
    ["circle", { cx: "5", cy: "5", r: "2", key: "…" }],
    ["circle", { cx: "19", cy: "19", r: "2", key: "…" }],
    ["path", { d: "M5 7A12 12 0 0 0 17 19", key: "1okkup" }],
];
const Spline = createLucideIcon("spline", __iconNode);

export { __iconNode, Spline as default };
