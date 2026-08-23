//! PTP-IP transport (ISO 15740 over TCP, port 15740).
//!
//! This is the shared layer Nikon and Sony both sit on, which is why it lives
//! next to the vendor backends rather than inside one of them.
//!
//! # Two sockets
//!
//! PTP-IP's defining quirk: a session needs *two* TCP connections. The first
//! carries commands and data, the second carries events. The camera hands out a
//! connection number on the first and expects it echoed on the second. Skipping
//! the event channel does not work - the handshake is not complete without it.
//!
//! # Serialized by protocol, not by choice
//!
//! PTP is strictly request/response with a monotonic transaction id on one
//! socket, so the command channel sits behind a mutex held across the whole
//! exchange. That is not a bottleneck we introduced; interleaving two operations
//! on one PTP session is simply not legal.
//!
//! # What was verified against hardware
//!
//! Against a Nikon Z 6 on firmware V3.80: the handshake, `OpenSession`,
//! `GetDeviceInfo`, `GetDevicePropDesc` and `SetDevicePropValue` all work.
//! Notably the body's advertised `OperationsSupported` lists only five entries
//! and omits every property operation - while serving them perfectly. Do not
//! gate features on that list; probe instead.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{tcp::OwnedWriteHalf, TcpStream};
use tokio::sync::{broadcast, Mutex};
use tokio::task::JoinHandle;

use super::error::{CameraError, CameraResult};
use super::model::CameraEvent;

/// Translates a raw PTP event into something the app acts on, or discards it.
///
/// Passed in by the vendor backend because the mapping is vendor knowledge -
/// which property codes matter, which events are noise. Taking it here rather
/// than mapping in a second task means one reader owns the event channel.
///
/// A closure rather than a plain function pointer so a backend can record state as
/// events go past - the Nikon one notes the handles of newly written files, which
/// is where previews come from.
pub type EventMapper = Arc<dyn Fn(&PtpEvent) -> Option<CameraEvent> + Send + Sync>;

/// Cameras on a local network answer in milliseconds. A long timeout only hides
/// a body that has gone to sleep behind a stalled UI.
const IO_TIMEOUT: Duration = Duration::from_secs(6);

/// Pulling an image is different: several megabytes over the camera's own access
/// point, which is not a fast link. Applied per packet, so a stalled transfer
/// still fails rather than hanging forever.
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(60);

// Packet types.
const INIT_COMMAND_REQUEST: u32 = 1;
const INIT_COMMAND_ACK: u32 = 2;
const INIT_EVENT_REQUEST: u32 = 3;
const INIT_EVENT_ACK: u32 = 4;
const INIT_FAIL: u32 = 5;
const OPERATION_REQUEST: u32 = 6;
const OPERATION_RESPONSE: u32 = 7;
const EVENT: u32 = 8;
const START_DATA: u32 = 9;
const DATA: u32 = 10;
const END_DATA: u32 = 12;
/// The camera's liveness check. Ignoring these is how a session dies: the body
/// asks whether the client is still there and closes the connection when nothing
/// answers.
const PROBE_REQUEST: u32 = 13;
const PROBE_RESPONSE: u32 = 14;

// Operations.
pub const OP_GET_DEVICE_INFO: u16 = 0x1001;
pub const OP_OPEN_SESSION: u16 = 0x1002;
pub const OP_CLOSE_SESSION: u16 = 0x1003;
pub const OP_GET_OBJECT_INFO: u16 = 0x1008;
pub const OP_GET_OBJECT: u16 = 0x1009;
/// Ask the camera which objects it holds.
///
/// Standard PTP rather than a Nikon extension, so what it enables is not limited to one make.
/// Takes a storage id, a format code and a parent handle - all three accept a wildcard.
pub const OP_GET_STORAGE_IDS: u16 = 0x1004;
pub const OP_GET_OBJECT_HANDLES: u16 = 0x1007;
/// Every storage the camera has, rather than naming one.
pub const STORAGE_ALL: u32 = 0xFFFF_FFFF;
/// The root of the object hierarchy, meaning "do not filter by folder".
pub const PARENT_ANY: u32 = 0x0000_0000;

pub const OP_GET_DEVICE_PROP_DESC: u16 = 0x1014;
pub const OP_SET_DEVICE_PROP_VALUE: u16 = 0x1016;

/// EXIF/JPEG. The only format worth pulling off a card for a preview - asking for
/// the object info first is what keeps a multi-megabyte NEF from ever being
/// transferred.
pub const FORMAT_EXIF_JPEG: u16 = 0x3801;
/// Plain JFIF, in case a body reports that instead.
pub const FORMAT_JFIF: u16 = 0x3808;

pub fn is_jpeg(format: u16) -> bool {
    matches!(format, FORMAT_EXIF_JPEG | FORMAT_JFIF)
}

const RESPONSE_OK: u16 = 0x2001;
const RESPONSE_OPERATION_NOT_SUPPORTED: u16 = 0x2005;
const RESPONSE_DEVICE_PROP_NOT_SUPPORTED: u16 = 0x200A;
/// The body is mid-operation and will not take another one yet.
///
/// Measured on a D5300 and a Z 6: a `SetDevicePropValue` sent while the shutter is open comes
/// back with this. It is not a refusal of the value, only of the timing.
const RESPONSE_DEVICE_BUSY: u16 = 0x2019;

