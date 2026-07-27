//! Answer one question: does the camera still work while we hold a session open?
//!
//! ```sh
//! cargo run --example nikon_watch -- 192.168.178.75
//! ```
//!
//! Connects, then sits there printing every event the camera volunteers and every
//! settings change it observes. While this runs, go and use the camera: press the
//! shutter, turn a dial, fire your intervalometer.
//!
//! What to look for:
//!
//! * `0x4002 ObjectAdded` - a frame was written. This is proof the body still
//!   shoots with our session open, and it is the signal a ramp would advance on.
//! * `0x4006 DevicePropChanged` - something moved on the body.
//! * settings lines - the same thing seen through polling rather than events.
//!
//! Read-only. Never writes a setting, never releases the shutter.

use std::time::Duration;

use dusklapse_lib::camera::nikon::NikonPtpIp;
use dusklapse_lib::camera::ptpip::{
    PtpEvent, EVENT_CAPTURE_COMPLETE, EVENT_DEVICE_INFO_CHANGED, EVENT_DEVICE_PROP_CHANGED,
    EVENT_OBJECT_ADDED,
};
use dusklapse_lib::camera::{Camera, CameraTarget, ExposureSettings, Vendor};

const WATCH_FOR: Duration = Duration::from_secs(120);
const POLL_EVERY: Duration = Duration::from_secs(2);

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let host = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: nikon_watch <camera-ip>");
        std::process::exit(2);
    });

    let target = CameraTarget::new(Vendor::Nikon, host, Vendor::Nikon.default_port());
    let camera = NikonPtpIp::connect(target).await?;
    // The raw stream, not the filtered one the app uses: a diagnostic wants to see
    // everything the body says, including the noise the app deliberately drops.
    let mut events = camera.raw_events();

    let info = camera.info();
    println!(
        "connected to {} {} - watching for {} seconds\n",
        info.manufacturer,
        info.model,
        WATCH_FOR.as_secs()
    );
    println!("Go use the camera now: press the shutter, turn a dial, run your");
    println!("intervalometer. Anything the body reports shows up below.\n");

    let deadline = tokio::time::Instant::now() + WATCH_FOR;
    let mut poll = tokio::time::interval(POLL_EVERY);
    let mut frames = 0usize;
    let mut files = 0usize;
    let mut previous: Option<ExposureSettings> = None;

    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => break,

            received = events.recv() => match received {
                Ok(event) => {
                    // One exposure, however many files it produced.
                    match event.code {
                        EVENT_CAPTURE_COMPLETE => frames += 1,
                        EVENT_OBJECT_ADDED => files += 1,
                        _ => {}
                    }
                    println!("  {:>8}  event  {}", stamp(), describe(&event));
                }
                // Lagged only means we fell behind; the session is fine.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                    println!("  {:>8}  event  (missed {missed} while busy)", stamp());
                }
                Err(_) => {
                    println!("  {:>8}  the event channel closed", stamp());
                    break;
                }
            },

            _ = poll.tick() => match camera.exposure().await {
                Ok(current) => {
                    if previous.as_ref() != Some(&current) {
                        println!("  {:>8}  set    {}", stamp(), summarise(&current));
                        previous = Some(current);
                    }
                }
                Err(err) => {
                    println!("  {:>8}  lost the camera: {err}", stamp());
                    break;
                }
            },
        }
    }

    println!("\n{frames} frame(s) and {files} file(s) while the session was open.");
    if frames > 0 {
        println!("The body shoots with a session held open - an external");
        println!("intervalometer is all the frame timing this needs.");
        if files > frames {
            // RAW+JPEG, or two storages. Either way ObjectAdded is not a frame.
            println!(
                "Note: {:.0} files per frame, so count CaptureComplete, not ObjectAdded.",
                files as f32 / frames as f32
            );
        }
    } else {
        println!("No CaptureComplete seen. Either nothing was shot, or the body");
        println!("refuses to shoot in this state.");
    }

    camera.disconnect().await?;
    Ok(())
}

fn describe(event: &PtpEvent) -> String {
    let name = match event.code {
        EVENT_OBJECT_ADDED => "ObjectAdded - one file written",
        EVENT_DEVICE_PROP_CHANGED => "DevicePropChanged - a setting moved on the body",
        EVENT_DEVICE_INFO_CHANGED => "DeviceInfoChanged - the body switched profile",
        EVENT_CAPTURE_COMPLETE => "CaptureComplete - one frame exposed",
        _ => "unrecognised",
    };
    format!("0x{:04x} {name} {:?}", event.code, event.params)
}

fn summarise(settings: &ExposureSettings) -> String {
    let part = |value: &Option<dusklapse_lib::camera::ExposureValue>| {
        value
            .as_ref()
            .map(|value| value.label.clone())
            .unwrap_or_else(|| "-".into())
    };
    format!(
        "{} {} ISO {}",
        part(&settings.shutter),
        part(&settings.aperture),
        part(&settings.iso)
    )
}

/// Seconds since the watch started, so events can be lined up with what you did.
fn stamp() -> String {
    use std::sync::OnceLock;
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    let start = START.get_or_init(std::time::Instant::now);
    format!("{:.1}s", start.elapsed().as_secs_f32())
}
