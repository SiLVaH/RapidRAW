//! PTP container packing (ISO 15740 §9).
//!
//! Layout:
//!   [0..4]   u32 LE length
//!   [4..6]   u16 LE type
//!   [6..8]   u16 LE code
//!   [8..12]  u32 LE transaction id
//!   [12..]   params (command/response) and/or payload (data)

use super::codes::container_type;
use crate::fuji_raw_conv::error::{FujiRawConvError, FujiRawConvErrorKind};

pub const HEADER_SIZE: usize = 12;
pub const MAX_PARAMS: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtpContainer {
    pub type_: u16,
    pub code: u16,
    pub transaction_id: u32,
    pub params: Vec<u32>,
    pub data: Vec<u8>,
}

impl PtpContainer {
    pub fn command(code: u16, transaction_id: u32, params: &[u32]) -> Self {
        Self {
            type_: container_type::COMMAND,
            code,
            transaction_id,
            params: params.iter().copied().take(MAX_PARAMS).collect(),
            data: Vec::new(),
        }
    }

    pub fn data(code: u16, transaction_id: u32, payload: Vec<u8>) -> Self {
        Self {
            type_: container_type::DATA,
            code,
            transaction_id,
            params: Vec::new(),
            data: payload,
        }
    }

    pub fn pack(&self) -> Vec<u8> {
        let params = &self.params[..self.params.len().min(MAX_PARAMS)];
        let total = HEADER_SIZE + params.len() * 4 + self.data.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&self.type_.to_le_bytes());
        out.extend_from_slice(&self.code.to_le_bytes());
        out.extend_from_slice(&self.transaction_id.to_le_bytes());
        for p in params {
            out.extend_from_slice(&p.to_le_bytes());
        }
        out.extend_from_slice(&self.data);
        out
    }

    pub fn unpack(raw: &[u8]) -> Result<Self, FujiRawConvError> {
        if raw.len() < HEADER_SIZE {
            return Err(FujiRawConvError::new(
                FujiRawConvErrorKind::Protocol,
                "PTP container too short.",
            )
            .with_detail(format!("{} bytes", raw.len())));
        }

        let length = u32::from_le_bytes(raw[0..4].try_into().unwrap()) as usize;
        if length < HEADER_SIZE || length > raw.len() {
            return Err(FujiRawConvError::new(
                FujiRawConvErrorKind::Protocol,
                "PTP container length field is invalid.",
            )
            .with_detail(format!("length={length}, buffer={}", raw.len())));
        }

        let type_ = u16::from_le_bytes(raw[4..6].try_into().unwrap());
        let code = u16::from_le_bytes(raw[6..8].try_into().unwrap());
        let transaction_id = u32::from_le_bytes(raw[8..12].try_into().unwrap());
        let rest = &raw[HEADER_SIZE..length];

        let (params, data) = match type_ {
            container_type::DATA => (Vec::new(), rest.to_vec()),
            container_type::COMMAND | container_type::RESPONSE | container_type::EVENT => {
                let mut params = Vec::new();
                let mut offset = 0;
                while offset + 4 <= rest.len() && params.len() < MAX_PARAMS {
                    let p = u32::from_le_bytes(rest[offset..offset + 4].try_into().unwrap());
                    params.push(p);
                    offset += 4;
                }
                (params, Vec::new())
            }
            other => {
                return Err(FujiRawConvError::new(
                    FujiRawConvErrorKind::Protocol,
                    "Unknown PTP container type.",
                )
                .with_detail(format!("type=0x{other:04X}")));
            }
        };

        Ok(Self {
            type_,
            code,
            transaction_id,
            params,
            data,
        })
    }

    pub fn declared_length(raw: &[u8]) -> Option<usize> {
        if raw.len() < 4 {
            return None;
        }
        Some(u32::from_le_bytes(raw[0..4].try_into().ok()?) as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuji_raw_conv::ptp::codes::op;

    #[test]
    fn command_round_trip() {
        let c = PtpContainer::command(op::OPEN_SESSION, 7, &[1]);
        let packed = c.pack();
        assert_eq!(packed.len(), HEADER_SIZE + 4);
        let back = PtpContainer::unpack(&packed).unwrap();
        assert_eq!(back.type_, container_type::COMMAND);
        assert_eq!(back.code, op::OPEN_SESSION);
        assert_eq!(back.transaction_id, 7);
        assert_eq!(back.params, vec![1]);
        assert!(back.data.is_empty());
    }

    #[test]
    fn data_round_trip_preserves_payload() {
        let payload = b"hello-fuji".to_vec();
        let c = PtpContainer::data(op::GET_DEVICE_INFO, 3, payload.clone());
        let back = PtpContainer::unpack(&c.pack()).unwrap();
        assert_eq!(back.type_, container_type::DATA);
        assert_eq!(back.data, payload);
        assert!(back.params.is_empty());
    }

    #[test]
    fn rejects_truncated_buffer() {
        assert!(PtpContainer::unpack(&[1, 2, 3]).is_err());
    }
}
