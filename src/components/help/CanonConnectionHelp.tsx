import img1 from "../../assets/connection-guide/canon/en1.jpg";
import img2 from "../../assets/connection-guide/canon/en2.jpg";
import img3 from "../../assets/connection-guide/canon/en3.jpg";
import img4 from "../../assets/connection-guide/canon/en4.jpg";
import img5 from "../../assets/connection-guide/canon/en5.jpg";
import img6 from "../../assets/connection-guide/canon/en6.jpg";
import img7 from "../../assets/connection-guide/canon/en7.jpg";

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

            <ImageStep
                step="Step 1: Open the Menu and switch to the 'Network' tab. Select the 'Wi-Fi Settings' option."
                image={img1}
            />
            <ImageStep step="Step 2: Click on Camera Control API." image={img2} />
            <ImageStep step="Step 3: Click on Connect." image={img3} />
            <ImageStep
                step="The camera has now set up a network (access point). You will now see an SSID and a password on the screen. Use these credentials to connect the device running Dusklapse to the camera's network."
                image={img4}
            />
            <ImageStep
                step="Step 4: After connecting your device to the camera's network, you will see the ccapi url on the screen."
                image={img5}
            />
            <ImageStep
                step="In Dusklapse, select Canon under ‘Camera’. Enter the IP-Address and Port from the prompted ccapi url on the camera screen. Now click ‘Connect’. Dusklapse should then switch to the camera interface."
                image={img7}
            />
            <ImageStep
                step="Once the connection has been successfully established, you will see the message ‘Connection Established’ on your camera screen."
                image={img6}
            />
        </div>
    );
}

function ImageStep({ step, image }: { step: string; image: string }) {
    return (
        <div className="py-4">
            <p>{step}</p>
            <img src={image} alt="Canon Connection Help" className="max-h-84 mt-2" />
        </div>
    );
}
