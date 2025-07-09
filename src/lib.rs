pub mod app;
pub mod cli;
pub mod script;
pub mod spell;
pub mod tx;
pub mod utils;

/// RISC-V binary compiled from `charms-app-checker`.
pub const APP_CHECKER_BINARY: &[u8] = include_bytes!("./bin/charms-app-checker");
/// RISC-V binary compiled from `charms-spell-checker`.
pub const SPELL_CHECKER_BINARY: &[u8] = include_bytes!("./bin/charms-spell-checker");

/// Verification key for the `charms-spell-checker` binary.
pub const SPELL_VK: &str = "0x00de43c6a19774cbecd5fb6eb0eaad7f1d5852741bfdf91ac11217ee6fe52850";

#[cfg(test)]
mod test {
    use super::*;
    use crate::SPELL_VK;
    use charms_client::finality::{BTC_FINALITY_BINARY, BTC_FINALITY_VK};
    use sp1_sdk::{HashableKey, Prover, ProverClient};

    #[test]
    fn test_spell_vk() {
        let client = ProverClient::builder().mock().build();

        let (_, vk) = client.setup(APP_CHECKER_BINARY);
        assert_eq!(charms_client::APP_VK, vk.hash_u32());

        let (_, vk) = client.setup(SPELL_CHECKER_BINARY);
        let s = vk.bytes32();
        assert_eq!(SPELL_VK, s.as_str());
    }

    #[test]
    fn test_btc_finality_vk() {
        let client = ProverClient::builder().mock().build();

        let (_, vk) = client.setup(BTC_FINALITY_BINARY);
        let s = vk.bytes32();
        assert_eq!(BTC_FINALITY_VK, s.as_str().strip_prefix("0x").unwrap());
    }
}
