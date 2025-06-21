use crate::{tx::EnchantedTx, NormalizedSpell, Proof};
use anyhow::{anyhow, ensure};
use charms_data::{util, TxId, UtxoId};
use cml_chain::{
    crypto::TransactionHash,
    plutus::PlutusData,
    transaction::{ConwayFormatTxOut, DatumOption, Transaction, TransactionOutput},
    Deserialize, Serialize, SetTransactionInput,
};
use sp1_verifier::Groth16Verifier;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CardanoTx(pub Transaction);

impl PartialEq for CardanoTx {
    fn eq(&self, other: &Self) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        self.0.to_canonical_cbor_bytes() == other.0.to_canonical_cbor_bytes()
    }
}

impl CardanoTx {
    pub fn from_hex(hex: &str) -> anyhow::Result<Self> {
        Ok(Self(
            Transaction::from_cbor_bytes(&hex::decode(hex.as_bytes())?)
                .map_err(|e| anyhow!("{}", e))?,
        ))
    }
}

impl EnchantedTx for CardanoTx {
    fn extract_and_verify_spell(&self, spell_vk: &str) -> anyhow::Result<NormalizedSpell> {
        let tx = &self.0;

        let inputs = &tx.body.inputs;
        ensure!(inputs.len() > 0, "Transaction has no inputs");

        let outputs = &tx.body.outputs;
        ensure!(outputs.len() > 0, "Transaction has no outputs");

        let spell_output = outputs.get(outputs.len() - 1);

        let Some(TransactionOutput::ConwayFormatTxOut(ConwayFormatTxOut {
            datum_option:
                Some(DatumOption::Datum {
                    datum:
                        PlutusData::Bytes {
                            bytes: spell_data, ..
                        },
                    ..
                }),
            ..
        })) = spell_output
        else {
            return Err(anyhow::anyhow!("Transaction has no spell output"));
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
        self.0.body.outputs.len()
    }

    fn tx_id(&self) -> TxId {
        let transaction_hash = self.0.body.hash();
        tx_id(transaction_hash)
    }

    fn hex(&self) -> String {
        hex::encode(self.0.to_canonical_cbor_bytes())
    }
}

fn spell_with_ins(spell: NormalizedSpell, tx_ins: &SetTransactionInput) -> NormalizedSpell {
    let n = tx_ins.len() - 1;
    let tx_ins: Vec<UtxoId> = tx_ins
        .iter()
        .take(n)
        .map(|tx_in| {
            let tx_id = tx_id(tx_in.transaction_id);
            let index = tx_in.index as u32;

            UtxoId(tx_id, index)
        })
        .collect();

    let mut spell = spell;
    spell.tx.ins = Some(tx_ins);

    spell
}

pub fn tx_id(transaction_hash: TransactionHash) -> TxId {
    let mut txid: [u8; 32] = transaction_hash.into();
    txid.reverse(); // Charms use Bitcoin's reverse byte order for txids
    let tx_id = TxId(txid);
    tx_id
}

pub fn tx_hash(tx_id: TxId) -> TransactionHash {
    let mut txid_bytes = tx_id.0;
    txid_bytes.reverse(); // Charms use Bitcoin's reverse byte order for txids
    let tx_hash = txid_bytes.into();
    tx_hash
}
