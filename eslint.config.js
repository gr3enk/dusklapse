import js from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import tseslint from "typescript-eslint";

/**
 * Lint rules.
 *
 * The reason this exists at all is `react-hooks`: this frontend is mostly composed hooks, and the
 * mistakes they invite - a stale dependency array, a conditional call - are silent at runtime and
 * invisible to the type checker.
 *
 * Only the two long-standing hook rules are on. Version 7 of the plugin also ships a set aimed at
 * the React Compiler - `set-state-in-effect`, `refs`, `purity` and others - and those flag ordinary
 * data-fetching effects and the latest-ref pattern that this codebase uses throughout. They are
 * worth a deliberate pass one day; switching them on here would mean either a refactor of every
 * hook or a list of suppressions, and a gate full of suppressions stops being a gate.
 *
 * Type-aware linting is off for a different reason: `tsc --noEmit` already runs in CI and covers
 * the same ground far faster.
 */
export default tseslint.config(
    { ignores: ["dist", "src-tauri", "node_modules"] },
    js.configs.recommended,
    tseslint.configs.recommended,
    {
        // The release script runs under Node, not in the browser, so the browser-shaped default
        // globals do not cover it. Declared rather than pulled from a `globals` package: two names
        // are not worth a dependency.
        files: ["scripts/**/*.{js,mjs}"],
        languageOptions: {
            globals: { process: "readonly", console: "readonly" },
        },
    },
    {
        files: ["**/*.{ts,tsx}"],
        plugins: { "react-hooks": reactHooks },
        rules: {
            // Always a bug: a hook called conditionally or in a loop corrupts React's state
            // bookkeeping in ways that surface as unrelated components misbehaving.
            "react-hooks/rules-of-hooks": "error",
            // A warning rather than an error, because several dependency arrays here are narrowed
            // on purpose - `location?.latitude` instead of `location` so a new object with the same
            // coordinates does not restart a timer. Worth seeing; not worth failing a build over.
            "react-hooks/exhaustive-deps": "warn",
            // The convention in this codebase is a leading underscore for a parameter that exists
            // only to satisfy a signature.
            "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_" }],
        },
    },
);
