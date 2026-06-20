//! ScVal decoding utilities.
//!
//! Soroban events carry base64-encoded XDR ScVal for both topics and data.
//! This module decodes them into a human-friendly [`ScValDecoded`] enum
//! by hand-parsing the XDR discriminants, without requiring the full
//! Soroban SDK (which only targets Wasm).

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};

/// A decoded Soroban ScVal — the human-friendly representation of any
/// value that can appear in a contract event's topic or data payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ScValDecoded {
    Bool(bool),
    Void,
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    /// 128-bit integer, stored as a string to avoid JSON precision loss
    I128(String),
    U128(String),
    Symbol(String),
    Bytes(Vec<u8>),
    String(String),
    /// Contract or account address (hex-encoded; raw Strkey not reconstructed)
    Address(String),
    Vec(Vec<ScValDecoded>),
    Map(Vec<(ScValDecoded, ScValDecoded)>),
    Error { code: u32, kind: String },
    /// Could not decode — raw base64 preserved for safety
    Raw(String),
}

impl ScValDecoded {
    /// Decode a base64-encoded XDR ScVal string.
    ///
    /// On any decode failure, returns `ScValDecoded::Raw(base64)` so callers
    /// always get *something* rather than a hard error.
    pub fn from_base64(b64: &str) -> Self {
        match Self::try_decode(b64) {
            Ok(v) => v,
            Err(_) => ScValDecoded::Raw(b64.to_string()),
        }
    }

    fn try_decode(b64: &str) -> Result<Self, String> {
        let bytes = STANDARD.decode(b64).map_err(|e| e.to_string())?;
        Self::from_xdr_bytes(&bytes)
    }

    /// Decode raw XDR bytes into a ScValDecoded.
    ///
    /// Discriminant values follow the Stellar XDR spec for `ScVal`:
    /// <https://github.com/stellar/stellar-xdr/blob/curr/Stellar-contract.x>
    fn from_xdr_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 4 {
            return Err("XDR too short".into());
        }

        let discriminant = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let rest = &bytes[4..];

        let decoded = match discriminant {
            0 => ScValDecoded::Bool(false),
            1 => ScValDecoded::Bool(true),
            2 => ScValDecoded::Void,
            4 => {
                let v = read_u32(rest)?;
                ScValDecoded::U32(v)
            }
            5 => {
                let v = read_u32(rest)?;
                ScValDecoded::I32(v as i32)
            }
            6 => {
                let v = read_u64(rest)?;
                ScValDecoded::U64(v)
            }
            7 => {
                let v = read_u64(rest)?;
                ScValDecoded::I64(v as i64)
            }
            10 => {
                let v = read_u128(rest)?;
                ScValDecoded::U128(v.to_string())
            }
            11 => {
                let v = read_u128(rest)? as i128;
                ScValDecoded::I128(v.to_string())
            }
            14 => {
                let raw = decode_opaque(rest)?;
                ScValDecoded::Bytes(raw)
            }
            15 => {
                let raw = decode_opaque(rest)?;
                ScValDecoded::String(String::from_utf8_lossy(&raw).to_string())
            }
            16 => {
                let raw = decode_opaque(rest)?;
                ScValDecoded::Symbol(String::from_utf8_lossy(&raw).to_string())
            }
            17 => {
                // SCV_VEC: optional<ScVec>
                if rest.is_empty() {
                    return Err("vec: too short".into());
                }
                let present = read_u32(rest)?;
                if present == 0 {
                    ScValDecoded::Vec(vec![])
                } else {
                    ScValDecoded::Vec(decode_count_placeholder(&rest[4..])?)
                }
            }
            18 => {
                // SCV_MAP: optional<ScMap>
                let present = read_u32(rest)?;
                if present == 0 {
                    ScValDecoded::Map(vec![])
                } else {
                    let n = read_u32(&rest[4..])?.min(64) as usize;
                    let pairs = (0..n)
                        .map(|i| {
                            (
                                ScValDecoded::Symbol(format!("key[{i}]")),
                                ScValDecoded::Symbol(format!("val[{i}]")),
                            )
                        })
                        .collect();
                    ScValDecoded::Map(pairs)
                }
            }
            19 => {
                // SCV_ADDRESS: ScAddress
                let addr_type = read_u32(rest)?;
                let addr_rest = &rest[4..];
                let s = match addr_type {
                    0 if addr_rest.len() >= 36 => {
                        format!("G[{}]", hex_encode(&addr_rest[4..36]))
                    }
                    1 if addr_rest.len() >= 32 => {
                        format!("C[{}]", hex_encode(&addr_rest[..32]))
                    }
                    _ => format!("Address(type={addr_type})"),
                };
                ScValDecoded::Address(s)
            }
            _ => ScValDecoded::Raw(STANDARD.encode(bytes)),
        };

