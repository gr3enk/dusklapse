import { themes as prismThemes } from "prism-react-renderer";
import type { Config } from "@docusaurus/types";
import type * as Preset from "@docusaurus/preset-classic";

// This runs in Node.js - Don't use client-side code here (browser APIs, JSX...)

const GITHUB = "https://github.com/gr3enk/dusklapse";

const config: Config = {
    title: "Dusklapse",
    tagline: "Holy-grail timelapse control for networked cameras",
    favicon: "img/app-icon-rounded-no-padding.webp",

    future: {
        v4: true, // Improve compatibility with the upcoming Docusaurus v4
    },

    url: "https://dusklapse.com",
    baseUrl: "/",
    trailingSlash: false,

    // Only used by `docusaurus deploy` for GitHub Pages, which this site does not use - it is
    // deployed from Vercel. Kept accurate rather than removed, so nobody has to work out later
    // whether the wrong values meant something.
    organizationName: "gr3enk",
    projectName: "dusklapse",

    // Kept as `throw`. A broken link is nearly always in the navbar or footer, which puts it on
    // every page at once, and the alternative is a site that quietly points at nothing.
    onBrokenLinks: "throw",

    i18n: {
        defaultLocale: "en",
        locales: ["en"],
    },

    staticDirectories: ["static", "../src/assets"],
    plugins: ["./src/plugins/tailwind-config.ts"],

    presets: [
        [
            "classic",
            {
                docs: {
                    sidebarPath: "./sidebars.ts",
                    editUrl: `${GITHUB}/tree/main/docs/`,
                },
                // No blog. It shipped with the template full of sample posts, and a project this
                // size announces releases on GitHub instead. To bring it back, replace `false`
                // with the options and add a `blog/` folder.
                blog: false,
                theme: {
                    customCss: "./src/css/custom.css",
                },
            } satisfies Preset.Options,
        ],
    ],

    themeConfig: {
        // No `image` yet: a social card wants roughly 1200x630, and shipping the template's
        // Docusaurus-branded one would have been worse than having none. Add one here when it
        // exists and link previews will pick it up.
        colorMode: {
            respectPrefersColorScheme: true,
        },
        navbar: {
            title: "Dusklapse",
            logo: {
                alt: "Dusklapse",
                src: "img/app-icon-rounded.webp",
            },
            items: [
                { to: "/docs/intro", label: "Docs", position: "left" },
                { to: "/support", label: "Support", position: "left" },
                { href: GITHUB, label: "GitHub", position: "right" },
            ],
        },
        footer: {
            style: "dark",
            links: [
                {
                    title: "Docs",
                    items: [
                        { label: "Introduction", to: "/docs/intro" },
                        { label: "Support", to: "/support" },
                    ],
                },
                {
                    title: "Project",
                    items: [
                        { label: "GitHub", href: GITHUB },
                        { label: "Issues", href: `${GITHUB}/issues` },
                        { label: "Releases", href: `${GITHUB}/releases` },
                    ],
                },
            ],
            copyright: `Dual licensed under MIT and Apache-2.0. Built with Docusaurus.`,
        },
        prism: {
            theme: prismThemes.github,
            darkTheme: prismThemes.dracula,
            // The languages this project is actually written in. Docusaurus bundles a small set by
            // default and Rust is not in it, so a Rust snippet would render unhighlighted.
            additionalLanguages: ["rust", "toml", "bash", "json"],
        },
    } satisfies Preset.ThemeConfig,
};

export default config;
