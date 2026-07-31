import img1 from "../../assets/connection-guide/nikon/en1.jpg";
import img2 from "../../assets/connection-guide/nikon/en2.jpg";
import img3 from "../../assets/connection-guide/nikon/en3.jpg";
import img4 from "../../assets/connection-guide/nikon/en4.jpg";
import img5 from "../../assets/connection-guide/nikon/en5.jpg";
import img6 from "../../assets/connection-guide/nikon/en6.jpg";

export default function NikonConnectionHelp() {
    return (
        <div>
            <h1 className="text-2xl font-bold">Nikon Connection Help</h1>
            <p>To connect to your Nikon camera, you need to follow these steps:</p>

            <ImageStep
                step="Step 1: Open the Menu and switch to the 'System' tab. Select the 'Connect to smart device' option."
                image={img1}
            />
            <ImageStep step="Step 2: Click on Wi-Fi connection." image={img2} />
            <ImageStep step="Step 3: Click on Establish Wi-Fi connection." image={img3} />
            <ImageStep
                step="The camera has now set up a network (access point). You will now see an SSID and a password on the screen. Use these credentials to connect the device running Dusklapse to the camera's network."
                image={img4}
            />
            <ImageStep
                step="In Dusklapse, select ‘Nikon’ under ‘Camera’. The camera’s default IP address (192.168.1.1) and the PTP IP port (15740) are entered by default. Now click ‘Connect’. Dusklapse should then switch to the camera interface."
                image={img6}
            />
            <ImageStep
                step="Once the connection has been successfully established, you will see the message ‘Connected to smart device’ on your camera screen."
                image={img5}
            />
        </div>
    );
}

function ImageStep({ step, image }: { step: string; image: string }) {
    return (
        <div className="py-4">
            <p>{step}</p>
            <img src={image} alt="Nikon Connection Help" className="max-h-84 mt-2" />
        </div>
    );
}