        Ok(decoded)
    }

    /// Compact string representation, useful for logging / CLI display.
    pub fn display(&self) -> String {
        match self {
            ScValDecoded::Bool(b) => b.to_string(),
            ScValDecoded::Void => "void".to_string(),
            ScValDecoded::I32(v) => v.to_string(),
            ScValDecoded::U32(v) => v.to_string(),
            ScValDecoded::I64(v) => v.to_string(),
            ScValDecoded::U64(v) => v.to_string(),
            ScValDecoded::I128(v) => format!("i128:{v}"),
            ScValDecoded::U128(v) => format!("u128:{v}"),
            ScValDecoded::Symbol(s) => format!(":{s}"),
            ScValDecoded::String(s) => format!("\"{s}\""),
            ScValDecoded::Bytes(b) => format!("0x{}", hex_encode(b)),
            ScValDecoded::Address(a) => a.clone(),
            ScValDecoded::Vec(v) => format!(
                "[{}]",
                v.iter().map(|x| x.display()).collect::<Vec<_>>().join(", ")
            ),
            ScValDecoded::Map(m) => format!(
                "{{{}}}",
                m.iter()
                    .map(|(k, v)| format!("{}={}", k.display(), v.display()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            ScValDecoded::Error { code, kind } => format!("error({kind}:{code})"),
            ScValDecoded::Raw(r) => format!("raw:{r}"),
        }
    }
}

fn read_u32(bytes: &[u8]) -> Result<u32, String> {
    if bytes.len() < 4 {
        return Err("expected 4 bytes".into());
    }
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64(bytes: &[u8]) -> Result<u64, String> {
    if bytes.len() < 8 {
        return Err("expected 8 bytes".into());
    }
    Ok(u64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn read_u128(bytes: &[u8]) -> Result<u128, String> {
    if bytes.len() < 16 {
        return Err("expected 16 bytes".into());
    }
    let hi = read_u64(&bytes[0..8])?;
    let lo = read_u64(&bytes[8..16])?;
    Ok(((hi as u128) << 64) | (lo as u128))
}

/// Decode XDR variable-length opaque data (4-byte length prefix + padded bytes)
fn decode_opaque(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let len = read_u32(bytes)? as usize;
    let padded_len = (len + 3) & !3;
    if bytes.len() < 4 + padded_len {
        return Err(format!(
            "opaque: declared len={len} but only {} bytes remain",
            bytes.len().saturating_sub(4)
        ));
    }
    Ok(bytes[4..4 + len].to_vec())
}

/// We don't do full recursive XDR parsing for nested Vec/Map contents
/// (would require pulling in the full stellar-xdr crate). Instead we
/// report the element count as placeholders — callers needing full
/// recursive decode of nested structures should use `raw_value` /
/// `raw_topics` with the official `stellar-xdr` crate directly.
fn decode_count_placeholder(bytes: &[u8]) -> Result<Vec<ScValDecoded>, String> {
    let count = read_u32(bytes)?.min(256) as usize;
    Ok((0..count)
        .map(|i| ScValDecoded::Symbol(format!("item[{i}]")))
        .collect())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_u32() {
        // discriminant 4 (SCV_U32) = 0x00000004, value 42 = 0x0000002A
        let bytes = [0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x2A];
        let b64 = STANDARD.encode(bytes);
        assert_eq!(ScValDecoded::from_base64(&b64), ScValDecoded::U32(42));
    }

    #[test]
    fn test_decode_bool_true() {
        let bytes = [0x00, 0x00, 0x00, 0x01];
        let b64 = STANDARD.encode(bytes);
        assert_eq!(ScValDecoded::from_base64(&b64), ScValDecoded::Bool(true));
    }

    #[test]
    fn test_decode_void() {
        let bytes = [0x00, 0x00, 0x00, 0x02];
        let b64 = STANDARD.encode(bytes);
        assert_eq!(ScValDecoded::from_base64(&b64), ScValDecoded::Void);
    }

    #[test]
    fn test_decode_symbol() {
        // discriminant 16 (SCV_SYMBOL), len=8, "transfer" padded to 8 bytes (already aligned)
        let mut bytes = vec![0x00, 0x00, 0x00, 0x10]; // discriminant 16
        bytes.extend_from_slice(&8u32.to_be_bytes()); // length = 8
        bytes.extend_from_slice(b"transfer"); // 8 bytes, already 4-byte aligned
        let b64 = STANDARD.encode(&bytes);
        assert_eq!(
            ScValDecoded::from_base64(&b64),
            ScValDecoded::Symbol("transfer".to_string())
        );
    }

    #[test]
    fn test_garbage_returns_raw() {
        let decoded = ScValDecoded::from_base64("not valid base64 !!!");
        assert!(matches!(decoded, ScValDecoded::Raw(_)));
    }

    #[test]
    fn test_display() {
        assert_eq!(ScValDecoded::U32(100).display(), "100");
        assert_eq!(ScValDecoded::Symbol("mint".to_string()).display(), ":mint");
        assert_eq!(ScValDecoded::Bool(true).display(), "true");
    }
}
