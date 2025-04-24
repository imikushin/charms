use crate::{
    bitcoin_tx::BitcoinTx, cardano_tx::CardanoTx, NormalizedSpell, CURRENT_VERSION, V0,
    V0_SPELL_VK, V1, V1_SPELL_VK,
};
use anyhow::bail;
use charms_data::{util, TxId};
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use sp1_primitives::io::SP1PublicValues;

#[enum_dispatch]
pub trait EnchantedTx {
    fn extract_and_verify_spell(&self, spell_vk: &str) -> anyhow::Result<NormalizedSpell>;
    fn tx_outs_len(&self) -> usize;
    fn tx_id(&self) -> TxId;
}

#[enum_dispatch(EnchantedTx)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Tx {
    Bitcoin(BitcoinTx),
    Cardano(CardanoTx),
}

/// Extract a [`NormalizedSpell`] from a transaction and verify it.
/// Incorrect spells are rejected.
#[tracing::instrument(level = "debug", skip_all)]
pub fn extract_and_verify_spell(
    spell_vk: &str,
    enchanted_tx: &impl EnchantedTx,
) -> anyhow::Result<NormalizedSpell> {
    enchanted_tx.extract_and_verify_spell(spell_vk)
}

pub fn vks(spell_version: u32, spell_vk: &str) -> anyhow::Result<(&str, &[u8])> {
    match spell_version {
        CURRENT_VERSION => Ok((spell_vk, *sp1_verifier::GROTH16_VK_BYTES)),
        V1 => Ok((V1_SPELL_VK, *sp1_verifier::GROTH16_VK_BYTES)),
        V0 => Ok((V0_SPELL_VK, V0_GROTH16_VK_BYTES)),
        _ => bail!("unsupported spell version: {}", spell_version),
    }
}

pub const V0_GROTH16_VK_BYTES: &'static [u8] = include_bytes!("../vk/v0/groth16_vk.bin");

pub fn to_sp1_pv<T: Serialize>(spell_version: u32, t: &T) -> SP1PublicValues {
    let mut pv = SP1PublicValues::new();
    match spell_version {
        CURRENT_VERSION => {
            // we commit to CBOR-encoded tuple `(spell_vk, n_spell)`
            pv.write_slice(util::write(t).unwrap().as_slice());
        }
        V1 => {
            // we commit to CBOR-encoded tuple `(spell_vk, n_spell)`
            pv.write_slice(util::write(t).unwrap().as_slice());
        }
        V0 => {
            // we used to commit to the tuple `(spell_vk, n_spell)`, which was serialized internally
            // by SP1
            pv.write(t);
        }
        _ => unreachable!(),
    }
    pv
}
