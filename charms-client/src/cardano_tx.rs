use crate::{tx::EnchantedTx, NormalizedSpell};
use cardano_serialization_lib::Transaction;

pub struct CardanoTx(pub Transaction);

impl EnchantedTx for CardanoTx {
    fn extract_and_verify_spell(&self, spell_vk: &str) -> anyhow::Result<NormalizedSpell> {
        todo!()
    }
}
