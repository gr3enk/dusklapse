import type { ReactNode } from "react";
import Link from "@docusaurus/Link";
import useDocusaurusContext from "@docusaurus/useDocusaurusContext";
import Layout from "@theme/Layout";
import Heading from "@theme/Heading";

import styles from "./index.module.css";
import Hero from "../components/Hero";
import Providers from "../providers";
import Features from "../components/Features";
import Waitlist from "../components/Waitlist";

/**
 * The landing page.
 *
 * Deliberately short: it says what the app is and sends you to the documentation. The template's
 * three illustrated feature cards were removed with the artwork they depended on, and a page that
 * repeats the introduction in bullet form only gives the reader the same thing twice.
 */
export default function Home(): ReactNode {
    const { siteConfig } = useDocusaurusContext();

    return (
        <Layout title={siteConfig.title} description={siteConfig.tagline}>
            <Providers>
                <Hero />
                <Features />
                {/* <Waitlist /> */}
            </Providers>
        </Layout>
    );
}