// Data types.
const TYPE_INT8: u16 = 0x0001;
const TYPE_UINT8: u16 = 0x0002;
const TYPE_INT16: u16 = 0x0003;
const TYPE_UINT16: u16 = 0x0004;
const TYPE_INT32: u16 = 0x0005;
const TYPE_UINT32: u16 = 0x0006;

const PROTOCOL_VERSION: u32 = 0x0001_0000;

/// Identifies this client to the camera.
///
/// Deliberately constant rather than random: Nikon associates a pairing with the
/// client GUID, so a stable value is what lets a body recognise us across
/// sessions. It should become per-install persistent once there is somewhere to
/// store it - a shared constant means two phones look like the same client.
const CLIENT_GUID: [u8; 16] = [
    0x64, 0x75, 0x73, 0x6b, 0x6c, 0x61, 0x70, 0x73, 0x65, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x01,
];

/// A PTP property value, in whatever width the property declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtpValue {
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
}

impl PtpValue {
    /// Numeric value, widened. Every exposure property we touch is an unsigned
    /// count, so this is the form callers actually want.
    pub fn as_u32(self) -> u32 {
        match self {
            PtpValue::I8(v) => v as u32,
            PtpValue::U8(v) => v as u32,
            PtpValue::I16(v) => v as u32,
            PtpValue::U16(v) => v as u32,
            PtpValue::I32(v) => v as u32,
            PtpValue::U32(v) => v,
        }
    }
}

/// The constraint a property puts on its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Form {
    Any,
    Range {
        min: PtpValue,
        max: PtpValue,
        step: PtpValue,
    },
    /// The selectable values. This is the PTP counterpart to Canon's `ability`
    /// list, and what a ramp snaps onto.
    Enumeration(Vec<PtpValue>),
}

#[derive(Debug, Clone)]
pub struct PropDesc {
    pub property: u16,
    pub datatype: u16,
    pub writable: bool,
    pub current: PtpValue,
    pub form: Form,
}

/// What the camera knows about one file on the card, without transferring it.
///
/// Reading this before fetching is the whole trick behind JPEG-only previews:
/// `format` says what a file is for the cost of a few dozen bytes, so a RAW never
/// has to cross the network to be recognised and rejected.
#[derive(Debug, Clone)]
pub struct ObjectInfo {
    pub format: u16,
    pub compressed_size: u32,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub filename: String,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceInfo {
    pub manufacturer: String,
    pub model: String,
    pub device_version: String,
    pub serial: String,
    /// Which vendor extension set the body claims to speak.
    ///
    /// Worth keeping because it is the first hint of where a vendor's own opcodes live: a body
    /// declaring an extension is announcing that the standard property codes are not the whole
    /// story.
    pub vendor_extension_id: u32,
    pub operations: Vec<u16>,
    pub events: Vec<u16>,
    pub properties: Vec<u16>,
}

/// Something the camera volunteered on the event channel.
///
/// With an external intervalometer driving the shutter, these events are the only
/// way the app learns a frame happened - and therefore what a ramp advances on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtpEvent {
    pub code: u16,
    pub params: Vec<u32>,
}

/// One *file* was written. Measured on a Z 6: a single exposure shooting RAW+JPEG
/// emits this **twice**, with handles from two different ranges. Counting frames
/// with this would run a ramp at double speed - use [`EVENT_CAPTURE_COMPLETE`].
pub const EVENT_OBJECT_ADDED: u16 = 0x4002;
pub const EVENT_DEVICE_PROP_CHANGED: u16 = 0x4006;
/// The camera changed its device profile. A Z 6 emits this as it leaves the
/// connect-to-computer pairing screen, immediately before closing the session.
pub const EVENT_DEVICE_INFO_CHANGED: u16 = 0x4008;
/// One *exposure* finished. Exactly one per frame regardless of how many files it
/// produced, which makes this the frame signal a ramp should count.
pub const EVENT_CAPTURE_COMPLETE: u16 = 0x400D;

/// Events are dropped rather than queued without bound if nobody is listening -
/// a stalled consumer must not be able to make the camera connection back up.
const EVENT_BUFFER: usize = 64;

pub struct PtpIp {
    command: Mutex<Channel>,
    /// Drains the event channel. The socket has to stay open for the session to
    /// survive, and something has to consume what arrives on it or the camera
    /// eventually blocks on a full buffer.
    events: JoinHandle<()>,
    /// Raw protocol events, for diagnostics.
    event_tx: broadcast::Sender<PtpEvent>,
    /// The subset the app acts on.
    camera_event_tx: broadcast::Sender<CameraEvent>,
    /// Whether the camera has ever volunteered anything on the event channel.
    ///
    /// The one fact that separates a body which reports its own frames from one that has to be
    /// asked, and it cannot be known at connect time - only by waiting to see.
    saw_event: Arc<AtomicBool>,
    device_info: DeviceInfo,
}

struct Channel {
    stream: TcpStream,
    next_transaction: u32,
}

