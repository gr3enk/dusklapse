//! Sony over PTP-IP, through the SDIO extension.
//!
//! # What a ZV-E10 actually offers
//!
//! Measured, firmware 2.01. The standard handshake completes and `GetDeviceInfo` answers, but what
//! it says is that almost nothing standard is there:
//!
//! - `GetDevicePropDesc` (0x1014), `GetDevicePropValue` (0x1015) and `SetDevicePropValue` (0x1016)
//!   are **not** in the operation list.
//! - The device property list is **empty**.
//! - Instead there are sixteen vendor operations in the 0x92xx range and fifteen vendor events in
//!   the 0xC2xx range.
//!
//! So a session that only speaks standard PTP connects, reports the model, and then dies the moment
//! anything asks for a property - which is exactly what happened: `Broken pipe` a few seconds in,
//! once the first `capabilities()` call reached the body.
//!
//! Everything worth controlling sits behind Sony's SDIO extension, and getting at it means a second
//! handshake the standard knows nothing about.
//!
//! # The SDIO handshake
//!
//! Three calls to `SDIO_Connect` (0x9201) with a phase number, and one call in between that asks
//! the camera to list its own property codes:
//!
//! 1. `SDIO_Connect(1)`
//! 2. `SDIO_Connect(2)`
//! 3. `SDIO_GetExtDeviceInfo(0x12c)` - repeated until it answers with something, because the body
//!    needs a moment before it will
//! 4. `SDIO_Connect(3)`
//!
//! Only after phase 3 does the camera accept property reads and writes. The order and the retry
//! both come from libgphoto2, which has carried Sony support for years.
//!
//! # Reading and writing
//!
//! `SDIO_GetAllExtDevicePropInfo` (0x9209) returns every property in one transfer: an eight byte
//! header, then a run of descriptors with no count in front of them. Each descriptor carries its
//! own code, data type, current value and either a range or an enumeration. One call therefore
//! answers both "what can this dial be set to" and "what is it set to now", where the Nikon backend
//! needs three round trips for the same thing.
//!
//! Writes go through `SDIO_SetExtDevicePropValue` (0x9205), with the property code as the operation
//! parameter and the value as the data phase.
//!
//! # Where the pictures go, and why it decides the frame signal
//!
//! `StillImageStoreDestination` (0xD222) decides whether the body keeps a copy of each frame in
//! memory for a host to collect. This backend sets it to 0x11, card and memory, because the card
//! alone leaves no picture this app can read at all.
//!
//! That copy has to be collected. A host that leaves it fills the body's memory, and the camera
//! then puts a progress bar on screen that stays until the Wi-Fi session ends and stops reporting
//! frames - measured, and the reason [`Camera::preview`] runs on every capture.
//!
//! That choice changes which event announces a frame, measured on the same body:
//!
//! | Destination | Event per shutter release |
//! |---|---|
//! | 0x11, card and memory | `ObjectAdded` 0xC201, once per *file* - twice for RAW+JPEG |
//! | 0x10, card only | `CapturedEvent` 0xC206, once per *exposure* |
//!
//! Both are honoured, since only one arrives in any given mode, and two within half a second are
//! taken as one frame.
//!
//! # Getting the picture
//!
//! The same arrangement as the Nikon backend: react to the capture event, fetch the frame. Two
//! things about this body change how the frame is found.
//!
//! **There is no file to ask for.** In PC Remote mode a ZV-E10 answers `GetStorageIDs` with an
//! empty list and refuses the `GetObjectHandles` wildcard with `Invalid_StorageID`. It writes the
//! card and shows nothing of it. Only the copy in camera memory is reachable, at the fixed handle
//! `0xFFFFC001`, which is why the storage destination is set to keep one.
//!
//! **It must not be touched too early.** `ObjectInMemory` (0xD215) also passes through small values
//! while the frame is still being written, and reading the object then crashes the camera's
//! firmware. Only from 0x8000 upwards is the picture real, and the low bits count how many wait.
//!
//! # Value encoding
//!
//! | Dial | Property | Encoding |
//! |---|---|---|
//! | Shutter | `0xD20D` | UINT32, numerator in the high half, denominator in the low: `0x0001_0064` is 1/100 s, `0x0006_0001` is 6 s |
//! | Aperture | `0x5007` | UINT16 hundredths, as everywhere else: `280` is f/2.8 |
//! | ISO | `0xD21E` | UINT32, the ISO number in the low 24 bits; `0x00FF_FFFF` means auto, and the top byte is a multi-frame noise reduction flag |
//!
//! # Connecting
//!
//! The camera hosts its own network and that is the only arrangement this app cares about. On the
//! body: **MENU → (Network) → PC Remote Function**, connection method **Wi-Fi Direct**, then
//! **Wi-Fi Direct Info.** for the SSID and password. The camera is the gateway of the network it
//! hosts, so its address is whatever the device reports as the router.
//!
//! Never use the pairing route. A Sony that has been paired to an application reportedly cannot be
//! unpaired short of a factory reset, and Wi-Fi Direct needs no pairing at all.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use super::error::{CameraError, CameraResult};
use super::exposure;
use super::model::{
    BatteryStatus, CameraEvent, CameraInfo, CameraTarget, Dial, ExposureCapabilities,
    ExposureSettings, ExposureValue, Preview, Vendor, VendorProfile,
};
use super::ptpip::{is_jpeg, EventMapper, PtpEvent, PtpIp, Reader};
use super::Camera;

/// Shown on the camera during the handshake.
const CLIENT_NAME: &str = "Dusklapse";

// Sony's own operations. Only the four this backend needs.
const OP_SDIO_CONNECT: u16 = 0x9201;
const OP_SDIO_GET_EXT_DEVICE_INFO: u16 = 0x9202;
const OP_SDIO_SET_EXT_DEVICE_PROP_VALUE: u16 = 0x9205;
const OP_SDIO_GET_ALL_EXT_DEVICE_PROP_INFO: u16 = 0x9209;

/// Protocol version to ask for. A body that does not know it answers with its own highest.
const SDIO_PROTOCOL_3_00: u32 = 0x12c;

/// How many times to ask for the extension info before giving up.
///
/// The body needs a moment after phase 2 and answers with an empty payload until it is ready.
/// libgphoto2 tries twenty times; there is no signal to wait on, only the empty answer.
const EXT_INFO_ATTEMPTS: usize = 20;

// Sony's property codes. Aperture is the one place the standard code is reused.
const PROP_SHUTTER_SPEED: u16 = 0xD20D;
const PROP_F_NUMBER: u16 = 0x5007;
const PROP_ISO: u16 = 0xD21E;

/// The ISO number occupies the low 24 bits; the top byte flags multi-frame noise reduction.
const ISO_MASK: u32 = 0x00FF_FFFF;
/// An ISO field of all ones is the camera saying "auto", not an ISO of 16777215.
const ISO_AUTO: u32 = ISO_MASK;

