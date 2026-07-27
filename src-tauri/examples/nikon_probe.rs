//! Exercise the real camera backend against real hardware.
//!
//! ```sh
//! cargo run --example nikon_probe -- 192.168.178.75
//! ```
//!
//! The camera has to be sitting on its connect-to-computer screen - it only
//! listens on port 15740 while it is waiting to be paired.
//!
//! Read-only: prints the dials and their selectable values, holds the session
//! open for a few seconds to show that it survives being idle, then disconnects.
//! Never writes a setting and never releases the shutter.

use std::time::Duration;

use dusklapse_lib::camera::{self, CameraTarget, Dial, Vendor};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let host = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: nikon_probe <camera-ip>");
        std::process::exit(2);
    });

    let target = CameraTarget::new(Vendor::Nikon, host, Vendor::Nikon.default_port());
    println!("connecting to {}:{}", target.host, target.port);

    let camera = camera::connect(target).await?;
    let info = camera.info();
    println!(
        "\nconnected: {} {} (firmware {}, serial {})",
        info.manufacturer,
        info.model,
        info.firmware.as_deref().unwrap_or("?"),
        info.serial.as_deref().unwrap_or("?"),
    );
    println!("remote release supported: {}", info.supports_release);

    let exposure = camera.exposure().await?;
    println!("\ncurrent settings");
    for dial in Dial::ALL {
        match exposure.dial(dial) {
            Some(value) => println!(
                "  {:<9} {:<8} (raw {:>10}, {})",
                dial.label(),
                value.label,
                value.raw,
                value
                    .stops
                    .map(|stops| format!("{stops:+.2} stops"))
                    .unwrap_or_else(|| "no fixed brightness".into()),
            ),
            None => println!("  {:<9} unavailable", dial.label()),
        }
    }
    match exposure.total_stops() {
        Some(stops) => println!("  total brightness {stops:+.2} EV"),
        None => println!("  total brightness unavailable"),
    }

    let capabilities = camera.capabilities().await?;
    println!("\nselectable values");
    for dial in Dial::ALL {
        let values = capabilities.dial(dial);
        let labels: Vec<&str> = values.iter().map(|value| value.label.as_str()).collect();
        println!("  {:<9} {:>3} values", dial.label(), values.len());
        println!("            {}", labels.join("  "));
    }

    match camera.battery().await? {
        Some(battery) => println!("\nbattery {}", battery.label),
        None => println!("\nbattery not reported"),
    }

    // The reason the app polls: a quiet session gets dropped by the body. Hold it
    // and keep reading to show that the connection survives.
    println!("\nholding the session open, reading once a second:");
    for round in 1..=5 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        match camera.exposure().await {
            Ok(exposure) => println!(
                "  {round}: {} {} ISO {}",
                exposure.shutter.map(|v| v.label).unwrap_or_default(),
                exposure.aperture.map(|v| v.label).unwrap_or_default(),
                exposure.iso.map(|v| v.label).unwrap_or_default(),
            ),
            Err(err) => {
                println!("  {round}: lost the camera: {err}");
                break;
            }
        }
    }

    camera.disconnect().await?;
    println!("\ndisconnected cleanly");
    Ok(())
}