impl PtpIp {
    pub async fn connect(
        host: &str,
        port: u16,
        client_name: &str,
        mapper: EventMapper,
    ) -> CameraResult<Self> {
        let mut command = connect_socket(host, port).await?;

        let mut payload = Vec::new();
        payload.extend_from_slice(&CLIENT_GUID);
        payload.extend_from_slice(&utf16z(client_name));
        payload.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
        write_packet(&mut command, INIT_COMMAND_REQUEST, &payload).await?;

        let (kind, body) = read_packet(&mut command, IO_TIMEOUT).await?;
        if kind == INIT_FAIL {
            let reason = Reader::new(&body).u32().unwrap_or(0);
            return Err(CameraError::Rejected {
                status: reason as u16,
                message: "the camera refused the connection - put it back into \
                          its connect-to-computer screen and try again"
                    .into(),
            });
        }
        if kind != INIT_COMMAND_ACK {
            return Err(CameraError::Protocol(format!(
                "expected Init_Command_Ack, got packet type {kind}"
            )));
        }

        let mut reader = Reader::new(&body);
        let connection_number = reader.u32()?;
        reader.take(16)?; // camera GUID
        let camera_name = reader.utf16z().unwrap_or_default();
        log::info!("PTP-IP handshake with {camera_name:?}, connection #{connection_number}");

        // The event channel is not optional; the session is incomplete without it.
        let mut event = connect_socket(host, port).await?;
        write_packet(
            &mut event,
            INIT_EVENT_REQUEST,
            &connection_number.to_le_bytes(),
        )
        .await?;
        let (kind, _) = read_packet(&mut event, IO_TIMEOUT).await?;
        if kind != INIT_EVENT_ACK {
            return Err(CameraError::Protocol(format!(
                "expected Init_Event_Ack, got packet type {kind}"
            )));
        }

        let (event_tx, _) = broadcast::channel(EVENT_BUFFER);
        let (camera_event_tx, _) = broadcast::channel(EVENT_BUFFER);
        let saw_event = Arc::new(AtomicBool::new(false));
        let events = tokio::spawn(drain_events(
            event,
            event_tx.clone(),
            camera_event_tx.clone(),
            mapper,
            saw_event.clone(),
        ));

        // Built before the device info is known, then filled in: `PtpIp` has a
        // `Drop` impl, so it cannot be rebuilt from its own pieces afterwards.
        let mut session = Self {
            command: Mutex::new(Channel {
                stream: command,
                next_transaction: 0,
            }),
            events,
            event_tx,
            camera_event_tx,
            saw_event,
            device_info: DeviceInfo::default(),
        };

        // Session id must be non-zero.
        session.operation(OP_OPEN_SESSION, &[1], IO_TIMEOUT).await?;
        let raw = session
            .operation(OP_GET_DEVICE_INFO, &[], IO_TIMEOUT)
            .await?;
        session.device_info = parse_device_info(&raw)?;
        log::info!(
            "PTP-IP session open: {} {} ({})",
            session.device_info.manufacturer,
            session.device_info.model,
            session.device_info.device_version
        );
        // Logged rather than acted on. A Z 6 advertises five operations and omits
        // every property operation it then happily serves, so these lists are
        // only ever a debugging hint - never a feature gate.
        log::debug!(
            "camera advertises operations {:04x?} and properties {:04x?}",
            session.device_info.operations,
            session.device_info.properties
        );

        Ok(session)
    }

    pub fn device_info(&self) -> &DeviceInfo {
        &self.device_info
    }

    /// Every event the camera sends, unfiltered. For diagnostics.
    ///
    /// Subscribers that fall behind lose what they missed rather than stalling the
    /// session - a slow consumer must never be able to make the connection back up.
    pub fn subscribe(&self) -> broadcast::Receiver<PtpEvent> {
        self.event_tx.subscribe()
    }

    /// Only the events the app acts on, as decided by the backend's mapper.
    /// Whether a transaction is in flight on the command channel.
    ///
    /// PTP allows one at a time, so anything asked while this is true simply waits its turn - and
    /// a caller that has nothing urgent to say is better off skipping its turn entirely than
    /// queueing behind a multi-megabyte image and firing the moment it lands.
    pub fn is_busy(&self) -> bool {
        self.command.try_lock().is_err()
    }

    /// Whether the camera has volunteered anything on the event channel so far.
    pub fn saw_event(&self) -> bool {
        self.saw_event.load(Ordering::Relaxed)
    }

    /// Announce something the transport worked out for itself.
    ///
    /// For a body that reports nothing: the frame has to reach the same channel a talkative
    /// camera's would, so that everything downstream is unaware of the difference.
    pub fn emit(&self, event: CameraEvent) {
        // Err only means nobody is listening, which is fine.
        let _ = self.camera_event_tx.send(event);
    }

    pub fn subscribe_camera(&self) -> broadcast::Receiver<CameraEvent> {
        self.camera_event_tx.subscribe()
    }

    pub async fn prop_desc(&self, property: u16) -> CameraResult<PropDesc> {
        let raw = self
            .operation(OP_GET_DEVICE_PROP_DESC, &[property as u32], IO_TIMEOUT)
            .await?;
        let desc = parse_prop_desc(&raw)?;

        // A reply about a different property means the transaction stream has
        // desynchronised. Catching it here beats writing a shutter speed into an
        // aperture.
        if desc.property != property {
            return Err(CameraError::Protocol(format!(
                "asked about property 0x{property:04x} but the camera described \
                 0x{:04x}",
                desc.property
            )));
        }
        Ok(desc)
    }

