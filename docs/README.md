# Dusklapse documentation

The site at [dusklapse.com](https://dusklapse.com), built with
[Docusaurus](https://docusaurus.io/).

Its own pnpm project, not part of the app's workspace - it has its own `pnpm-lock.yaml` and its
own dependency tree.

## Working on it

```bash
pnpm install
pnpm start
```

`pnpm start` serves the site with hot reload. `pnpm build` produces the static site in `build/`
and is what CI and Vercel run.

## Adding a page

Drop an `.mdx` file into `docs/`. The sidebar is generated from the folder structure, so nothing
has to be registered; use `sidebar_position` in the front matter to place it.

## Deployment

Vercel builds this directory, and only when something in it has changed. That is the
`ignoreCommand` in `vercel.json`:

```
git diff --quiet HEAD^ HEAD ./
```

`./` is the project's Root Directory, which is this folder, so the command asks whether the last
commit touched the documentation. The exit codes read backwards from what you would guess: **0
skips the build, 1 runs it** - and `git diff --quiet` exits 0 when it finds no differences. A
commit that only changes the app therefore costs nothing.

If `HEAD^` cannot be resolved - a first deployment, or a clone one commit deep - the command fails
with neither 0 nor 1 and the build goes ahead. The safe direction: an unnecessary build rather than
a missing one.

The `Docs Build` GitHub workflow builds the site on every pull request that touches `docs/`. It
exists because Docusaurus treats a broken link as a build failure, and a broken link in the navbar
or footer appears on every page at once - better to hear about it from CI than from a failed
deployment.
