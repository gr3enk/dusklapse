//! Sony: a profile, and nothing behind it yet.
//!
//! Deliberately a module of its own rather than a special case inside the factory.
//! Everything the app knows about a vendor lives in that vendor's module, and "there is
//! no backend for this one" is a fact about Sony, not about the factory.
//!
//! What it would take: Sony publishes no usable Wi-Fi API. The old Camera Remote API is
//! dead and covered only bodies up to about 2017, and the current Camera Remote SDK is
//! USB and wired Ethernet with no mobile build. That leaves reverse-engineered vendor
//! opcodes on top of the PTP-IP layer in [`super::ptpip`], which Nikon already proved
//! works - the framing is done, only the vendor semantics are missing.

use super::model::{Vendor, VendorProfile};

pub fn profile() -> VendorProfile {
    VendorProfile {
        vendor: Vendor::Sony,
        label: Vendor::Sony.label().to_string(),
        summary: "PTP-IP - no backend yet, Sony publishes no usable Wi-Fi API".into(),
        default_port: Vendor::Sony.default_port(),
        access_point_host: None,
        needs_address: true,
        implemented: false,
    }
}