    /// Ask what a file is without downloading it.
    /// Handles of the objects the camera holds, optionally of one format only.
    ///
    /// Filtering by format is what keeps this cheap enough to ask repeatedly: a card with a
    /// thousand exposures answers with the JPEGs alone, and a handle is four bytes.
    ///
    /// This exists for bodies that never volunteer anything on the event channel - a D5300 opens
    /// the channel, says nothing for eleven minutes and then closes it - where asking is the only
    /// way to learn that a frame was taken.
    pub async fn object_handles(&self, format: u16) -> CameraResult<Vec<u32>> {
        self.object_handles_in(STORAGE_ALL, format).await
    }

    /// The same, naming one storage rather than asking for all of them.
    ///
    /// Not every body accepts the wildcard. A ZV-E10 answers `Invalid_StorageID` (0x2008) to it and
    /// wants an identifier from [`PtpIp::storage_ids`], so a caller that has one should say so.
    pub async fn object_handles_in(&self, storage: u32, format: u16) -> CameraResult<Vec<u32>> {
        let raw = self
            .operation(
                OP_GET_OBJECT_HANDLES,
                &[storage, format as u32, PARENT_ANY],
                IO_TIMEOUT,
            )
            .await?;
        parse_u32_array(&raw)
    }

    /// The storages the camera has, usually one card.
    pub async fn storage_ids(&self) -> CameraResult<Vec<u32>> {
        let raw = self.operation(OP_GET_STORAGE_IDS, &[], IO_TIMEOUT).await?;
        parse_u32_array(&raw)
    }

    pub async fn object_info(&self, handle: u32) -> CameraResult<ObjectInfo> {
        let raw = self
            .operation(OP_GET_OBJECT_INFO, &[handle], IO_TIMEOUT)
            .await?;
        parse_object_info(&raw)
    }

    /// Download a file whole.
    ///
    /// Check [`PtpIp::object_info`] first. This blocks the command channel for the
    /// duration of the transfer - PTP allows one transaction at a time - so pulling
    /// a RAW by mistake would stall every other read for as long as it takes.
    pub async fn object(&self, handle: u32) -> CameraResult<Vec<u8>> {
        self.operation(OP_GET_OBJECT, &[handle], TRANSFER_TIMEOUT)
            .await
    }

    /// Write a property, encoding the payload at the width the property declares.
    pub async fn set_prop(&self, property: u16, datatype: u16, value: u32) -> CameraResult<()> {
        let payload = match datatype {
            TYPE_INT8 | TYPE_UINT8 => vec![value as u8],
            TYPE_INT16 | TYPE_UINT16 => (value as u16).to_le_bytes().to_vec(),
            TYPE_INT32 | TYPE_UINT32 => value.to_le_bytes().to_vec(),
            other => {
                return Err(CameraError::Protocol(format!(
                    "cannot write property 0x{property:04x} of data type 0x{other:04x}"
                )))
            }
        };
        self.operation_out(OP_SET_DEVICE_PROP_VALUE, &[property as u32], &payload)
            .await
            .map(|_| ())
    }

    pub async fn close(&self) -> CameraResult<()> {
        let result = self
            .operation(OP_CLOSE_SESSION, &[], IO_TIMEOUT)
            .await
            .map(|_| ());
        self.events.abort();
        result
    }

    /// Run a vendor's own opcode and return whatever data phase comes back.
    ///
    /// The escape hatch for a body whose controls are not reachable through the standard
    /// operations at all. Sony is the case this exists for: a ZV-E10 advertises no
    /// `GetDevicePropDesc` and an empty property list, and keeps everything behind its own
    /// 0x92xx operations instead.
    ///
    /// Deliberately raw. The transport has no idea what these codes mean and should not: which
    /// opcode does what is vendor knowledge and belongs in the vendor's module.
    pub async fn vendor_operation(&self, opcode: u16, params: &[u32]) -> CameraResult<Vec<u8>> {
        self.operation(opcode, params, IO_TIMEOUT).await
    }

    /// The same, for an opcode that carries data to the camera.
    pub async fn vendor_operation_out(
        &self,
        opcode: u16,
        params: &[u32],
        data: &[u8],
    ) -> CameraResult<()> {
        self.operation_out(opcode, params, data).await.map(|_| ())
    }

    /// An operation with no data-out phase. Returns the data-in bytes, empty when
    /// the operation carries none.
    async fn operation(
        &self,
        opcode: u16,
        params: &[u32],
        timeout: Duration,
    ) -> CameraResult<Vec<u8>> {
        let mut channel = self.command.lock().await;
        let transaction = channel.take_transaction();
        write_packet(
            &mut channel.stream,
            OPERATION_REQUEST,
            &operation_payload(1, opcode, transaction, params),
        )
        .await?;
        read_until_response(&mut channel.stream, opcode, timeout).await
    }

    /// An operation that sends data to the camera.
    async fn operation_out(
        &self,
        opcode: u16,
        params: &[u32],
        data: &[u8],
    ) -> CameraResult<Vec<u8>> {
        let mut channel = self.command.lock().await;
        let transaction = channel.take_transaction();

        // Data phase info 2 announces the data-out phase that follows.
        write_packet(
            &mut channel.stream,
            OPERATION_REQUEST,
            &operation_payload(2, opcode, transaction, params),
        )
        .await?;

        let mut start = transaction.to_le_bytes().to_vec();
        start.extend_from_slice(&(data.len() as u64).to_le_bytes());
        write_packet(&mut channel.stream, START_DATA, &start).await?;

        let mut end = transaction.to_le_bytes().to_vec();
        end.extend_from_slice(data);
        write_packet(&mut channel.stream, END_DATA, &end).await?;

        read_until_response(&mut channel.stream, opcode, IO_TIMEOUT).await
    }
}

