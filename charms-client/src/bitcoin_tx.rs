use crate::{tx, tx::EnchantedTx, NormalizedSpell, Proof};
use anyhow::{anyhow, bail, ensure};
use bitcoin::{
    consensus::encode::{deserialize_hex, serialize_hex},
    hashes::Hash,
    opcodes::all::{OP_ENDIF, OP_IF},
    script::{Instruction, PushBytes},
    TxIn,
};
use charms_data::{util, TxId, UtxoId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BitcoinTx(pub bitcoin::Transaction);

impl BitcoinTx {
    pub fn from_hex(hex: &str) -> anyhow::Result<Self> {
        let tx = deserialize_hex(hex)?;
        Ok(Self(tx))
    }
}

impl EnchantedTx for BitcoinTx {
    fn extract_and_verify_spell(&self, spell_vk: &str) -> anyhow::Result<NormalizedSpell> {
        let tx = &self.0;

        let Some((spell_tx_in, tx_ins)) = tx.input.split_last() else {
            bail!("transaction does not have inputs")
        };

        let (spell, proof) = parse_spell_and_proof(spell_tx_in)?;

        ensure!(
            &spell.tx.ins.is_none(),
            "spell must inherit inputs from the enchanted tx"
        );
        ensure!(
            &spell.tx.outs.len() <= &tx.output.len(),
            "spell tx outs mismatch"
        );

        let spell = spell_with_ins(spell, tx_ins);

        let spell_vk = tx::spell_vk(spell.version, spell_vk)?;

        let public_values = tx::to_serialized_pv(spell.version, &(spell_vk, &spell));

        tx::verify_snark_proof(&proof, &public_values, spell_vk, spell.version)?;

        Ok(spell)
    }

    fn tx_outs_len(&self) -> usize {
        self.0.output.len()
    }

    fn tx_id(&self) -> TxId {
        TxId(self.0.compute_txid().to_byte_array())
    }

    fn hex(&self) -> String {
        serialize_hex(&self.0)
    }
}

#[tracing::instrument(level = "debug", skip_all)]
pub(crate) fn spell_with_ins(spell: NormalizedSpell, spell_tx_ins: &[TxIn]) -> NormalizedSpell {
    let tx_ins = spell_tx_ins // exclude spell commitment input
        .iter()
        .map(|tx_in| {
            let out_point = tx_in.previous_output;
            UtxoId(TxId(out_point.txid.to_byte_array()), out_point.vout)
        })
        .collect();

    let mut spell = spell;
    spell.tx.ins = Some(tx_ins);

    spell
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn parse_spell_and_proof(spell_tx_in: &TxIn) -> anyhow::Result<(NormalizedSpell, Proof)> {
    ensure!(
        spell_tx_in
            .witness
            .taproot_control_block()
            .ok_or(anyhow!("no control block"))?
            .len()
            == 33,
        "the Taproot tree contains more than one leaf: only a single script is supported"
    );

    let leaf_script = spell_tx_in
        .witness
        .taproot_leaf_script()
        .ok_or(anyhow!("no spell data in the last input's witness"))?;

    let mut instructions = leaf_script.script.instructions();

    ensure!(instructions.next() == Some(Ok(Instruction::PushBytes(PushBytes::empty()))));
    ensure!(instructions.next() == Some(Ok(Instruction::Op(OP_IF))));
    let Some(Ok(Instruction::PushBytes(push_bytes))) = instructions.next() else {
        bail!("no spell data")
    };
    if push_bytes.as_bytes() != b"spell" {
        bail!("no spell marker")
    }

    let mut spell_data = vec![];

    loop {
        match instructions.next() {
            Some(Ok(Instruction::PushBytes(push_bytes))) => {
                spell_data.extend(push_bytes.as_bytes());
            }
            Some(Ok(Instruction::Op(OP_ENDIF))) => {
                break;
            }
            _ => {
                bail!("unexpected opcode")
            }
        }
    }

    let (spell, proof): (NormalizedSpell, Proof) = util::read(spell_data.as_slice())
        .map_err(|e| anyhow!("could not parse spell and proof: {}", e))?;
    Ok((spell, proof))
}
