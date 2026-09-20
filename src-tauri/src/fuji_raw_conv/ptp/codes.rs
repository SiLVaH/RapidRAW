//! Standard PTP (ISO 15740) operation / response codes and public USB IDs.
//!
//! Fujifilm vendor property encodings belong in later phases (preset encode/decode).
//! Values here are either from the public PTP specification or public USB-IF IDs.

/// Fujifilm USB vendor ID (USB-IF).
pub const FUJI_VENDOR_ID: u16 = 0x04CB;

/// PTP container types (ISO 15740).
pub mod container_type {
    pub const COMMAND: u16 = 0x0001;
    pub const DATA: u16 = 0x0002;
    pub const RESPONSE: u16 = 0x0003;
    pub const EVENT: u16 = 0x0004;
}

/// Standard PTP operation codes used by the transport/session layer.
#[allow(dead_code)] // Get/SetDevicePropValue are used from A2 onward.
pub mod op {
    pub const GET_DEVICE_INFO: u16 = 0x1001;
    pub const OPEN_SESSION: u16 = 0x1002;
    pub const CLOSE_SESSION: u16 = 0x1003;
    pub const GET_DEVICE_PROP_DESC: u16 = 0x1014;
    pub const GET_DEVICE_PROP_VALUE: u16 = 0x1015;
    pub const SET_DEVICE_PROP_VALUE: u16 = 0x1016;
}

/// Standard PTP response codes.
#[allow(dead_code)] // GENERAL_ERROR / OPERATION_NOT_SUPPORTED matched via raw u16 today.
pub mod response {
    pub const OK: u16 = 0x2001;
    pub const GENERAL_ERROR: u16 = 0x2002;
    pub const SESSION_NOT_OPEN: u16 = 0x2003;
    pub const OPERATION_NOT_SUPPORTED: u16 = 0x2005;
    pub const SESSION_ALREADY_OPEN: u16 = 0x201E;
}

/// Fujifilm vendor operations / properties used only to probe RAW CONV mode.
/// Property *value encodings* are intentionally not defined here (phase A2).
pub mod fuji {
    pub const PROP_START_RAW_CONVERSION: u16 = 0xD183;
    pub const PROP_RAW_CONV_PROFILE: u16 = 0xD185;
}