impl Drop for PtpIp {
    fn drop(&mut self) {
        self.events.abort();
    }
}

impl Channel {
    fn take_transaction(&mut self) -> u32 {
        let transaction = self.next_transaction;
        self.next_transaction = self.next_transaction.wrapping_add(1);
        transaction
    }
}

async fn connect_socket(host: &str, port: u16) -> CameraResult<TcpStream> {
    let stream = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| CameraError::Transport(format!("no answer from {host}:{port}")))?
        .map_err(|err| CameraError::Transport(format!("{host}:{port}: {err}")))?;
    // PTP exchanges are small and latency-sensitive; Nagle would batch them.
    let _ = stream.set_nodelay(true);
    Ok(stream)
}

fn operation_payload(data_phase: u32, opcode: u16, transaction: u32, params: &[u32]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(10 + params.len() * 4);
    payload.extend_from_slice(&data_phase.to_le_bytes());
    payload.extend_from_slice(&opcode.to_le_bytes());
    payload.extend_from_slice(&transaction.to_le_bytes());
    for param in params {
        payload.extend_from_slice(&param.to_le_bytes());
    }
    payload
}

/// Collect the data phase, then interpret the response code.
async fn read_until_response(
    stream: &mut TcpStream,
    opcode: u16,
    timeout: Duration,
) -> CameraResult<Vec<u8>> {
    let mut data = Vec::new();
    loop {
        let (kind, body) = read_packet(stream, timeout).await?;
        match kind {
            START_DATA => {}
            // Both carry a 4-byte transaction id ahead of the payload.
            DATA | END_DATA => data.extend_from_slice(body.get(4..).unwrap_or_default()),
            EVENT => {}
            OPERATION_RESPONSE => {
                let code = Reader::new(&body).u16()?;
                return match code {
                    RESPONSE_OK => Ok(data),
                    RESPONSE_OPERATION_NOT_SUPPORTED => Err(CameraError::Unavailable(format!(
                        "operation 0x{opcode:04x}"
                    ))),
                    RESPONSE_DEVICE_PROP_NOT_SUPPORTED => {
                        Err(CameraError::Unavailable("this setting".into()))
                    }
                    RESPONSE_DEVICE_BUSY => Err(CameraError::Busy),
                    other => Err(CameraError::Rejected {
                        status: other,
                        message: format!(
                            "operation 0x{opcode:04x} failed with PTP response 0x{other:04x}"
                        ),
                    }),
                };
            }
            other => {
                return Err(CameraError::Protocol(format!(
                    "unexpected packet type {other} during operation 0x{opcode:04x}"
                )))
            }
        }
    }
}

/// Own the event channel for the life of the session.
///
/// Two jobs. It republishes events to whoever subscribed, and - the part that
/// keeps the session alive at all - it answers the camera's `Probe_Request` with
/// a `Probe_Response`. A body that gets no answer to a probe concludes the client
/// is gone and closes the connection, which looks exactly like the camera going
/// to sleep on its own.
async fn drain_events(
    stream: TcpStream,
    raw: broadcast::Sender<PtpEvent>,
    mapped: broadcast::Sender<CameraEvent>,
    mapper: EventMapper,
    saw_event: Arc<AtomicBool>,
) {
    let (mut read, mut write): (_, OwnedWriteHalf) = stream.into_split();

    let mut probed = false;
    let mut header = [0u8; 8];
    loop {
        if read.read_exact(&mut header).await.is_err() {
            log::info!("PTP-IP event channel closed");
            return;
        }
        let length = u32::from_le_bytes(header[0..4].try_into().unwrap()) as usize;
        let kind = u32::from_le_bytes(header[4..8].try_into().unwrap());
        if length < 8 {
            log::warn!("PTP-IP event packet with impossible length {length}");
            return;
        }
        let mut body = vec![0u8; length - 8];
        if read.read_exact(&mut body).await.is_err() {
            return;
        }

        match kind {
            PROBE_REQUEST => {
                if !probed {
                    probed = true;
                    // Once, at info: on a body whose session keeps dropping, whether the camera
                    // checks for us at all is the first thing worth knowing, and the log view a
                    // user can reach only shows info.
                    log::info!("camera probes for liveness; answering its probes from here on");
                }
                log::debug!("PTP-IP probe from camera, answering");
                if write_packet(&mut write, PROBE_RESPONSE, &[]).await.is_err() {
                    log::warn!("could not answer the camera's probe; session will drop");
                    return;
                }
            }
            EVENT => {
                if let Some(event) = parse_event(&body) {
                    log::info!("PTP-IP event 0x{:04x} {:?}", event.code, event.params);
                    // Deliberately not set for a probe: a keep-alive says the socket is open,
                    // not that the camera reports what it is doing.
                    saw_event.store(true, Ordering::Relaxed);
                    if let Some(mapped_event) = mapper(&event) {
                        // Err only means nobody is listening, which is fine.
                        let _ = mapped.send(mapped_event);
                    }
                    let _ = raw.send(event);
                }
            }
            other => log::debug!("ignoring packet type {other} on the event channel"),
        }
    }
}