// PTP data types, needed because a Sony descriptor names its own.
const TYPE_INT8: u16 = 0x0001;
const TYPE_UINT8: u16 = 0x0002;
const TYPE_INT16: u16 = 0x0003;
const TYPE_UINT16: u16 = 0x0004;
const TYPE_INT32: u16 = 0x0005;
const TYPE_UINT32: u16 = 0x0006;
const TYPE_INT64: u16 = 0x0007;
const TYPE_UINT64: u16 = 0x0008;
const TYPE_INT128: u16 = 0x0009;
const TYPE_UINT128: u16 = 0x000A;
/// A text property. Carries no number, but its width has to be measured or the run desynchronises.
const TYPE_STR: u16 = 0xFFFF;

/// Array types are the element type with this bit set, and carry a 32-bit count in front.
const TYPE_ARRAY_FLAG: u16 = 0x4000;

// Form flags in a property descriptor.
const FORM_NONE: u8 = 0x00;
const FORM_RANGE: u8 = 0x01;
const FORM_ENUMERATION: u8 = 0x02;

/// `enabled` byte: the camera is offering this control at the moment.
const ENABLED: u8 = 1;
/// `get_set` byte: the property can be written, not only read.
const GET_SET: u8 = 1;

/// Below this, a word following an enumeration is a second list rather than the next record.
///
/// Some bodies write the enumeration twice, the second one authoritative. Nothing flags it; the
/// two cases are told apart by size, because a property code is always 0x5xxx or 0xDxxx while a
/// count of selectable values is small. Measured consequence of missing this: the count is read as
/// a property code and the value behind it as a data type, so the stream stops after one record -
/// which is exactly what a ZV-E10 did.
const SECONDARY_LIST_LIMIT: u16 = 0x200;

/// Where the body puts a picture it has just taken.
const PROP_STORE_DESTINATION: u16 = 0xD222;

/// Write the card *and* keep a copy in memory for this app to collect.
///
/// The alternatives are 0x01, memory only, and 0x10, card only. Card only looks like the obvious
/// choice for a timelapse and is not, because of what the body does *not* offer: measured on a
/// ZV-E10 in PC Remote mode, `GetStorageIDs` answers with an empty list and `GetObjectHandles`
/// refuses the wildcard with `Invalid_StorageID`. The card is written and stays invisible.
///
/// So the copy in memory is the only picture reachable over the wire, and this app has to collect
/// every one of them. A host that does not fills that memory up: the camera puts a progress bar on
/// screen that stays until the Wi-Fi session ends, and stops reporting frames entirely.
const STORE_CARD_AND_MEMORY: u32 = 0x0011;

/// How many pictures are waiting in camera memory.
///
/// Read before touching [`PENDING_OBJECT`], never skipped. libgphoto2 records that the value also
/// passes through small numbers while the frame is still being written, and that fetching then
/// **crashes the camera's firmware**. Only from [`OBJECT_READY`] upwards is the picture real.
const PROP_OBJECT_IN_MEMORY: u16 = 0xD215;

/// The bit that says a picture in memory is finished rather than in progress.
///
/// The low bits count the pictures waiting, so 0x8001 is one, 0x8002 two.
const OBJECT_READY: u32 = 0x8000;

/// Where the picture waiting in memory lives. A fixed pseudo-handle, not a file on a card.
const PENDING_OBJECT: u32 = 0xFFFF_C001;

/// How many times to look for the picture before giving up on this frame.
///
/// The capture event arrives before the body has finished writing, so the first look usually finds
/// nothing. This is a bounded wait for a state change that has already been announced, not a poll:
/// it starts because a frame happened and ends as soon as that frame is ready.
const READY_ATTEMPTS: usize = 12;

/// How long to leave the camera alone between those looks.
const READY_INTERVAL: std::time::Duration = std::time::Duration::from_millis(400);

/// How many times to re-read a property before accepting that a write has not taken.
const SETTLE_ATTEMPTS: usize = 12;

/// How long to wait between those reads. Short, because a dial someone just turned should follow
/// the finger, and each read is a few kilobytes on a link that has nothing else to do.
const SETTLE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(120);

/// One exposure finished. Measured on a ZV-E10 saving to the card alone.
const EVENT_SONY_CAPTURED: u16 = 0xC206;

/// A picture is waiting in camera memory. Measured on the same body saving to card and memory.
///
/// Which of the two arrives depends on where the camera saves, and in each mode only one of them
/// does. Both mean the same thing here, so both are honoured - see [`mapper`] for why that is safe.
const EVENT_SONY_OBJECT_ADDED: u16 = 0xC201;

/// Two frame events closer together than this are one frame.
///
/// `ObjectAdded` counts *files*, so a body writing RAW and JPEG announces twice - measured, two
/// within the same second. A timelapse never has two real frames this close, so treating them as
/// one costs nothing and keeps the ramp from advancing at double speed. The Nikon backend avoids
/// the problem by preferring `CaptureComplete`; Sony has no equivalent that works in both storage
/// modes.
const FRAME_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(500);

pub fn profile() -> VendorProfile {
    VendorProfile {
        vendor: Vendor::Sony,
        label: Vendor::Sony.label().to_string(),
        summary: "PTP-IP - use 'PC Remote Function' with Wi-Fi Direct".into(),
        default_port: Vendor::Sony.default_port(),
        // Measured on a ZV-E10 hosting its own network.
        access_point_host: Some("192.168.122.1".into()),
        needs_address: true,
        implemented: true,
        // Offered like any other vendor. Measured on a ZV-E10: the session holds, the three dials
        // read and write, frames are announced and the picture comes across for the ramp to
        // measure. Other Sony bodies are unverified, but that is true of every vendor here.
        developer_only: false,
    }
}

/// A property as the camera described it.
#[derive(Debug, Clone)]
pub struct SonyProp {
    pub code: u16,
    pub datatype: u16,
    pub current: u32,
    /// Whether the camera says this value can be set *right now*.
    ///
    /// Not a fixed fact about the property. A body changes its mind with the shooting mode and
    /// with what it is busy doing, and it is the only statement available about whether a write
    /// will be honoured - the write itself reports success either way.
    pub writable: bool,
    /// What the dial may be set to. Empty for a property described by a range, for the same reason
    /// as everywhere else in this app: invented step positions produce writes the body rejects.
    pub values: Vec<u32>,
}

pub struct SonyPtpIp {
    target: CameraTarget,
    session: PtpIp,
    info: CameraInfo,
    /// Data type and last seen value per property, from the most recent inventory.
    ///
    /// The width is needed to write at all, and fetching the whole inventory to learn one width
    /// would put a full transfer in front of every ramp step. The value is kept alongside it so a
    /// write knows what it is replacing - see [`SonyPtpIp::settle`].
    known: Mutex<HashMap<u16, (u16, u32)>>,
}

