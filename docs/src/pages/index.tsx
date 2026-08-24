import type { ReactNode } from "react";
import Link from "@docusaurus/Link";
import useDocusaurusContext from "@docusaurus/useDocusaurusContext";
import Layout from "@theme/Layout";
import Heading from "@theme/Heading";

import styles from "./index.module.css";

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
            <header className={styles.hero}>
                <div className="container">
                    <Heading as="h1" className={styles.title}>
                        {siteConfig.title}
                    </Heading>
                    <p className={styles.tagline}>{siteConfig.tagline}</p>
                    <p className={styles.summary}>
                        Dusklapse is an app for creating day-to-night or night-to-day time-lapses (the so-called Holy
                        Grail) using DSLR / DSLM cameras.
                        <br />
                        <br />
                        The app connects to your camera via Wi-Fi and adjusts your camera’s exposure time, aperture and
                        ISO settings to pre-defined limits, enabling you to capture time-lapse footage with significant
                        changes in light, such as from day to night or vice versa.
                    </p>
                    <div className={styles.actions}>
                        <Link className="button button--primary button--lg" to="/docs/intro">
                            Read the docs
                        </Link>
                        <Link
                            className="button button--secondary button--lg"
                            href="https://github.com/gr3enk/dusklapse"
                        >
                            GitHub
                        </Link>
                    </div>
                </div>
            </header>
            <div className="">
                <img src="/img/interface_1.webp" alt="Dusklapse Screenshot" />
            </div>
        </Layout>
    );
}
