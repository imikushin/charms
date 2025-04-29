use crate::{tx::EnchantedTx, NormalizedSpell, Proof};
use anyhow::{anyhow, ensure};
use cardano_serialization_lib::{chain_crypto::Blake2b256, Transaction, TransactionInputs};
use charms_data::{util, TxId, UtxoId};
use serde::{Deserialize, Serialize};
use sp1_verifier::Groth16Verifier;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CardanoTx(pub Transaction);

impl CardanoTx {
    pub fn from_hex(hex: &str) -> anyhow::Result<Self> {
        Ok(Self(Transaction::from_hex(hex)?))
    }
}

impl EnchantedTx for CardanoTx {
    fn extract_and_verify_spell(&self, spell_vk: &str) -> anyhow::Result<NormalizedSpell> {
        let tx = &self.0;

        let inputs = tx.body().inputs();
        ensure!(inputs.len() > 0, "Transaction has no inputs");

        let outputs = tx.body().outputs();
        ensure!(outputs.len() > 0, "Transaction has no outputs");

        let spell_output = outputs.get(outputs.len() - 1);

        let Some(spell_data) = spell_output.plutus_data() else {
            return Err(anyhow::anyhow!("Transaction has no spell output"));
        };

        let Some(spell_data) = spell_data.as_bytes() else {
            return Err(anyhow::anyhow!("Spell output has no data"));
        };

        let (spell, proof): (NormalizedSpell, Proof) = util::read(spell_data.as_slice())
            .map_err(|e| anyhow!("could not parse spell and proof: {}", e))?;

        ensure!(
            &spell.tx.ins.is_none(),
            "spell must inherit inputs from the enchanted tx"
        );
        ensure!(
            spell.tx.outs.len() < outputs.len(),
            "spell tx outs mismatch"
        );

        let spell = spell_with_ins(spell, inputs);

        let (spell_vk, groth16_vk) = crate::tx::vks(spell.version, spell_vk)?;

        Groth16Verifier::verify(
            &proof,
            crate::tx::to_sp1_pv(spell.version, &(spell_vk, &spell)).as_slice(),
            spell_vk,
            groth16_vk,
        )
        .map_err(|e| anyhow!("could not verify spell proof: {}", e))?;

        Ok(spell)
    }

    fn tx_outs_len(&self) -> usize {
        self.0.body().outputs().len()
    }

    fn tx_id(&self) -> TxId {
        let transaction_hash = Blake2b256::new(&self.0.body().to_bytes());
        let mut txid: [u8; 32] = transaction_hash.into();
        txid.reverse(); // Cardano seems to store IDs reverted compared to Bitcoin txid format
        TxId(txid)
    }

    fn hex(&self) -> String {
        self.0.to_hex()
    }
}

fn spell_with_ins(spell: NormalizedSpell, tx_ins: TransactionInputs) -> NormalizedSpell {
    let n = tx_ins.len() - 1;
    let tx_ins: Vec<UtxoId> = tx_ins
        .into_iter()
        .take(n)
        .map(|tx_in| {
            let mut txid: [u8; 32] = tx_in.transaction_id().to_bytes().try_into().unwrap();
            txid.reverse();
            let index = tx_in.index();

            UtxoId(TxId(txid), index)
        })
        .collect();

    let mut spell = spell;
    spell.tx.ins = Some(tx_ins);

    spell
}
