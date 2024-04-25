use cainome_cairo_serde::{CairoSerde, Error};
use starknet::core::types::FieldElement;

#[derive(Debug, Clone)]
/// Represents an unsigned integer of 256 bits
pub(crate) struct U256 {
    /// Lower 128 bits
    pub(crate) low: u128,
    /// Upper 128 bits
    pub(crate) high: u128,
}

impl CairoSerde for U256 {
    type RustType = Self;

    fn cairo_serialize(rust: &Self::RustType) -> Vec<FieldElement> {
        let mut felts = u128::cairo_serialize(&rust.low);
        felts.extend(u128::cairo_serialize(&rust.high));
        felts
    }

    fn cairo_deserialize(
        felts: &[FieldElement],
        offset: usize,
    ) -> cainome_cairo_serde::Result<Self::RustType> {
        if offset >= felts.len() {
            return Err(Error::Deserialize(format!(
                "Buffer too short to deserialize a unsigned integer: offset ({}) : buffer {:?}",
                offset, felts,
            )));
        }

        let low: u128 = felts[offset].try_into().unwrap();
        let high: u128 = felts[offset + 1].try_into().unwrap();
        Ok(U256 { low, high })
    }
}
