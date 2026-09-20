//! Known-body capability table. Generically accepts any Fujifilm USB device;
//! only explicitly verified bodies skip the "untested" warning.

use serde::Serialize;

use crate::fuji_raw_conv::ptp::codes::FUJI_VENDOR_ID;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraCapability {
    pub product_id: u16,
    pub model_name: &'static str,
    /// Hardware-verified in this fork for RAW CONV. round-trips.
    pub verified: bool,
}

/// Seed table. Extend as bodies are tested — do not invent verified=true.
pub const KNOWN_CAPABILITIES: &[CameraCapability] = &[CameraCapability {
    product_id: 0x0305,
    model_name: "X100VI",
    verified: true,
}];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredCamera {
    pub bus_id: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial_number: Option<String>,
    pub model_name: String,
    pub verified: bool,
    /// Present when the body is not in the verified table.
    pub untested_warning: Option<String>,
}

pub fn lookup_capability(product_id: u16) -> Option<&'static CameraCapability> {
    KNOWN_CAPABILITIES
        .iter()
        .find(|c| c.product_id == product_id)
}

pub fn describe_device(
    bus_id: String,
    vendor_id: u16,
    product_id: u16,
    manufacturer: Option<String>,
    product: Option<String>,
    serial_number: Option<String>,
) -> DiscoveredCamera {
    let cap = lookup_capability(product_id);
    let verified = cap.map(|c| c.verified).unwrap_or(false);
    let model_name = cap
        .map(|c| c.model_name.to_string())
        .or_else(|| product.clone())
        .unwrap_or_else(|| format!("Fujifilm (PID 0x{product_id:04X})"));

    let untested_warning = if vendor_id == FUJI_VENDOR_ID && !verified {
        Some(format!(
            "{model_name} has not been verified with RapidRAW camera RAW conversion yet. \
             Conversion may work, but expect rough edges. The only verified body so far is the X100VI."
        ))
    } else {
        None
    };

    DiscoveredCamera {
        bus_id,
        vendor_id,
        product_id,
        manufacturer,
        product,
        serial_number,
        model_name,
        verified,
        untested_warning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x100vi_is_verified() {
        let cap = lookup_capability(0x0305).unwrap();
        assert!(cap.verified);
        assert_eq!(cap.model_name, "X100VI");
    }

    #[test]
    fn unknown_body_gets_untested_warning() {
        let d = describe_device(
            "1-2".into(),
            FUJI_VENDOR_ID,
            0x02E7,
            None,
            Some("X-T4".into()),
            None,
        );
        assert!(!d.verified);
        assert!(d.untested_warning.is_some());
    }
}
