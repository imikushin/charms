use std::{fs::File, io::BufReader};

use crate::BitcoinFinalityInput;

/// RISC-V binary compiled from `charms-spell-checker`.
pub const BTC_FINALITY_BINARY: &[u8] = include_bytes!("../bin/btc-finality");

/// Verification key for the `btc-finality` binary.
pub const BTC_FINALITY_VK: &str =
    "00c94046701e60fb4812584e39f4a00a667313bca8fe91fe06ef0c9c0dd67dfe";
pub const U32_BTC_FINALITY_VK: [u32; 8] = [
    0x00c94046, 0x701e60fb, 0x4812584e, 0x39f4a00a, 0x667313bc, 0xa8fe91fe, 0x06ef0c9c, 0x0dd67dfe,
];

pub fn load_finality_input(path: &str) -> Result<BitcoinFinalityInput, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let input: BitcoinFinalityInput = serde_json::from_reader(reader)?;
    Ok(input)
}

#[cfg(test)]
mod test {
    use sp1_sdk::{HashableKey, Prover, ProverClient};

    use crate::finality::{BTC_FINALITY_BINARY, BTC_FINALITY_VK};

    #[test]
    fn test_btc_finality_vk() {
        let client = ProverClient::builder().mock().build();

        let (_, vk) = client.setup(BTC_FINALITY_BINARY);
        let s = vk.bytes32();
        assert_eq!(BTC_FINALITY_VK, s.as_str().strip_prefix("0x").unwrap());
    }
}