/// Event dataset: code, transaction id, then up to three parameters.
fn parse_event(body: &[u8]) -> Option<PtpEvent> {
    let mut reader = Reader::new(body);
    let code = reader.u16().ok()?;
    reader.u32().ok()?; // transaction id
    let mut params = Vec::new();
    while let Ok(param) = reader.u32() {
        params.push(param);
    }
    Some(PtpEvent { code, params })
}

/// Generic over the writer so the event channel's write half can answer probes
/// with the same framing the command channel uses.
async fn write_packet<W: tokio::io::AsyncWrite + Unpin>(
    stream: &mut W,
    kind: u32,
    payload: &[u8],
) -> CameraResult<()> {
    let mut frame = Vec::with_capacity(8 + payload.len());
    frame.extend_from_slice(&((8 + payload.len()) as u32).to_le_bytes());
    frame.extend_from_slice(&kind.to_le_bytes());
    frame.extend_from_slice(payload);

    tokio::time::timeout(IO_TIMEOUT, stream.write_all(&frame))
        .await
        .map_err(|_| CameraError::Transport("timed out writing to the camera".into()))?
        .map_err(|err| CameraError::Transport(err.to_string()))
}

async fn read_packet(stream: &mut TcpStream, timeout: Duration) -> CameraResult<(u32, Vec<u8>)> {
    let mut header = [0u8; 8];
    tokio::time::timeout(timeout, stream.read_exact(&mut header))
        .await
        .map_err(|_| CameraError::Transport("the camera stopped answering".into()))?
        .map_err(|err| CameraError::Transport(err.to_string()))?;

    let length = u32::from_le_bytes(header[0..4].try_into().unwrap()) as usize;
    let kind = u32::from_le_bytes(header[4..8].try_into().unwrap());
    if length < 8 {
        return Err(CameraError::Protocol(format!(
            "packet claims an impossible length of {length} bytes"
        )));
    }

    let mut body = vec![0u8; length - 8];
    tokio::time::timeout(timeout, stream.read_exact(&mut body))
        .await
        .map_err(|_| CameraError::Transport("the camera stopped mid-packet".into()))?
        .map_err(|err| CameraError::Transport(err.to_string()))?;

    Ok((kind, body))
}

fn utf16z(text: &str) -> Vec<u8> {
    let mut bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    bytes.extend_from_slice(&[0, 0]);
    bytes
}

fn parse_device_info(data: &[u8]) -> CameraResult<DeviceInfo> {
    let mut reader = Reader::new(data);
    reader.u16()?; // standard version
    let vendor_extension_id = reader.u32()?;
    reader.u16()?; // vendor extension version
    reader.ptp_string()?; // vendor extension description
    reader.u16()?; // functional mode

    let operations = reader.u16_array()?;
    let events = reader.u16_array()?;
    let properties = reader.u16_array()?;
    reader.u16_array()?; // capture formats
    reader.u16_array()?; // image formats

    Ok(DeviceInfo {
        manufacturer: reader.ptp_string()?,
        model: reader.ptp_string()?,
        device_version: reader.ptp_string()?,
        serial: reader.ptp_string()?,
        vendor_extension_id,
        operations,
        events,
        properties,
    })
}

/// ObjectInfo dataset. Fixed-width preamble, then four PTP strings.
///
/// Everything past the filename is date and keyword metadata we have no use for,
/// so parsing stops there.
/// A PTP array: a count, then that many 32-bit values.
fn parse_u32_array(raw: &[u8]) -> CameraResult<Vec<u32>> {
    let mut reader = Reader::new(raw);
    let count = reader.u32()? as usize;
    // Guarded against a length the payload cannot possibly hold, which would otherwise be a very
    // large allocation on the word of a device that has already surprised us once.
    if count > (raw.len() - 4) / 4 {
        return Err(CameraError::Protocol(format!(
            "object handle array claims {count} entries but carries {} bytes",
            raw.len() - 4
        )));
    }
    (0..count).map(|_| reader.u32()).collect()
}

fn parse_object_info(data: &[u8]) -> CameraResult<ObjectInfo> {
    let mut reader = Reader::new(data);
    reader.u32()?; // storage id
    let format = reader.u16()?;
    reader.u16()?; // protection status
    let compressed_size = reader.u32()?;
    reader.u16()?; // thumb format
    reader.u32()?; // thumb compressed size
    reader.u32()?; // thumb width
    reader.u32()?; // thumb height
    let pixel_width = reader.u32()?;
    let pixel_height = reader.u32()?;
    reader.u32()?; // image bit depth
    reader.u32()?; // parent object
    reader.u16()?; // association type
    reader.u32()?; // association description
    reader.u32()?; // sequence number

    Ok(ObjectInfo {
        format,
        compressed_size,
        pixel_width,
        pixel_height,
        filename: reader.ptp_string()?,
    })
}

