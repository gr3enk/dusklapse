import { TextStep } from "./HelpComponents";

export default function NikonConnectionHelp() {
    return (
        <div>
            <h1 className="text-2xl font-bold">Canon Connection Help</h1>
            <div className="bg-red-600/50 border border-red-500 p-4 rounded-md my-4 w-full flex flex-col gap-2">
                <p className="mb-2">
                    To use Dusklapse with a Canon camera, the Camera Control API (CCAPI) must be enabled on the camera.
                    On certain models, CCAPI is enabled by default. In other cases, CCAPI must be enabled through a
                    Canon Developer Account. For more information, see:
                </p>
                <a className="underline w-full" href="https://developers.canon-europe.com/s/camera?t=1788350696044">
                    https://developers.canon-europe.com/s/camera?t=1788350696044
                </a>
                <a
                    className="underline w-full"
                    href="https://developercommunity.usa.canon.com/s/article/CCAPI-Supported-Cameras"
                >
                    https://developercommunity.usa.canon.com/s/article/CCAPI-Supported-Cameras
                </a>
                <a className="underline w-full" href="https://www.dusklapse.com/docs/cameras">
                    https://www.dusklapse.com/docs/cameras{" "}
                </a>
            </div>
            <p>To connect to your Canon camera, you need to follow these steps:</p>

            <TextStep step="Step 1: Open the Menu and switch to the 'Network' tab. Select the 'Wi-Fi Settings' option." />
            <TextStep step="Step 2: Click on Camera Control API." />
            <TextStep step="Step 3: Click on Connect." />
            <TextStep step="The camera has now set up a network (access point). You will now see an SSID and a password on the screen. Use these credentials to connect the device running Dusklapse to the camera's network." />
            <TextStep step="Step 4: After connecting your device to the camera's network, you will see the ccapi url on the screen." />
            <TextStep step="In Dusklapse, select Canon under ‘Camera’. Enter the IP-Address and Port from the prompted ccapi url on the camera screen. Now click ‘Connect’. Dusklapse should then switch to the camera interface." />
            <TextStep step="Once the connection has been successfully established, you will see the message ‘Connection Established’ on your camera screen." />
        </div>
    );
}