impl SonyPtpIp {
    pub async fn connect(target: CameraTarget) -> CameraResult<Self> {
        let session = PtpIp::connect(&target.host, target.port, CLIENT_NAME, mapper()).await?;

        // Without this the session opens, reports the model, and dies as soon as anything asks it
        // for a property.
        sdio_handshake(&session).await?;

        let device = session.device_info();
        let info = CameraInfo {
            vendor: Vendor::Sony,
            manufacturer: non_empty(&device.manufacturer).unwrap_or_else(|| "Sony".to_string()),
            model: non_empty(&device.model).unwrap_or_else(|| "Unknown body".to_string()),
            serial: non_empty(&device.serial),
            firmware: non_empty(&device.device_version),
            api_version: None,
            // No capture path is confirmed over Wi-Fi yet.
            supports_release: false,
            pushes_events: true,
        };

        let camera = Self {
            target,
            session,
            info,
            known: Mutex::new(HashMap::new()),
        };

        // One inventory at connect, logged. The encodings below are read off libgphoto2 rather than
        // measured on this body, so the first sight of real values is what confirms or corrects
        // them.
        match camera.snapshot().await {
            Ok(props) => {
                report(&props);
                camera.store_card_and_memory(&props).await;
            }
            Err(err) => log::warn!("could not read the property inventory: {err}"),
        }

        Ok(camera)
    }

    /// Stop the camera holding each frame in memory for a download that never comes.
    ///
    /// A ZV-E10 ships set to write the card *and* keep a copy in memory for a host to collect.
    /// This app collects nothing, so that memory fills and the body stalls: a progress bar that
    /// stays until the Wi-Fi session ends, and no further frames reported.
    ///
    /// Left alone when it is already on the card, so a camera someone has configured is not
    /// written to for nothing. The change persists on the body after the session, which is why it
    /// is announced in the log rather than done quietly.
    async fn store_card_and_memory(&self, props: &HashMap<u16, SonyProp>) {
        let Some(prop) = props.get(&PROP_STORE_DESTINATION) else {
            log::info!("camera does not report where it saves pictures; leaving it alone");
            return;
        };
        if prop.current == STORE_CARD_AND_MEMORY {
            return;
        }

        log::info!(
            "camera saves to 0x{:04x}; switching it to card and memory, because the card itself is \
             invisible over Wi-Fi and the copy in memory is the only picture this app can read. \
             This setting stays changed on the camera.",
            prop.current
        );
        match self
            .write_prop(PROP_STORE_DESTINATION, prop.datatype, STORE_CARD_AND_MEMORY)
            .await
        {
            Ok(()) => log::info!("camera now saves to the card and keeps a copy for this app"),
            // Not fatal, but there will be no preview: nothing else can reach a picture.
            Err(err) => log::warn!("could not change where the camera saves: {err}"),
        }
    }

    /// Wait until the camera stops reporting the value the write replaced.
    ///
    /// Sony acknowledges a write and applies it a moment later, and until it has, the inventory
    /// still carries the old value. Returning straight after the acknowledgement means the caller
    /// reads back exactly what it just replaced - which puts the old setting back on screen until
    /// the next heartbeat happens to catch up, seconds later.
    ///
    /// Waits for the value to *change* rather than to become what was asked for, because those are
    /// not the same thing: a body that clamps to a neighbouring value has also settled, and
    /// insisting on the requested value would spend the whole budget every time that happened.
    async fn settle(&self, property: u16, was: u32) {
        let started = std::time::Instant::now();
        let mut declared_writable = None;

        for attempt in 1..=SETTLE_ATTEMPTS {
            tokio::time::sleep(SETTLE_INTERVAL).await;
            match self.snapshot().await {
                Ok(props) => {
                    let Some(prop) = props.get(&property) else {
                        return;
                    };
                    declared_writable = Some(prop.writable);
                    if prop.current != was {
                        // Info only when it took more than one look, which is the case worth
                        // knowing about. A body that answers at once stays silent rather than
                        // writing a line for every step of a ramp.
                        if attempt > 1 {
                            log::info!(
                                "camera took {:.0}ms to report the new value for 0x{property:04x}",
                                started.elapsed().as_secs_f32() * 1000.0
                            );
                        }
                        return;
                    }
                }
                // Nothing to be gained by asking again if the session is unhappy.
                Err(err) => {
                    log::debug!("could not confirm 0x{property:04x}: {err}");
                    return;
                }
            }
        }

        // The flag is the camera's own statement about whether the write could ever have taken.
        // A body that says "writable" and then ignores the write is doing something the protocol
        // gives no way to see; one that says otherwise has told us plainly and we can say so too.
        log::warn!(
            "camera accepted the write to 0x{property:04x} but still reports the old value after \
             {:.1}s (camera declares it writable: {})",
            started.elapsed().as_secs_f32(),
            match declared_writable {
                Some(writable) => writable.to_string(),
                None => "unknown".to_string(),
            }
        );
    }

    /// Send one value at the width its descriptor declared.
    async fn write_prop(&self, property: u16, datatype: u16, raw: u32) -> CameraResult<()> {
        let payload = match datatype {
            TYPE_INT8 | TYPE_UINT8 => vec![raw as u8],
            TYPE_INT16 | TYPE_UINT16 => (raw as u16).to_le_bytes().to_vec(),
            TYPE_INT32 | TYPE_UINT32 => raw.to_le_bytes().to_vec(),
            other => {
                return Err(CameraError::Protocol(format!(
                    "cannot write property 0x{property:04x} of data type 0x{other:04x}"
                )))
            }
        };

        self.session
            .vendor_operation_out(
                OP_SDIO_SET_EXT_DEVICE_PROP_VALUE,
                &[property as u32],
                &payload,
            )
            .await
    }

    /// Every property the camera currently reports, by code.
    ///
    /// One transfer for the lot. Both `capabilities` and `exposure` are built from this, so asking
    /// for both costs one round trip rather than six.
    async fn snapshot(&self) -> CameraResult<HashMap<u16, SonyProp>> {
        let raw = self
            .session
            .vendor_operation(OP_SDIO_GET_ALL_EXT_DEVICE_PROP_INFO, &[])
            .await?;
        let props = parse_all_props(&raw)?;

        // Refreshed on every read, so a write never has to ask.
        let mut known = lock(&self.known);
        for prop in &props {
            known.insert(prop.code, (prop.datatype, prop.current));
        }
        drop(known);

        Ok(props.into_iter().map(|prop| (prop.code, prop)).collect())
    }
}