fn parse_prop_desc(data: &[u8]) -> CameraResult<PropDesc> {
    let mut reader = Reader::new(data);
    let property = reader.u16()?;
    let datatype = reader.u16()?;
    let writable = reader.u8()? == 1;

    reader.typed(datatype)?; // factory default
    let current = reader.typed(datatype)?;

    let form = match reader.u8()? {
        1 => Form::Range {
            min: reader.typed(datatype)?,
            max: reader.typed(datatype)?,
            step: reader.typed(datatype)?,
        },
        2 => {
            let count = reader.u16()?;
            let mut values = Vec::with_capacity(count as usize);
            for _ in 0..count {
                values.push(reader.typed(datatype)?);
            }
            Form::Enumeration(values)
        }
        _ => Form::Any,
    };

    Ok(PropDesc {
        property,
        datatype,
        writable,
        current,
        form,
    })
}

/// Cursor over a PTP dataset.
pub(super) struct Reader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    pub(super) fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    /// The next `u16` without consuming it.
    ///
    /// Needed where a field's meaning depends on its own value - a vendor descriptor stream whose
    /// next word is either a count or the start of the following record.
    pub(super) fn peek_u16(&self) -> CameraResult<u16> {
        let bytes = self
            .data
            .get(self.offset..self.offset + 2)
            .ok_or_else(short)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    /// How many bytes are still unread. A vendor descriptor stream is a run of records with no
    /// count in front of it, so the only way to know it is finished is to watch this.
    pub(super) fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.offset)
    }

    pub(super) fn take(&mut self, count: usize) -> CameraResult<&'a [u8]> {
        let end = self.offset.checked_add(count).ok_or_else(short)?;
        let slice = self.data.get(self.offset..end).ok_or_else(short)?;
        self.offset = end;
        Ok(slice)
    }

    pub(super) fn u8(&mut self) -> CameraResult<u8> {
        Ok(self.take(1)?[0])
    }

    pub(super) fn u16(&mut self) -> CameraResult<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    pub(super) fn u32(&mut self) -> CameraResult<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn typed(&mut self, datatype: u16) -> CameraResult<PtpValue> {
        match datatype {
            TYPE_INT8 => Ok(PtpValue::I8(self.u8()? as i8)),
            TYPE_UINT8 => Ok(PtpValue::U8(self.u8()?)),
            TYPE_INT16 => Ok(PtpValue::I16(self.u16()? as i16)),
            TYPE_UINT16 => Ok(PtpValue::U16(self.u16()?)),
            TYPE_INT32 => Ok(PtpValue::I32(self.u32()? as i32)),
            TYPE_UINT32 => Ok(PtpValue::U32(self.u32()?)),
            other => Err(CameraError::Protocol(format!(
                "unsupported PTP data type 0x{other:04x}"
            ))),
        }
    }

    /// PTP string: a character count that includes the terminator, then UTF-16LE.
    pub(super) fn ptp_string(&mut self) -> CameraResult<String> {
        let count = self.u8()? as usize;
        if count == 0 {
            return Ok(String::new());
        }
        let raw = self.take(count * 2)?;
        let units: Vec<u16> = raw
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes(pair.try_into().unwrap()))
            .take_while(|unit| *unit != 0)
            .collect();
        String::from_utf16(&units)
            .map_err(|err| CameraError::Protocol(format!("malformed string from camera: {err}")))
    }

    /// Null-terminated UTF-16LE, used by the handshake packets.
    fn utf16z(&mut self) -> CameraResult<String> {
        let mut units = Vec::new();
        loop {
            let unit = self.u16()?;
            if unit == 0 {
                break;
            }
            units.push(unit);
        }
        String::from_utf16(&units)
            .map_err(|err| CameraError::Protocol(format!("malformed string from camera: {err}")))
    }

    pub(super) fn u16_array(&mut self) -> CameraResult<Vec<u16>> {
        let count = self.u32()? as usize;
        let raw = self.take(count * 2)?;
        Ok(raw
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes(pair.try_into().unwrap()))
            .collect())
    }
}

