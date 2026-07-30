import { LogLevel } from "@tauri-apps/plugin-log";
import { CheckIcon, CopyIcon, Trash2Icon } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { useLogs } from "../hooks/useLogs";
import { clearLogs, type LogEntry } from "../lib/logStore";
import { cn } from "../lib/utils";
import { Button } from "./ui/Button";
import { Modal } from "./ui/Modal";

/** What each level is called and how it is coloured. */
const LEVELS: Record<LogLevel, { label: string; className: string }> = {
    [LogLevel.Trace]: { label: "TRACE", className: "text-text-muted opacity-60" },
    [LogLevel.Debug]: { label: "DEBUG", className: "text-text-muted" },
    [LogLevel.Info]: { label: "INFO", className: "text-alert-info" },
    [LogLevel.Warn]: { label: "WARN", className: "text-alert-warning" },
    [LogLevel.Error]: { label: "ERROR", className: "text-alert-error" },
};

interface Props {
    open: boolean;
    onClose: () => void;
}

/**
 * Everything the backend has logged this session.
 *
 * The reason it exists: on a tablet in a field there is no console to attach to, and the lines
 * that explain a dropped connection are written exactly when nobody can read them.
 *
 * Wider than the other dialogs, because these lines are long and wrapping them at every camera
 * command would make the sequence impossible to follow.
 */
export function LogDialog({ open, onClose }: Props) {
    const entries = useLogs();

    return (
        <Modal open={open} onClose={onClose} title="Log" className="w-[min(52rem,94vw)]">
            <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-sm tabular-nums text-text-muted">
                    {entries.length === 0
                        ? "Nothing logged yet."
                        : `${entries.length} ${entries.length === 1 ? "line" : "lines"}`}
                </span>
                <div className="flex gap-2">
                    <CopyButton entries={entries} />
                    <Button size="compact" onClick={clearLogs} disabled={entries.length === 0}>
                        <Trash2Icon className="size-4" />
                        Clear
                    </Button>
                </div>
            </div>

            <LogList entries={entries} open={open} />
        </Modal>
    );
}

/**
 * The lines, newest at the bottom, following along while they arrive.
 *
 * Only follows while the view is already at the bottom. Scrolling up is how you read something
 * that went past, and yanking the view back down every time a frame lands would make that
 * impossible.
 */
function LogList({ entries, open }: { entries: LogEntry[]; open: boolean }) {
    const list = useRef<HTMLDivElement | null>(null);
    const pinned = useRef(true);

    useEffect(() => {
        const element = list.current;
        if (!element || !pinned.current) return;
        element.scrollTop = element.scrollHeight;
    }, [entries, open]);

    return (
        <div
            ref={list}
            onScroll={(event) => {
                const { scrollTop, scrollHeight, clientHeight } = event.currentTarget;
                // A few pixels of slack: a list scrolled to the end is rarely at exactly zero.
                pinned.current = scrollHeight - scrollTop - clientHeight < 24;
            }}
            className="h-[min(28rem,60dvh)] overflow-y-auto overscroll-contain rounded-card border border-border bg-bg p-2 font-mono text-[0.75rem] leading-relaxed"
        >
            {entries.length === 0 ? (
                <p className="m-0 p-2 text-text-muted">Lines from the backend appear here as they are written.</p>
            ) : (
                entries.map((entry) => {
                    const level = LEVELS[entry.level] ?? LEVELS[LogLevel.Info];
                    return (
                        <div key={entry.id} className="flex gap-2 py-[0.1rem]">
                            <span className="shrink-0 tabular-nums text-text-muted">{time(entry.at)}</span>
                            <span className={cn("w-11 shrink-0 font-semibold", level.className)}>{level.label}</span>
                            {/* `break-all`, not `truncate`: a long line is usually a camera reply, and
                                the interesting part is as often at the end as at the start. */}
                            <span className="min-w-0 break-all text-text">{entry.message}</span>
                        </div>
                    );
                })
            )}
        </div>
    );
}

/**
 * Puts the whole log on the clipboard.
 *
 * The point of a log on a device with no console: getting it somewhere it can be read, pasted into
 * an issue, or sent to someone.
 */
function CopyButton({ entries }: { entries: LogEntry[] }) {
    const [copied, setCopied] = useState(false);

    useEffect(() => {
        if (!copied) return;
        const timer = window.setTimeout(() => setCopied(false), 2000);
        return () => window.clearTimeout(timer);
    }, [copied]);

    return (
        <Button
            size="compact"
            disabled={entries.length === 0}
            onClick={() => {
                const text = entries
                    .map((entry) => `${time(entry.at)} ${LEVELS[entry.level]?.label ?? ""} ${entry.message}`)
                    .join("\n");
                void navigator.clipboard.writeText(text).then(() => setCopied(true));
            }}
        >
            {copied ? <CheckIcon className="size-4" /> : <CopyIcon className="size-4" />}
            {copied ? "Copied" : "Copy"}
        </Button>
    );
}

/** Wall clock to the second. The date is not worth the width in a list this dense. */
function time(at: number): string {
    return new Date(at).toLocaleTimeString(undefined, { hour12: false });
}