/// The second handshake, the one the standard knows nothing about.
async fn sdio_handshake(session: &PtpIp) -> CameraResult<()> {
    session
        .vendor_operation(OP_SDIO_CONNECT, &[1, 0, 0])
        .await?;
    session
        .vendor_operation(OP_SDIO_CONNECT, &[2, 0, 0])
        .await?;

    // The body answers this with nothing at all until it is ready, and there is no event or status
    // to wait on - only the empty answer.
    let mut ready = false;
    for attempt in 1..=EXT_INFO_ATTEMPTS {
        let raw = session
            .vendor_operation(OP_SDIO_GET_EXT_DEVICE_INFO, &[SDIO_PROTOCOL_3_00, 1])
            .await?;
        if !raw.is_empty() {
            log::info!("Sony extension info answered on attempt {attempt}");
            report_ext_info(&raw);
            ready = true;
            break;
        }
    }
    if !ready {
        return Err(CameraError::Protocol(
            "the camera never returned its extension info; the SDIO session cannot be opened"
                .into(),
        ));
    }

    session
        .vendor_operation(OP_SDIO_CONNECT, &[3, 0, 0])
        .await?;
    log::info!("Sony SDIO session open");
    Ok(())
}

/// What the extension info says the body supports.
///
/// Two arrays after a version word: the properties it will talk about, then the controls it will
/// accept. Logged rather than acted on, because the property inventory below is the thing this
/// backend actually reads.
fn report_ext_info(raw: &[u8]) {
    let mut reader = Reader::new(raw);
    let Ok(version) = reader.u16() else { return };
    log::info!("Sony extension protocol version 0x{version:03x}");

    if let Ok(props) = reader.u16_array() {
        log::info!("vendor properties: {}", hex_list(&props));
    }
    if let Ok(controls) = reader.u16_array() {
        log::info!("vendor controls: {}", hex_list(&controls));
    }
}

/// The response to `GetAllExtDevicePropInfo`: eight bytes of header, then descriptors back to back.
///
/// The count in the header is not trusted to end the loop - the descriptors are read until the
/// bytes run out, so a body that miscounts still yields everything that parsed.
fn parse_all_props(raw: &[u8]) -> CameraResult<Vec<SonyProp>> {
    if raw.len() <= 8 {
        return Err(CameraError::Protocol(format!(
            "property inventory is only {} bytes",
            raw.len()
        )));
    }

    let declared = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
    let body = &raw[8..];
    let mut reader = Reader::new(body);
    let mut props = Vec::new();

    // A descriptor whose data type this code cannot measure ends the run: the fields are
    // variable-width and there is no length in front of them, so an unknown type means the next
    // read would start in the middle of something.
    while reader.remaining() > 0 {
        match parse_prop(&mut reader) {
            Ok(prop) => props.push(prop),
            Err(err) => {
                // Loud, and with the position. A stream that stops early is the difference between
                // a camera with three dials and a camera with none, and where it stopped is the
                // whole clue.
                log::warn!(
                    "property inventory stopped after {} of {declared} records, {} of {} bytes: {err}",
                    props.len(),
                    body.len() - reader.remaining(),
                    body.len()
                );
                return Ok(props);
            }
        }
    }

    if props.len() != declared {
        log::warn!(
            "property inventory claims {declared} records but carries {}",
            props.len()
        );
    }

    Ok(props)
}

fn parse_prop(reader: &mut Reader) -> CameraResult<SonyProp> {
    let code = reader.u16()?;
    let datatype = reader.u16()?;
    let get_set = reader.u8()?;
    let enabled = reader.u8()?;

    read_value(reader, datatype)?; // default value, never used
    let current = read_value(reader, datatype)?;

    let form = reader.u8()?;
    let mut values = match form {
        FORM_ENUMERATION => read_enumeration(reader, datatype)?,
        FORM_RANGE => {
            // Minimum, maximum and step, read to keep the cursor aligned and then discarded.
            read_value(reader, datatype)?;
            read_value(reader, datatype)?;
            read_value(reader, datatype)?;
            Vec::new()
        }
        FORM_NONE => Vec::new(),
        other => {
            return Err(CameraError::Protocol(format!(
                "property 0x{code:04x} has form flag 0x{other:02x}"
            )))
        }
    };

    // A second enumeration may follow the first, and where it does it is the one that counts.
    // Only after an enumeration: that is the single case libgphoto2 consumes, and taking it
    // anywhere else would eat the start of the next record.
    if form == FORM_ENUMERATION
        && reader
            .peek_u16()
            .is_ok_and(|next| next < SECONDARY_LIST_LIMIT)
    {
        let replacement = read_enumeration(reader, datatype)?;
        if !replacement.is_empty() {
            values = replacement;
        }
    }

    Ok(SonyProp {
        code,
        datatype,
        current,
        writable: writable(get_set, enabled),
        values,
    })
}

/// Whether a descriptor's two flag bytes say the value can be set.
///
/// Read the way libgphoto2 reads them for a body speaking extension protocol 3.00, which is what a
/// ZV-E10 answers with. `enabled` is the camera's live judgement - 1 means the control is available,
/// 0 means greyed out and 2 means shown but not offered - and `get_set` is the property's own
/// nature, 1 where it can be written at all.
fn writable(get_set: u8, enabled: u8) -> bool {
    enabled == ENABLED && get_set == GET_SET
}

/// A count followed by that many values.
fn read_enumeration(reader: &mut Reader, datatype: u16) -> CameraResult<Vec<u32>> {
    let count = reader.u16()? as usize;
    (0..count).map(|_| read_value(reader, datatype)).collect()
}

/// Read one value of the given PTP data type, widened to `u32`.
///
/// Only the fixed-width numeric types. Anything else - a string, an array - has a width this code
/// cannot compute, and guessing would desynchronise every descriptor after it.
fn read_value(reader: &mut Reader, datatype: u16) -> CameraResult<u32> {
    match datatype {
        TYPE_INT8 | TYPE_UINT8 => Ok(reader.u8()? as u32),
        TYPE_INT16 | TYPE_UINT16 => Ok(reader.u16()? as u32),
        TYPE_INT32 | TYPE_UINT32 => reader.u32(),

        // Wider than any dial needs, so stepped over rather than kept. Measured on a ZV-E10:
        // record 56 is a UINT64, and refusing it cost the remaining forty.
        TYPE_INT64 | TYPE_UINT64 => {
            reader.take(8)?;
            Ok(0)
        }
        TYPE_INT128 | TYPE_UINT128 => {
            reader.take(16)?;
            Ok(0)
        }

        // Also stepped over. A PTP string is a character count followed by that many UTF-16 code
        // units, and a count of zero carries no characters at all - measured at record 29.
        TYPE_STR => {
            let characters = reader.u8()? as usize;
            reader.take(characters * 2)?;
            Ok(0)
        }

        // An array: a 32-bit count, then that many elements of the type without the array bit.
        // No dial is an array either, but the width still has to come out exact.
        array if array & TYPE_ARRAY_FLAG != 0 => {
            let element = array & !TYPE_ARRAY_FLAG;
            let width = fixed_width(element).ok_or_else(|| {
                CameraError::Protocol(format!("array of unsupported type 0x{element:04x}"))
            })?;
            let count = reader.u32()? as usize;
            reader.take(count.checked_mul(width).ok_or_else(|| {
                CameraError::Protocol(format!("array of 0x{element:04x} claims {count} elements"))
            })?)?;
            Ok(0)
        }

        other => Err(CameraError::Protocol(format!(
            "unsupported data type 0x{other:04x}"
        ))),
    }
}