fn short() -> CameraError {
    CameraError::Protocol("the camera's reply ended sooner than its structure implies".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_ptp_string() {
        // count 3 (including terminator), then "Z6\0" as UTF-16LE.
        let data = [3, b'Z', 0, b'6', 0, 0, 0];
        assert_eq!(Reader::new(&data).ptp_string().unwrap(), "Z6");
    }

    #[test]
    fn an_empty_ptp_string_is_a_bare_zero() {
        let data = [0u8];
        assert_eq!(Reader::new(&data).ptp_string().unwrap(), "");
    }

    #[test]
    fn a_truncated_reply_is_an_error_not_a_panic() {
        let data = [9, b'Z', 0];
        assert!(Reader::new(&data).ptp_string().is_err());
        assert!(Reader::new(&[1u8]).u32().is_err());
    }

    #[test]
    fn reads_a_uint16_array() {
        let mut data = 3u32.to_le_bytes().to_vec();
        for value in [0x1001u16, 0x1002, 0x9207] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        assert_eq!(
            Reader::new(&data).u16_array().unwrap(),
            vec![0x1001, 0x1002, 0x9207]
        );
    }

    /// Byte-for-byte against a real Z 6 reply for FNumber: UINT16, writable,
    /// enum of 20 values, currently f/2.8.
    #[test]
    fn parses_a_prop_desc_enumeration() {
        let values: [u16; 20] = [
            180, 200, 220, 250, 280, 320, 350, 400, 450, 500, 560, 630, 710, 800, 900, 1000, 1100,
            1300, 1400, 1600,
        ];

        let mut data = Vec::new();
        data.extend_from_slice(&0x5007u16.to_le_bytes()); // FNumber
        data.extend_from_slice(&TYPE_UINT16.to_le_bytes());
        data.push(1); // writable
        data.extend_from_slice(&280u16.to_le_bytes()); // factory default
        data.extend_from_slice(&280u16.to_le_bytes()); // current
        data.push(2); // enumeration
        data.extend_from_slice(&(values.len() as u16).to_le_bytes());
        for value in values {
            data.extend_from_slice(&value.to_le_bytes());
        }

        let desc = parse_prop_desc(&data).unwrap();
        assert_eq!(desc.property, 0x5007);
        assert!(desc.writable);
        assert_eq!(desc.current, PtpValue::U16(280));
        match desc.form {
            Form::Enumeration(items) => {
                assert_eq!(items.len(), 20);
                assert_eq!(items[0], PtpValue::U16(180));
                assert_eq!(items[19], PtpValue::U16(1600));
            }
            other => panic!("expected an enumeration, got {other:?}"),
        }
    }

    /// The dataset a Z 6 returns for a JPEG. Getting the fixed-width preamble wrong
    /// by one field would misread the format and let a RAW through.
    #[test]
    fn parses_an_object_handle_array() {
        let mut data = 3u32.to_le_bytes().to_vec();
        for handle in [0x0000_0001u32, 0x2900_0011, 0xFFFF_0000] {
            data.extend_from_slice(&handle.to_le_bytes());
        }
        assert_eq!(
            parse_u32_array(&data).unwrap(),
            vec![0x0000_0001, 0x2900_0011, 0xFFFF_0000]
        );
    }

    /// An empty card is a valid answer, not an error - and it is what a fresh session sees before
    /// anything has been shot.
    #[test]
    fn parses_an_empty_object_handle_array() {
        assert_eq!(
            parse_u32_array(&0u32.to_le_bytes()).unwrap(),
            Vec::<u32>::new()
        );
    }

    /// A count the payload cannot hold must be refused rather than believed. The device has
    /// already shown it does not behave the way the specification suggests.
    #[test]
    fn refuses_an_object_handle_count_the_payload_cannot_hold() {
        let mut data = 1_000_000u32.to_le_bytes().to_vec();
        data.extend_from_slice(&7u32.to_le_bytes());
        assert!(parse_u32_array(&data).is_err());
    }

    #[test]
    fn parses_object_info_and_identifies_a_jpeg() {
        let mut data = Vec::new();
        data.extend_from_slice(&0x0001_0001u32.to_le_bytes()); // storage id
        data.extend_from_slice(&FORMAT_EXIF_JPEG.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes()); // protection
        data.extend_from_slice(&1_234_567u32.to_le_bytes()); // compressed size
        data.extend_from_slice(&FORMAT_EXIF_JPEG.to_le_bytes()); // thumb format
        data.extend_from_slice(&8_000u32.to_le_bytes()); // thumb size
        data.extend_from_slice(&160u32.to_le_bytes()); // thumb width
        data.extend_from_slice(&120u32.to_le_bytes()); // thumb height
        data.extend_from_slice(&6048u32.to_le_bytes()); // image width
        data.extend_from_slice(&4024u32.to_le_bytes()); // image height
        data.extend_from_slice(&24u32.to_le_bytes()); // bit depth
        data.extend_from_slice(&0u32.to_le_bytes()); // parent
        data.extend_from_slice(&0u16.to_le_bytes()); // association type
        data.extend_from_slice(&0u32.to_le_bytes()); // association desc
        data.extend_from_slice(&1u32.to_le_bytes()); // sequence number
                                                     // Filename "A.JPG" as a PTP string: 6 chars including the terminator.
        data.push(6);
        for unit in "A.JPG\0".encode_utf16() {
            data.extend_from_slice(&unit.to_le_bytes());
        }

        let info = parse_object_info(&data).unwrap();
        assert_eq!(info.filename, "A.JPG");
        assert_eq!(info.compressed_size, 1_234_567);
        assert_eq!((info.pixel_width, info.pixel_height), (6048, 4024));
        assert!(is_jpeg(info.format));
    }

    #[test]
    fn nikon_raw_is_not_mistaken_for_a_jpeg() {
        // 0xB101 is Nikon's vendor format for NEF; 0x3000 is "undefined".
        assert!(!is_jpeg(0xB101));
        assert!(!is_jpeg(0x3000));
        assert!(!is_jpeg(0x3801 + 1));
        assert!(is_jpeg(FORMAT_EXIF_JPEG));
        assert!(is_jpeg(FORMAT_JFIF));
    }

    #[test]
    fn encodes_the_handshake_name_as_null_terminated_utf16() {
        assert_eq!(utf16z("Z6"), vec![b'Z', 0, b'6', 0, 0, 0]);
    }

    #[test]
    fn operation_payload_matches_the_wire_layout() {
        let payload = operation_payload(1, OP_GET_DEVICE_PROP_DESC, 7, &[0x500D]);
        assert_eq!(&payload[0..4], &1u32.to_le_bytes()); // data phase
        assert_eq!(&payload[4..6], &0x1014u16.to_le_bytes()); // opcode
        assert_eq!(&payload[6..10], &7u32.to_le_bytes()); // transaction
        assert_eq!(&payload[10..14], &0x500Du32.to_le_bytes()); // param 1
        assert_eq!(payload.len(), 14);
    }
}
