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

Vercel builds this directory. Automatic deployments on push are switched off in `vercel.json`, so
a deployment happens on a manual redeploy or through a deploy hook.

The `Docs Build` GitHub workflow builds the site on every push and pull request that touches
`docs/`. It exists because Docusaurus treats a broken link as a build failure, and a broken link
in the navbar or footer appears on every page at once - better to hear about it from CI than from
a failed deployment.