/// Bytes one value of a fixed-width type occupies, or `None` for the variable-width ones.
fn fixed_width(datatype: u16) -> Option<usize> {
    match datatype {
        TYPE_INT8 | TYPE_UINT8 => Some(1),
        TYPE_INT16 | TYPE_UINT16 => Some(2),
        TYPE_INT32 | TYPE_UINT32 => Some(4),
        TYPE_INT64 | TYPE_UINT64 => Some(8),
        TYPE_INT128 | TYPE_UINT128 => Some(16),
        _ => None,
    }
}

/// Write down what the camera reported, in the form that confirms or corrects the encodings above.
fn report(props: &HashMap<u16, SonyProp>) {
    let mut codes: Vec<u16> = props.keys().copied().collect();
    codes.sort_unstable();
    log::info!(
        "camera reports {} properties: {}",
        codes.len(),
        hex_list(&codes)
    );

    for (label, code) in [
        ("shutter", PROP_SHUTTER_SPEED),
        ("aperture", PROP_F_NUMBER),
        ("ISO", PROP_ISO),
    ] {
        match props.get(&code) {
            Some(prop) => log::info!(
                "{label} 0x{code:04x}: type 0x{:04x}, now {} (raw 0x{:08x}), {} selectable, \
                 camera says writable: {}",
                prop.datatype,
                describe(code, prop.current),
                prop.current,
                prop.values.len(),
                prop.writable
            ),
            None => log::warn!("{label} 0x{code:04x} is not among the reported properties"),
        }
    }
}

/// A raw value written the way a photographer would recognise it, for the log.
fn describe(code: u16, raw: u32) -> String {
    match code {
        PROP_SHUTTER_SPEED => shutter_seconds(raw)
            .map(exposure::shutter_label)
            .unwrap_or_else(|| "?".into()),
        PROP_F_NUMBER => format!("f/{:.1}", raw as f32 / 100.0),
        PROP_ISO => iso_label(raw),
        _ => raw.to_string(),
    }
}

