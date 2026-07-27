//! Watch a camera over a held-open session.
//!
//! ```sh
//! cargo run --example nikon_watch -- 192.168.1.1              # two minutes
//! cargo run --example nikon_watch -- 192.168.1.1 1800          # half an hour
//! cargo run --example nikon_watch -- 192.168.1.1 1800 --preview
//! ```
//!
//! Prints every event the camera volunteers and every settings change it observes.
//! While it runs, go and use the camera: press the shutter, turn a dial, fire your
//! intervalometer.
//!
//! What to look for:
//!
//! * `0x400d CaptureComplete` - one exposure. This is the frame signal.
//! * `0x4002 ObjectAdded` - one *file*. Two per frame when shooting RAW+JPEG.
//! * `0x4006 DevicePropChanged` - something moved on the body.
//!
//! With `--preview` it also fetches the JPEG after each frame, which is the way to
//! check the JPEG-only filtering without deploying to a device: the log names each
//! file it skips and why. Leave it off for a long durability run - every fetch is
//! several megabytes over the camera's access point.
//!
//! Read-only either way. Never writes a setting, never releases the shutter.

use std::time::Duration;

use dusklapse_lib::camera::nikon::NikonPtpIp;
use dusklapse_lib::camera::ptpip::{
    PtpEvent, EVENT_CAPTURE_COMPLETE, EVENT_DEVICE_INFO_CHANGED, EVENT_DEVICE_PROP_CHANGED,
    EVENT_OBJECT_ADDED,
};
use dusklapse_lib::camera::{Camera, CameraTarget, ExposureSettings, Vendor};

const DEFAULT_SECONDS: u64 = 120;
const POLL_EVERY: Duration = Duration::from_secs(2);

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let host = args.first().cloned().unwrap_or_else(|| {
        eprintln!("usage: nikon_watch <camera-ip> [seconds] [--preview]");
        std::process::exit(2);
    });
    let seconds = args
        .iter()
        .skip(1)
        .find_map(|arg| arg.parse::<u64>().ok())
        .unwrap_or(DEFAULT_SECONDS);
    let fetch_previews = args.iter().any(|arg| arg == "--preview");

    let target = CameraTarget::new(Vendor::Nikon, host, Vendor::Nikon.default_port());
    let camera = NikonPtpIp::connect(target).await?;
    // The raw stream, not the filtered one the app uses: a diagnostic wants to see
    // everything the body says, including the noise the app deliberately drops.
    let mut events = camera.raw_events();

    let info = camera.info();
    println!(
        "connected to {} {} - watching for {seconds} seconds{}\n",
        info.manufacturer,
        info.model,
        if fetch_previews {
            ", fetching previews"
        } else {
            ""
        }
    );
    println!("Go use the camera now: press the shutter, turn a dial, run your");
    println!("intervalometer. Anything the body reports shows up below.\n");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    let mut poll = tokio::time::interval(POLL_EVERY);
    let mut frames = 0usize;
    let mut files = 0usize;
    let mut previews = 0usize;
    let mut previous: Option<ExposureSettings> = None;

    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => break,

            received = events.recv() => match received {
                Ok(event) => {
                    match event.code {
                        EVENT_CAPTURE_COMPLETE => frames += 1,
                        EVENT_OBJECT_ADDED => files += 1,
                        _ => {}
                    }
                    println!("  {:>8}  event  {}", stamp(), describe(&event));

                    if fetch_previews && event.code == EVENT_CAPTURE_COMPLETE {
                        // Blocks this loop for the transfer, exactly as it blocks the
                        // command channel in the app. Events queue meanwhile.
                        match camera.preview().await {
                            Ok(Some(preview)) => {
                                previews += 1;
                                println!(
                                    "  {:>8}  image  {} - {} KiB, {}x{}",
                                    stamp(),
                                    preview.filename,
                                    preview.bytes.len() / 1024,
                                    preview.pixels.0,
                                    preview.pixels.1,
                                );
                            }
                            Ok(None) => {
                                println!("  {:>8}  image  nothing new to fetch", stamp());
                            }
                            Err(err) => {
                                println!("  {:>8}  image  failed: {err}", stamp());
                            }
                        }
                    }
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
    if fetch_previews {
        println!("{previews} preview(s) fetched.");
    }
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
