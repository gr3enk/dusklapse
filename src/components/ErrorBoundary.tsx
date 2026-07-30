import { Component, type ErrorInfo, type ReactNode } from "react";

interface Props {
    children: ReactNode;
}

interface State {
    error: Error | null;
}

/**
 * Turns a component crash into a message instead of a black screen.
 *
 * React unmounts the whole tree when a render throws, and with a full-viewport dark theme the
 * result is an app that looks switched off - no clue what happened, and no way back short of
 * a restart. That is exactly what a mismatched field name between the backend and a readout
 * produced here once.
 *
 * A class component because this is the one thing hooks cannot do: `componentDidCatch` has no
 * hook equivalent.
 */
export class ErrorBoundary extends Component<Props, State> {
    state: State = { error: null };

    static getDerivedStateFromError(error: Error): State {
        return { error };
    }

    componentDidCatch(error: Error, info: ErrorInfo) {
        // The component stack is the part that actually locates the fault, and it is lost
        // unless it is logged here.
        console.error("render failed", error, info.componentStack);
    }

    render() {
        if (!this.state.error) return this.props.children;

        return (
            <div className="m-auto flex max-w-md flex-col gap-4 p-6 text-center">
                <h1 className="m-0 text-[1.3rem] font-[650]">Something in the interface broke</h1>
                <p className="m-0 text-text-muted">
                    The camera connection is unaffected - it lives in the app's backend, not in this screen. Reloading
                    rejoins the session that is already open.
                </p>
                <pre className="m-0 overflow-x-auto rounded-card border border-border bg-surface p-3 text-left text-[0.8rem] text-danger">
                    {this.state.error.message}
                </pre>
                <button
                    className="min-h-tap cursor-pointer rounded-card border border-border bg-surface-raised px-4 font-[550]"
                    type="button"
                    onClick={() => window.location.reload()}
                >
                    Reload
                </button>
            </div>
        );
    }
}