fn hex_list(codes: &[u16]) -> String {
    if codes.is_empty() {
        return "none".to_string();
    }
    codes
        .iter()
        .map(|code| format!("0x{code:04x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Sony packs an exposure time as a fraction: numerator high, denominator low.
///
/// `None` for a zero denominator, which is not a time.
fn shutter_seconds(raw: u32) -> Option<f32> {
    let numerator = raw >> 16;
    let denominator = raw & 0xFFFF;
    (denominator != 0).then(|| numerator as f32 / denominator as f32)
}

fn iso_label(raw: u32) -> String {
    let base = raw & ISO_MASK;
    if base == ISO_AUTO {
        "Auto".to_string()
    } else {
        base.to_string()
    }
}

/// Sony's events, as far as this backend acts on them.
///
/// Only the two that announce a frame. `DevicePropChanged` (0xC203) fires constantly as dials move
/// but carries no property code - its parameter is always zero - so it cannot say what changed and
/// there is nothing useful to do with it.
fn mapper() -> EventMapper {
    let last_frame: Mutex<Option<std::time::Instant>> = Mutex::new(None);

    std::sync::Arc::new(move |event: &PtpEvent| match event.code {
        EVENT_SONY_CAPTURED | EVENT_SONY_OBJECT_ADDED => {
            let now = std::time::Instant::now();
            let mut last = lock(&last_frame);
            if last.is_some_and(|at| now.duration_since(at) < FRAME_DEBOUNCE) {
                return None;
            }
            *last = Some(now);
            Some(CameraEvent::FrameRecorded)
        }
        _ => None,
    })
}

fn dial_property(dial: Dial) -> u16 {
    match dial {
        Dial::Shutter => PROP_SHUTTER_SPEED,
        Dial::Aperture => PROP_F_NUMBER,
        Dial::Iso => PROP_ISO,
    }
}

#[async_trait]
impl Camera for SonyPtpIp {
    fn target(&self) -> &CameraTarget {
        &self.target
    }

    fn info(&self) -> &CameraInfo {
        &self.info
    }

    async fn capabilities(&self) -> CameraResult<ExposureCapabilities> {
        let props = self.snapshot().await?;
        Ok(ExposureCapabilities {
            shutter: selectable(Dial::Shutter, &props),
            aperture: selectable(Dial::Aperture, &props),
            iso: selectable(Dial::Iso, &props),
        })
    }

    async fn exposure(&self) -> CameraResult<ExposureSettings> {
        let props = self.snapshot().await?;
        Ok(ExposureSettings {
            shutter: current(Dial::Shutter, &props),
            aperture: current(Dial::Aperture, &props),
            iso: current(Dial::Iso, &props),
        })
    }

    async fn set_exposure(&self, dial: Dial, value: &str) -> CameraResult<()> {
        let property = dial_property(dial);
        let raw: u32 = value.parse().map_err(|_| CameraError::ValueNotSelectable {
            dial: dial.label(),
            value: value.to_string(),
        })?;

        // From the last inventory rather than a fresh one. Every read fills this, and connect does
        // one before anything else runs, so it is populated by the time a write can happen.
        let (datatype, was) = lock(&self.known)
            .get(&property)
            .copied()
            .ok_or_else(|| CameraError::Unavailable(format!("property 0x{property:04x}")))?;

        self.write_prop(property, datatype, raw).await?;
        self.settle(property, was).await;
        Ok(())
    }

    async fn shoot(&self, _autofocus: bool) -> CameraResult<()> {
        Err(CameraError::Unavailable(
            "releasing the shutter on a Sony over Wi-Fi".into(),
        ))
    }

    async fn bulb_open(&self) -> CameraResult<()> {
        Err(CameraError::Unavailable("bulb mode on a Sony".into()))
    }

    async fn bulb_close(&self) -> CameraResult<()> {
        Err(CameraError::Unavailable("bulb mode on a Sony".into()))
    }

    async fn battery(&self) -> CameraResult<Option<BatteryStatus>> {
        // Sony reports charge as 0xD218, but nothing has confirmed its scale on this body. Showing
        // an unverified percentage beside a real one would be worse than showing none.
        Ok(None)
    }

    /// Fetch the picture the last capture event announced.
    ///
    /// The same arrangement as the Nikon backend - react to the capture, pull the frame - with two
    /// differences forced by the body.
    ///
    /// First, there is no file to ask for. A ZV-E10 in PC Remote mode lists no storage at all, so
    /// the picture on the card cannot be reached; what can is the copy the camera keeps in memory,
    /// at the fixed handle [`PENDING_OBJECT`].
    ///
    /// Second, that copy must not be touched too early. `ObjectInMemory` passes through small
    /// values while the frame is still being written, and reading the object then crashes the
    /// camera's firmware. So the count is read first and nothing happens until it carries
    /// [`OBJECT_READY`].
    async fn preview(&self) -> CameraResult<Option<Preview>> {
        let mut waiting = 0;
        for attempt in 1..=READY_ATTEMPTS {
            // The whole inventory, because this body offers no way to read one property: its
            // operation list has 0x9209 and not Sony's single-property 0x9203.
            let props = self.snapshot().await?;
            let Some(prop) = props.get(&PROP_OBJECT_IN_MEMORY) else {
                log::info!("camera does not report pictures waiting in memory");
                return Ok(None);
            };

            if prop.current >= OBJECT_READY {
                waiting = prop.current - OBJECT_READY;
                break;
            }

            if attempt == READY_ATTEMPTS {
                log::info!(
                    "no picture ready in camera memory after {:.1}s (count stuck at 0x{:04x})",
                    READY_ATTEMPTS as f32 * READY_INTERVAL.as_secs_f32(),
                    prop.current
                );
                return Ok(None);
            }
            tokio::time::sleep(READY_INTERVAL).await;
        }

        if waiting > 1 {
            // Not fetched here. One picture per capture event is the contract; the rest are
            // collected by the events that announced them.
            log::info!("{waiting} pictures waiting in camera memory");
        }

        let info = self.session.object_info(PENDING_OBJECT).await?;
        if !is_jpeg(info.format) {
            log::info!(
                "skipping {} - format 0x{:04x} is not a JPEG",
                info.filename,
                info.format
            );
            return Ok(None);
        }

        log::info!(
            "fetching {} ({} KiB, {}x{})",
            info.filename,
            info.compressed_size / 1024,
            info.pixel_width,
            info.pixel_height
        );
        let started = std::time::Instant::now();
        let bytes = self.session.object(PENDING_OBJECT).await?;
        log::info!(
            "fetched {} - {} KiB in {:.1}s",
            info.filename,
            bytes.len() / 1024,
            started.elapsed().as_secs_f32()
        );

        // Decoded here rather than in the WebView so the curves on screen are the same data the
        // ramp reads. A failure is logged and dropped: the image is still worth showing.
        let analysis = match super::histogram::analyse(&bytes) {
            Ok(analysis) => {
                log::info!(
                    "{} measures {} on the brightness scale",
                    info.filename,
                    analysis.luminance.value
                );
                Some(analysis)
            }
            Err(err) => {
                log::warn!("could not measure {}: {err}", info.filename);
                None
            }
        };

        Ok(Some(Preview {
            bytes,
            mime: "image/jpeg".into(),
            filename: info.filename,
            pixels: (info.pixel_width, info.pixel_height),
            analysis,
        }))
    }

    fn events(&self) -> Option<tokio::sync::broadcast::Receiver<CameraEvent>> {
        Some(self.session.subscribe_camera())
    }

    async fn disconnect(&self) -> CameraResult<()> {
        self.session.close().await
    }
}

fn selectable(dial: Dial, props: &HashMap<u16, SonyProp>) -> Vec<ExposureValue> {
    let Some(prop) = props.get(&dial_property(dial)) else {
        return Vec::new();
    };
    prop.values
        .iter()
        .map(|raw| exposure_value(dial, *raw))
        .collect()
}

fn current(dial: Dial, props: &HashMap<u16, SonyProp>) -> Option<ExposureValue> {
    props
        .get(&dial_property(dial))
        .map(|prop| exposure_value(dial, prop.current))
}

/// Turn a raw value into something the app can display and reason about.
///
/// The raw token is the camera's own number in decimal, echoed back unchanged on a write - the same
/// rule as every other backend. Only the label and the stop value are derived.
fn exposure_value(dial: Dial, raw: u32) -> ExposureValue {
    let token = raw.to_string();
    match dial {
        Dial::Shutter => match shutter_seconds(raw) {
            Some(seconds) => ExposureValue {
                raw: token,
                label: exposure::shutter_label(seconds),
                stops: Some(exposure::shutter_stops(seconds)),
            },
            None => ExposureValue {
                raw: token,
                label: "-".into(),
                stops: None,
            },
        },
        Dial::Aperture => {
            let f_number = raw as f32 / 100.0;
            ExposureValue {
                raw: token,
                label: format!("f/{f_number:.1}"),
                stops: Some(exposure::aperture_stops(f_number)),
            }
        }
        Dial::Iso => {
            let base = raw & ISO_MASK;
            ExposureValue {
                raw: token,
                label: iso_label(raw),
                // Auto carries no sensitivity the ramp could reason about, and neither does zero.
                stops: (base != ISO_AUTO && base != 0).then(|| exposure::iso_stops(base as f32)),
            }
        }
    }
}

/// A poisoned lock here means a thread panicked while noting a data type. The map is still a valid
/// set of widths, and giving up on writes for the rest of the session would be the worse outcome.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Offered on the connect screen, unlike the mock camera.
    #[test]
    fn the_profile_is_shown_like_any_other_vendor() {
        assert!(!profile().developer_only);
        assert!(profile().implemented);
    }

    #[test]
    fn a_shutter_speed_is_a_fraction() {
        // 1/100 s, the encoding libgphoto2 documents.
        assert_eq!(shutter_seconds(0x0001_0064), Some(0.01));
        // 6 s, the exposure the ramp reaches at dusk.
        assert_eq!(shutter_seconds(0x0006_0001), Some(6.0));
        // 1/8000 s at the fast end.
        assert_eq!(shutter_seconds(0x0001_1F40), Some(1.0 / 8000.0));
    }

    #[test]
    fn a_zero_denominator_is_not_a_time() {
        assert_eq!(shutter_seconds(0x0001_0000), None);
    }

    #[test]
    fn iso_ignores_the_noise_reduction_flag() {
        assert_eq!(iso_label(3200), "3200");
        // Same ISO with multi-frame noise reduction set in the top byte.
        assert_eq!(iso_label(0x0100_0C80), "3200");
        assert_eq!(iso_label(ISO_AUTO), "Auto");
    }

    /// Auto has no sensitivity, so the ramp must not be handed a number for it.
    #[test]
    fn auto_iso_carries_no_stops() {
        assert!(exposure_value(Dial::Iso, ISO_AUTO).stops.is_none());
        assert!(exposure_value(Dial::Iso, 3200).stops.is_some());
    }

    #[test]
    fn the_raw_token_is_echoed_unchanged() {
        // The whole raw value, flags and all - not the decoded ISO. A write sends this back.
        assert_eq!(exposure_value(Dial::Iso, 0x0100_0C80).raw, "16780416");
        assert_eq!(exposure_value(Dial::Shutter, 0x0006_0001).raw, "393217");
    }

    /// A descriptor with an enumeration, built by hand to the layout in the module docs.
    #[test]
    fn a_descriptor_with_an_enumeration_is_read_whole() {
        let mut data = vec![0u8; 8]; // header: count, then a zero word
        data.extend_from_slice(&0x5007u16.to_le_bytes()); // code
        data.extend_from_slice(&TYPE_UINT16.to_le_bytes()); // data type
        data.push(0x01); // get/set
        data.push(0x01); // enabled
        data.extend_from_slice(&280u16.to_le_bytes()); // default
        data.extend_from_slice(&400u16.to_le_bytes()); // current
        data.push(FORM_ENUMERATION);
        data.extend_from_slice(&3u16.to_le_bytes());
        for value in [280u16, 400, 560] {
            data.extend_from_slice(&value.to_le_bytes());
        }

        let props = parse_all_props(&data).unwrap();
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].code, 0x5007);
        assert_eq!(props[0].current, 400);
        assert_eq!(props[0].values, vec![280, 400, 560]);
    }

    /// The bug the ZV-E10 exposed: an enumeration followed by a second one, and a record after it.
    ///
    /// Without consuming the second list, its count reads as the next property code and the value
    /// behind it as a data type, so the stream stops after one record.
    #[test]
    fn a_second_enumeration_replaces_the_first_and_the_run_continues() {
        let mut data = vec![0u8; 8];
        // First record: an enumeration of two, then a replacement list of three.
        data.extend_from_slice(&0x5005u16.to_le_bytes());
        data.extend_from_slice(&TYPE_UINT16.to_le_bytes());
        data.push(0x01);
        data.push(0x01);
        data.extend_from_slice(&2u16.to_le_bytes()); // default
        data.extend_from_slice(&4u16.to_le_bytes()); // current
        data.push(FORM_ENUMERATION);
        data.extend_from_slice(&2u16.to_le_bytes());
        for value in [2u16, 4] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        // The secondary list: a small count where a property code would be 0x5xxx or 0xDxxx.
        data.extend_from_slice(&3u16.to_le_bytes());
        for value in [2u16, 4, 8] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        // Second record, which only parses if the list above was consumed.
        data.extend_from_slice(&0x5007u16.to_le_bytes());
        data.extend_from_slice(&TYPE_UINT16.to_le_bytes());
        data.push(0x01);
        data.push(0x01);
        data.extend_from_slice(&280u16.to_le_bytes());
        data.extend_from_slice(&400u16.to_le_bytes());
        data.push(FORM_NONE);

        let props = parse_all_props(&data).unwrap();
        assert_eq!(
            props.len(),
            2,
            "the record after the secondary list must still parse"
        );
        assert_eq!(
            props[0].values,
            vec![2, 4, 8],
            "the second list is the one that counts"
        );
        assert_eq!(props[1].code, 0x5007);
        assert_eq!(props[1].current, 400);
    }

    /// A property code following an enumeration must not be mistaken for a secondary count.
    #[test]
    fn a_following_property_code_is_left_alone() {
        let mut data = vec![0u8; 8];
        data.extend_from_slice(&0x5005u16.to_le_bytes());
        data.extend_from_slice(&TYPE_UINT16.to_le_bytes());
        data.push(0x01);
        data.push(0x01);
        data.extend_from_slice(&2u16.to_le_bytes());
        data.extend_from_slice(&2u16.to_le_bytes());
        data.push(FORM_ENUMERATION);
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&2u16.to_le_bytes());
        // 0xD20D is a property code, well above the limit, so it starts a record.
        data.extend_from_slice(&0xD20Du16.to_le_bytes());
        data.extend_from_slice(&TYPE_UINT32.to_le_bytes());
        data.push(0x01);
        data.push(0x01);
        data.extend_from_slice(&0x0006_0001u32.to_le_bytes());
        data.extend_from_slice(&0x0006_0001u32.to_le_bytes());
        data.push(FORM_NONE);

        let props = parse_all_props(&data).unwrap();
        assert_eq!(props.len(), 2);
        assert_eq!(props[0].values, vec![2]);
        assert_eq!(props[1].code, 0xD20D);
        assert_eq!(shutter_seconds(props[1].current), Some(6.0));
    }

    /// Record 29 on a ZV-E10 is a text property, and stopping there cost the other 67.
    #[test]
    fn a_text_property_is_stepped_over_rather_than_stopping_the_run() {
        let mut data = vec![0u8; 8];
        // A string property: default and current are both PTP strings.
        data.extend_from_slice(&0xD223u16.to_le_bytes());
        data.extend_from_slice(&TYPE_STR.to_le_bytes());
        data.push(0x01);
        data.push(0x01);
        for text in ["ab", "cd"] {
            // A PTP string counts characters including the terminator.
            data.push(text.len() as u8 + 1);
            for character in text.chars() {
                data.extend_from_slice(&(character as u16).to_le_bytes());
            }
            data.extend_from_slice(&0u16.to_le_bytes());
        }
        data.push(FORM_NONE);
        // The record that used to be unreachable.
        data.extend_from_slice(&PROP_ISO.to_le_bytes());
        data.extend_from_slice(&TYPE_UINT32.to_le_bytes());
        data.push(0x01);
        data.push(0x01);
        data.extend_from_slice(&3200u32.to_le_bytes());
        data.extend_from_slice(&3200u32.to_le_bytes());
        data.push(FORM_NONE);

        let props = parse_all_props(&data).unwrap();
        assert_eq!(
            props.len(),
            2,
            "the record after the text property must parse"
        );
        assert_eq!(props[1].code, PROP_ISO);
        assert_eq!(props[1].current, 3200);
    }

    /// An empty string carries no characters at all, only its count.
    #[test]
    fn an_empty_text_property_is_stepped_over_too() {
        let mut data = vec![0u8; 8];
        data.extend_from_slice(&0xD223u16.to_le_bytes());
        data.extend_from_slice(&TYPE_STR.to_le_bytes());
        data.push(0x01);
        data.push(0x01);
        data.push(0); // default: no characters
        data.push(0); // current: no characters
        data.push(FORM_NONE);
        data.extend_from_slice(&PROP_F_NUMBER.to_le_bytes());
        data.extend_from_slice(&TYPE_UINT16.to_le_bytes());
        data.push(0x01);
        data.push(0x01);
        data.extend_from_slice(&280u16.to_le_bytes());
        data.extend_from_slice(&280u16.to_le_bytes());
        data.push(FORM_NONE);

        let props = parse_all_props(&data).unwrap();
        assert_eq!(props.len(), 2);
        assert_eq!(props[1].code, PROP_F_NUMBER);
    }

    /// Record 56 on a ZV-E10 is a UINT64, and refusing it cost the remaining forty.
    #[test]
    fn a_wide_property_is_stepped_over_rather_than_stopping_the_run() {
        let mut data = vec![0u8; 8];
        data.extend_from_slice(&0xD24Au16.to_le_bytes());
        data.extend_from_slice(&TYPE_UINT64.to_le_bytes());
        data.push(0x01);
        data.push(0x01);
        data.extend_from_slice(&1u64.to_le_bytes()); // default
        data.extend_from_slice(&2u64.to_le_bytes()); // current
        data.push(FORM_NONE);
        // The record that used to be unreachable.
        data.extend_from_slice(&PROP_SHUTTER_SPEED.to_le_bytes());
        data.extend_from_slice(&TYPE_UINT32.to_le_bytes());
        data.push(0x01);
        data.push(0x01);
        data.extend_from_slice(&0x0006_0001u32.to_le_bytes());
        data.extend_from_slice(&0x0006_0001u32.to_le_bytes());
        data.push(FORM_NONE);

        let props = parse_all_props(&data).unwrap();
        assert_eq!(props.len(), 2);
        assert_eq!(props[1].code, PROP_SHUTTER_SPEED);
        assert_eq!(shutter_seconds(props[1].current), Some(6.0));
    }

    /// An array carries a 32-bit count in front of its elements.
    #[test]
    fn an_array_property_is_stepped_over_by_its_own_length() {
        let mut data = vec![0u8; 8];
        data.extend_from_slice(&0xD283u16.to_le_bytes());
        data.extend_from_slice(&(TYPE_ARRAY_FLAG | TYPE_UINT16).to_le_bytes());
        data.push(0x01);
        data.push(0x01);
        for count in [2u32, 3] {
            data.extend_from_slice(&count.to_le_bytes());
            for value in 0..count as u16 {
                data.extend_from_slice(&value.to_le_bytes());
            }
        }
        data.push(FORM_NONE);
        data.extend_from_slice(&PROP_F_NUMBER.to_le_bytes());
        data.extend_from_slice(&TYPE_UINT16.to_le_bytes());
        data.push(0x01);
        data.push(0x01);
        data.extend_from_slice(&280u16.to_le_bytes());
        data.extend_from_slice(&280u16.to_le_bytes());
        data.push(FORM_NONE);

        let props = parse_all_props(&data).unwrap();
        assert_eq!(props.len(), 2);
        assert_eq!(props[1].code, PROP_F_NUMBER);
        assert_eq!(props[1].current, 280);
    }

    /// Either capture event counts as a frame, because which one arrives depends on where the
    /// camera saves and only one of them ever does.
    #[test]
    fn both_capture_events_count_as_a_frame() {
        let event = |code| PtpEvent {
            code,
            params: Vec::new(),
        };

        assert_eq!(
            mapper()(&event(EVENT_SONY_CAPTURED)),
            Some(CameraEvent::FrameRecorded)
        );
        assert_eq!(
            mapper()(&event(EVENT_SONY_OBJECT_ADDED)),
            Some(CameraEvent::FrameRecorded)
        );
    }

    /// `DevicePropChanged` fires constantly as dials move and never says which one.
    #[test]
    fn a_property_change_is_not_a_frame() {
        let event = PtpEvent {
            code: 0xC203,
            params: vec![0],
        };
        assert_eq!(mapper()(&event), None);
    }

    /// RAW+JPEG announces twice within a second. The twin must not advance the ramp.
    #[test]
    fn a_second_announcement_of_the_same_frame_is_ignored() {
        let map = mapper();
        let event = PtpEvent {
            code: EVENT_SONY_OBJECT_ADDED,
            params: Vec::new(),
        };

        assert_eq!(map(&event), Some(CameraEvent::FrameRecorded));
        assert_eq!(map(&event), None, "the file's twin is the same frame");
    }

    /// The two flag bytes are the only statement a camera makes about whether a write will take.
    #[test]
    fn a_property_is_writable_only_when_offered_and_settable() {
        assert!(writable(GET_SET, ENABLED));
        // Read-only property, however available it is.
        assert!(!writable(0, ENABLED));
        // Greyed out, and shown but not offered.
        assert!(!writable(GET_SET, 0));
        assert!(!writable(GET_SET, 2));
    }

    /// A range yields no selectable values, the same rule the Nikon backend follows.
    #[test]
    fn a_range_yields_nothing_to_choose_from() {
        let mut data = vec![0u8; 8];
        data.extend_from_slice(&0xD20Du16.to_le_bytes());
        data.extend_from_slice(&TYPE_UINT32.to_le_bytes());
        data.push(0x01);
        data.push(0x01);
        data.extend_from_slice(&0x0001_0064u32.to_le_bytes()); // default
        data.extend_from_slice(&0x0001_0064u32.to_le_bytes()); // current
        data.push(FORM_RANGE);
        for value in [1u32, 100, 1] {
            data.extend_from_slice(&value.to_le_bytes());
        }

        let props = parse_all_props(&data).unwrap();
        assert_eq!(props.len(), 1);
        assert!(props[0].values.is_empty());
        assert_eq!(props[0].current, 0x0001_0064);
    }

    /// Several descriptors run together with no separator, which is how the camera sends them.
    #[test]
    fn descriptors_are_read_one_after_another() {
        let mut data = vec![0u8; 8];
        for (code, current) in [(0x5007u16, 280u16), (0xD500, 7)] {
            data.extend_from_slice(&code.to_le_bytes());
            data.extend_from_slice(&TYPE_UINT16.to_le_bytes());
            data.push(0x01);
            data.push(0x01);
            data.extend_from_slice(&current.to_le_bytes());
            data.extend_from_slice(&current.to_le_bytes());
            data.push(FORM_NONE);
        }

        let props = parse_all_props(&data).unwrap();
        assert_eq!(props.len(), 2);
        assert_eq!(props[1].code, 0xD500);
    }

    /// A type this code cannot measure ends the run rather than desynchronising it. Everything
    /// parsed before that point is still returned.
    #[test]
    fn an_unreadable_type_stops_the_run_without_losing_what_came_before() {
        let mut data = vec![0u8; 8];
        data.extend_from_slice(&0x5007u16.to_le_bytes());
        data.extend_from_slice(&TYPE_UINT16.to_le_bytes());
        data.push(0x01);
        data.push(0x01);
        data.extend_from_slice(&280u16.to_le_bytes());
        data.extend_from_slice(&280u16.to_le_bytes());
        data.push(FORM_NONE);
        // A string property, whose width this code cannot compute.
        data.extend_from_slice(&0xD500u16.to_le_bytes());
        data.extend_from_slice(&0xFFFFu16.to_le_bytes());
        data.push(0x01);
        data.push(0x01);

        let props = parse_all_props(&data).unwrap();
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].code, 0x5007);
    }

    #[test]
    fn an_empty_inventory_is_an_error_rather_than_no_properties() {
        // Silently reporting zero properties would look like a camera with no controls.
        assert!(parse_all_props(&[0u8; 8]).is_err());
    }
}
