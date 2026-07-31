/**
 * Raise the marketing version, in every file that carries it.
 *
 * Three places hold it and they must agree: `tauri.conf.json` is what ends up in the bundle as
 * CFBundleShortVersionString, and the two manifests are what a reader of the repository sees. A
 * release where they disagree is a release nobody can identify afterwards.
 *
 * Not the build number. That is a different thing with a different job - it must rise on every
 * upload, including two uploads of the same version - and the release workflow supplies it from
 * the run counter.
 *
 * Usage: node scripts/bump-version.mjs <patch|minor|major>
 */

import { readFileSync, writeFileSync } from "node:fs";

const FILES = ["src-tauri/tauri.conf.json", "package.json"];
const CARGO = "src-tauri/Cargo.toml";
const CARGO_LOCK = "src-tauri/Cargo.lock";

const level = process.argv[2];
if (!["patch", "minor", "major"].includes(level)) {
    console.error(`Usage: node scripts/bump-version.mjs <patch|minor|major>`);
    process.exit(1);
}

/** The version already in the bundle config, which is the one that counts. */
const config = JSON.parse(readFileSync(FILES[0], "utf8"));
const current = config.version;

const parts = current.split(".").map(Number);
if (parts.length !== 3 || parts.some((n) => !Number.isInteger(n) || n < 0)) {
    console.error(`Cannot bump "${current}": expected three non-negative integers.`);
    process.exit(1);
}

const [major, minor, patch] = parts;
// A minor bump resets the patch and a major resets both, or the numbers stop meaning anything.
const next = { major: [major + 1, 0, 0], minor: [major, minor + 1, 0], patch: [major, minor, patch + 1] }[level].join(
    ".",
);

for (const file of FILES) {
    const raw = readFileSync(file, "utf8");
    // Replaced as text rather than re-serialised, so key order, indentation and the trailing
    // newline survive - a version bump should not produce a diff that touches the whole file.
    const updated = raw.replace(new RegExp(`("version"\\s*:\\s*)"${escape(current)}"`), `$1"${next}"`);
    if (updated === raw) {
        console.error(`${file}: no "version": "${current}" to replace.`);
        process.exit(1);
    }
    writeFileSync(file, updated);
}

const cargo = readFileSync(CARGO, "utf8");
// Anchored to the line so a dependency that happens to be on this version is left alone.
const updatedCargo = cargo.replace(new RegExp(`^version = "${escape(current)}"$`, "m"), `version = "${next}"`);
if (updatedCargo === cargo) {
    console.error(`${CARGO}: no version = "${current}" line to replace.`);
    process.exit(1);
}
writeFileSync(CARGO, updatedCargo);

// The lockfile records this crate's own version alongside every dependency, so leaving it behind
// publishes a commit where the two disagree. Cargo then silently rewrites it on the next build -
// including the one rust-analyzer runs when a file is opened - and everyone who pulls finds a
// modified working tree they did not touch and a pull that refuses to run.
//
// Edited as text rather than by running `cargo`: the release workflow raises the version before the
// Rust toolchain is installed, and a lockfile is not worth a toolchain.
const lock = readFileSync(CARGO_LOCK, "utf8");
// Anchored to this crate's own block. A dependency that happens to share the version must not be
// touched, and `[[package]]` blocks are separated by a blank line.
const block = new RegExp(`(\\[\\[package\\]\\]\\nname = "dusklapse"\\nversion = )"${escape(current)}"`);
if (!block.test(lock)) {
    console.error(`${CARGO_LOCK}: no dusklapse entry at version "${current}" to replace.`);
    process.exit(1);
}
writeFileSync(CARGO_LOCK, lock.replace(block, `$1"${next}"`));

console.log(next);

function escape(value) {
    return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
