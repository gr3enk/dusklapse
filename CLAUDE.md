# Dusklapse

Dusklapse is an app for creating day-to-night or night-to-day time-lapses (the so-called Holy Grail) using DSLR / DSLM cameras.

The app connects to your camera via Wi-Fi and adjusts your camera’s exposure time, aperture and ISO settings to pre-defined limits, enabling you to capture time-lapse footage with significant changes in light, such as from day to night or vice versa.

## Docs

The user documentation for Dusklapse is located in the docs/ subdirectory. [Docusaurus](https://docusaurus.io/) is used as the framework for the documentation.

Technical documentation is provided inline as comments or in separate Markdown files within the project, such as README.md.

## Camera connection

All camera I/O happens in Rust. The frontend never opens a socket, it calls Tauri commands and
listens for events. Anything that has to survive a WebView reload (ramp settings, app settings) is
owned by Rust rather than held in React state.

### Adding a vendor

Camera support is a strategy pattern. `src-tauri/src/camera/mod.rs` holds the `Camera` trait and the
vendor registry, and each vendor is a module beside it. Adding one means writing that module and
naming it in two places in `mod.rs`. Nothing in the frontend knows which vendor is connected.

The vendors have less in common than the trait suggests. Nikon speaks PTP-IP, Canon's CCAPI is
HTTP/REST, Panasonic is plain HTTP on `cam.cgi`. The trait is the only thing they share.

### Nikon over PTP-IP

Use access point mode, the camera menu item "connect to smart device", where the body hosts its own
network and answers at 192.168.1.1. The "connect to computer" path only listens while the camera
sits on one particular menu screen and drops the connection the moment it leaves it, including when
someone presses the shutter.

A session runs on two TCP sockets on port 15740, one for commands and one for events. The event
socket has to stay open and has to answer the camera's `Probe_Request`, or the body concludes the
client is gone and hangs up. `drain_events` in `ptpip.rs` does both.

PTP allows one transaction at a time. A preview fetch holds the command channel for the whole
transfer, so anything that is not urgent should skip its turn instead of queueing behind it. Ask
`PtpIp::is_busy` first.

Ask what an object is before pulling it. `GetObjectInfo` costs a few dozen bytes and says whether
the object is a JPEG, which is what keeps a 25 MB NEF off the network.

### Rules for every backend

Raw values are opaque. `ExposureValue.raw` is the token the camera itself sent. Echo it back, never
parse it for display, never build one yourself.

The ramp reasons in stops and then snaps onto the values the camera enumerates. A property
constrained by a range rather than an enumeration yields no selectable values, because invented step
positions produce writes the body rejects.

Not every body reports its own frames. A Z 6 fills the event channel, a D5300 stays silent for the
whole session. `watch_card` in `nikon.rs` polls the card instead and switches itself off permanently
as soon as a real event arrives, so a talkative body never counts a frame twice.

A busy camera is not a refusal. PTP `Device_Busy` (0x2019) means the shutter is open and the write
should be tried again, not that the value was wrong. `camera/patience.rs` owns that retry and sizes
its patience from the current shutter speed.

## Linting & Formatting

CI runs these five, and each is a separate check:

```bash
pnpm lint          # ESLint
pnpm typecheck     # tsc --noEmit
pnpm test          # Vitest
cd src-tauri && cargo clippy --all-targets -- -D warnings && cd ..
cd src-tauri && cargo test && cd ..
```

Formatting is Prettier for the web side and rustfmt for Rust, both behind one command:

```bash
pnpm format
```
