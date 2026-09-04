import { CodeXmlIcon, NetworkIcon, TabletSmartphoneIcon, WrenchIcon } from "lucide-react";
import React from "react";

const features = [
    {
        name: "Intuitive Interface",
        description:
            "Dusklapse features an intuitive interface that allows you to easily create and manage your holy-grail time-lapse sequences. The app is designed to be user-friendly and easy to use, with a focus on simplicity and efficiency.",
        icon: TabletSmartphoneIcon,
    },
    {
        name: "Customizable",
        description:
            "Dusklapse allows you to customize your time-lapse sequences to your needs. You can control the exposure time, aperture and ISO settings to your liking, and the app will adjust them automatically to capture the perfect time-lapse footage.",
        icon: WrenchIcon,
    },
    {
        name: "Compatible with multiple cameras",
        description:
            "Dusklapse is compatible with multiple cameras, including DSLRs, DSLMs and mirrorless cameras from various manufacturers.",
        icon: NetworkIcon,
    },

    {
        name: "Open Source",
        description:
            "Dusklapse is open source and free to use. The app is licensed under the MIT license, which means you can use it for free and modify it to your needs.",
        icon: CodeXmlIcon,
    },
];

export default function Features() {
    return (
        <div className="tw">
            <div className="bg-white py-18 sm:py-18">
                <div className="mx-auto max-w-7xl px-6 lg:px-8">
                    <div className="mx-auto max-w-2xl lg:text-center">
                        <h2 className="text-base/7 font-semibold text-primary!">Seemless Timelapse Control</h2>
                        <p className="mt-2 text-4xl font-semibold tracking-tight text-pretty text-gray-900 sm:text-5xl lg:text-balance">
                            Holy-grail timelapse control for networked cameras
                        </p>
                        <p className="mt-6 text-lg/8 text-gray-700">
                            Dusklapse is an app for creating day-to-night or night-to-day time-lapses (the so-called
                            Holy Grail) using DSLR / DSLM cameras.
                        </p>
                        <p className="mt-6 text-lg/8 text-gray-700">
                            The app connects to your camera via Wi-Fi and adjusts your camera’s exposure time, aperture
                            and ISO settings to pre-defined limits, enabling you to capture time-lapse footage with
                            significant changes in light, such as from day to night or vice versa.
                        </p>
                    </div>
                    <div className="mx-auto mt-16 max-w-2xl sm:mt-20 lg:mt-24 lg:max-w-4xl">
                        <dl className="grid max-w-xl grid-cols-1 gap-x-8 gap-y-10 lg:max-w-none lg:grid-cols-2 lg:gap-y-16">
                            {features.map((feature) => (
                                <div key={feature.name} className="relative pl-16">
                                    <dt className="text-base/7 font-semibold text-gray-900">
                                        <div className="absolute top-0 left-0 flex size-11 items-center justify-center rounded-lg bg-primary">
                                            <feature.icon aria-hidden="true" className="size-6 text-white" />
                                        </div>
                                        {feature.name}
                                    </dt>
                                    <dd className="mt-2 text-base/7 text-gray-600 ml-0!">{feature.description}</dd>
                                </div>
                            ))}
                        </dl>
                    </div>
                </div>
            </div>
        </div>
    );
}
