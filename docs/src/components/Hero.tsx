import { ChevronRightIcon } from "lucide-react";
import React from "react";
import "../css/tailwind.css";
import Link from "@docusaurus/Link";
import { useQuery } from "@tanstack/react-query";

export default function Hero() {
    const query = useQuery({
        queryKey: ["latest-version"],
        queryFn: () => fetch("https://api.github.com/repos/gr3enk/dusklapse/releases/latest").then((res) => res.json()),
    });

    const latestVersion = query.data?.tag_name ?? "";

    return (
        <div className="tw">
            <svg
                aria-hidden="true"
                className="absolute inset-0 -z-10 size-full mask-[radial-gradient(100%_100%_at_top_right,white,transparent)] stroke-gray-200"
            >
                <defs>
                    <pattern
                        x="50%"
                        y={-1}
                        id="0787a7c5-978c-4f66-83c7-11c213f99cb7"
                        width={200}
                        height={200}
                        patternUnits="userSpaceOnUse"
                    >
                        <path d="M.5 200V.5H200" fill="none" />
                    </pattern>
                </defs>
                <rect fill="url(#0787a7c5-978c-4f66-83c7-11c213f99cb7)" width="100%" height="100%" strokeWidth={0} />
            </svg>
            <div className="relative isolate overflow-hidden bg-linear-to-b from-primary/2">
                <div className="mx-auto max-w-7xl pt-10 pb-24 sm:pb-32 lg:grid lg:grid-cols-2 lg:gap-x-8 lg:px-8 lg:py-40">
                    <div className="px-6 lg:px-0 lg:pt-8 xl:pt-16 ">
                        <div className="mx-auto max-w-2xl">
                            <div className="max-w-lg">
                                <img alt="Dusklapse" src="/img/app-icon-rounded-no-padding.webp" className="size-20!" />
                                <div className="mt-24 sm:mt-16 lg:mt-16">
                                    <a
                                        href="https://github.com/gr3enk/dusklapse/releases/latest"
                                        target="_blank"
                                        rel="noopener noreferrer"
                                        className="inline-flex space-x-6"
                                    >
                                        <span className="inline-flex items-center rounded-full bg-primary/10 px-3 py-1 text-sm/6 font-semibold text-primary ring-1 ring-primary/20 ring-inset">
                                            {latestVersion === "" ? "Latest version" : `Version ${latestVersion}`}
                                            <ChevronRightIcon aria-hidden="true" className="size-5 text-primary" />
                                        </span>
                                        <span className="inline-flex items-center space-x-2 text-sm/6 font-medium text-gray-600"></span>
                                    </a>
                                </div>
                                <h1 className="mt-10! text-5xl! font-semibold! tracking-tight! text-pretty! text-gray-900! sm:text-7xl!">
                                    Dusklapse
                                </h1>
                                <p className="mt-8 text-lg font-medium text-pretty text-gray-500 sm:text-xl/8">
                                    Shoot perfectly exposed day-to-night time-lapses with ease
                                </p>
                                <div className="mt-10 flex items-center gap-x-6">
                                    <Link
                                        className="rounded-md bg-primary px-3.5 py-2.5 text-sm font-semibold transition-colors! hover:no-underline! text-white! shadow-xs hover:bg-primary/80 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
                                        to="/docs/intro"
                                    >
                                        Documentation
                                    </Link>
                                    <Link
                                        className="text-sm/6 font-semibold text-gray-900"
                                        href="https://github.com/gr3enk/dusklapse"
                                    >
                                        View on GitHub <span aria-hidden="true">→</span>
                                    </Link>
                                </div>
                            </div>
                        </div>
                    </div>
                    <div className="mt-20 sm:mt-24 md:mx-auto md:max-w-2xl lg:mx-0 lg:mt-0 lg:w-screen">
                        <img
                            src="/img/interface_1.webp"
                            alt="Dusklapse interface"
                            className="w-full shadow-lg md:rounded-3xl lg:w-5xl! lg:max-w-none! xl:w-6xl!"
                        />
                    </div>
                </div>
            </div>
        </div>
    );
}
