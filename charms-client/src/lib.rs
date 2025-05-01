use crate::tx::{extract_and_verify_spell, EnchantedTx, Tx};
use charms_data::{check, App, Charms, Data, Transaction, TxId, UtxoId, B32};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub mod bitcoin_tx;
pub mod cardano_tx;
pub mod tx;

/// Verification key for version `0` of the protocol implemented by `charms-spell-checker` binary.
pub const V0_SPELL_VK: &str = "0x00e9398ac819e6dd281f81db3ada3fe5159c3cc40222b5ddb0e7584ed2327c5d";
/// Verification key for version `1` of the protocol implemented by `charms-spell-checker` binary.
pub const V1_SPELL_VK: &str = "0x009f38f590ebca4c08c1e97b4064f39e4cd336eea4069669c5f5170a38a1ff97";
/// Verification key for version `2` of the protocol implemented by `charms-spell-checker` binary.
pub const V2_SPELL_VK: &str = "0x00bd312b6026dbe4a2c16da1e8118d4fea31587a4b572b63155252d2daf69280";

/// Version `0` of the protocol.
pub const V0: u32 = 0u32;
/// Version `1` of the protocol.
pub const V1: u32 = 1u32;
/// Version `2` of the protocol.
pub const V2: u32 = 2u32;
/// Version `3` of the protocol.
pub const V3: u32 = 3u32;

/// Current version of the protocol.
pub const CURRENT_VERSION: u32 = V3;

/// Maps the index of the charm's app (in [`NormalizedSpell`].`app_public_inputs`) to the charm's
/// data.
pub type NormalizedCharms = BTreeMap<u32, Data>;

/// Normalized representation of a Charms transaction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NormalizedTransaction {
    /// (Optional) input UTXO list. Is None when serialized in the transaction: the transaction
    /// already lists all inputs. **Must** be in the order of the transaction inputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ins: Option<Vec<UtxoId>>,

    /// Reference UTXO list. **May** be empty.
    pub refs: BTreeSet<UtxoId>,

    /// Output charms. **Must** be in the order of the transaction outputs.
    /// When proving spell correctness, we can't know the transaction ID yet.
    /// We only know the index of each output charm.
    /// **Must** be in the order of the hosting transaction's outputs.
    /// **Must not** be larger than the number of outputs in the hosting transaction.
    pub outs: Vec<NormalizedCharms>,

    /// Optional mapping from the beamed input index to the beaming source UtxoId.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beamed_ins: Option<BTreeMap<u32, UtxoId>>,

    /// Optional mapping from the beamed output index to the destination UtxoId.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beamed_outs: Option<BTreeMap<u32, B32>>,
}

impl NormalizedTransaction {
    /// Return a sorted set of transaction IDs of the inputs.
    /// Including source tx_ids for beamed inputs.
    pub fn prev_txids(&self) -> Option<BTreeSet<&TxId>> {
        let beam_source_txids: BTreeSet<_> = self
            .beamed_ins
            .as_ref()
            .map(|beamed_ins| beamed_ins.values().map(|utxo_id| &utxo_id.0).collect())
            .unwrap_or_default();

        let txids_opt = self
            .ins
            .as_ref()
            .map(|ins| ins.iter().map(|utxo_id| &utxo_id.0).collect());

        txids_opt.map(|set: BTreeSet<_>| set.union(&beam_source_txids).cloned().collect())
    }
}

/// Proof of spell correctness.
pub type Proof = Box<[u8]>;

/// Normalized representation of a spell.
/// Can be committed as public input.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NormalizedSpell {
    /// Protocol version.
    pub version: u32,
    /// Transaction data.
    pub tx: NormalizedTransaction,
    /// Maps all `App`s in the transaction to (potentially empty) public input data.
    pub app_public_inputs: BTreeMap<App, Data>,
}

pub fn utxo_id_hash(utxo_id: &UtxoId) -> B32 {
    let mut hasher = Sha256::new();
    hasher.update(utxo_id.to_bytes().as_slice());
    let result: [u8; 32] = hasher.finalize().into();
    B32(result)
}

/// Extract spells from previous transactions.
#[tracing::instrument(level = "debug", skip(prev_txs, spell_vk))]
pub fn prev_spells(
    prev_txs: &Vec<Tx>,
    spell_vk: &str,
) -> BTreeMap<TxId, (Option<NormalizedSpell>, usize)> {
    prev_txs
        .iter()
        .map(|tx| {
            let tx_id = tx.tx_id();
            (
                tx_id,
                (
                    extract_and_verify_spell(spell_vk, tx)
                        .map_err(|e| {
                            tracing::info!("no correct spell in tx {}: {}", tx_id, e);
                        })
                        .ok(),
                    tx.tx_outs_len(),
                ),
            )
        })
        .collect()
}

/// Check if the spell is well-formed.
#[tracing::instrument(level = "debug", skip(spell, prev_spells))]
pub fn well_formed(
    spell: &NormalizedSpell,
    prev_spells: &BTreeMap<TxId, (Option<NormalizedSpell>, usize)>,
    beamed_source_utxos_hint: &BTreeMap<u32, UtxoId>,
) -> bool {
    check!(spell.version == CURRENT_VERSION);
    let directly_created_by_prev_txns = |utxo_id: &UtxoId| -> bool {
        let tx_id = utxo_id.0;
        prev_spells
            .get(&tx_id)
            .is_some_and(|(n_spell_opt, num_tx_outs)| {
                let utxo_index = utxo_id.1;

                let is_beamed_out = n_spell_opt
                    .as_ref()
                    .and_then(|n_spell| n_spell.tx.beamed_outs.as_ref())
                    .and_then(|beamed_outs| beamed_outs.get(&utxo_index))
                    .is_some();

                utxo_index <= *num_tx_outs as u32 && !is_beamed_out
            })
    };
    check!({
        spell.tx.outs.iter().all(|n_charm| {
            n_charm
                .keys()
                .all(|&i| i < spell.app_public_inputs.len() as u32)
        })
    });
    // check that UTXOs we're spending or referencing in this tx
    // are created by pre-req transactions
    let Some(tx_ins) = &spell.tx.ins else {
        eprintln!("no tx.ins");
        return false;
    };
    check!(
        tx_ins.iter().all(directly_created_by_prev_txns)
            && spell.tx.refs.iter().all(directly_created_by_prev_txns)
    );
    let beamed_source_utxos_point_to_placeholder_dest_utxos =
        beamed_source_utxos_hint
            .iter()
            .all(|(&tx_in_index, beaming_source_utxo_id)| {
                let Some(tx_in_utxo_id) = tx_ins.get(tx_in_index as usize) else {
                    return false;
                };
                let prev_txid = tx_in_utxo_id.0;
                let prev_tx = prev_spells.get(&prev_txid);
                let Some((prev_spell_opt, _tx_outs)) = prev_tx else {
                    // prev_tx should be provided, so we know it doesn't carry a spell
                    return false;
                };
                // prev_tx must exist but not carry a spell
                check!(prev_spell_opt.is_none());

                let beaming_txid = beaming_source_utxo_id.0;
                let beaming_utxo_index = beaming_source_utxo_id.1;

                prev_spells
                    .get(&beaming_txid)
                    .and_then(|(n_spell_opt, _tx_outs)| {
                        n_spell_opt.as_ref().and_then(|n_spell| {
                            n_spell
                                .tx
                                .beamed_outs
                                .as_ref()
                                .and_then(|beamed_outs| beamed_outs.get(&beaming_utxo_index))
                        })
                    })
                    .is_some_and(|dest_utxo_hash| dest_utxo_hash == &utxo_id_hash(tx_in_utxo_id))
            });
    check!(beamed_source_utxos_point_to_placeholder_dest_utxos);
    true
}

/// Return the list of apps in the spell.
pub fn apps(spell: &NormalizedSpell) -> Vec<App> {
    spell.app_public_inputs.keys().cloned().collect()
}

/// Convert normalized spell to [`charms_data::Transaction`].
pub fn to_tx(
    spell: &NormalizedSpell,
    prev_spells: &BTreeMap<TxId, (Option<NormalizedSpell>, usize)>,
) -> Transaction {
    let from_utxo_id = |utxo_id: &UtxoId| -> (UtxoId, Charms) {
        let (prev_spell_opt, _) = &prev_spells[&utxo_id.0];
        let charms = prev_spell_opt
            .as_ref()
            .and_then(|prev_spell| {
                prev_spell
                    .tx
                    .outs
                    .get(utxo_id.1 as usize)
                    .map(|n_charms| charms(prev_spell, n_charms))
            })
            .unwrap_or_default();
        (utxo_id.clone(), charms)
    };

    let from_normalized_charms =
        |n_charms: &NormalizedCharms| -> Charms { charms(spell, n_charms) };

    let Some(tx_ins) = &spell.tx.ins else {
        unreachable!("self.tx.ins MUST be Some at this point");
    };
    Transaction {
        ins: tx_ins.iter().map(from_utxo_id).collect(),
        refs: spell.tx.refs.iter().map(from_utxo_id).collect(),
        outs: spell.tx.outs.iter().map(from_normalized_charms).collect(),
    }
}

/// Return [`charms_data::Charms`] for the given [`NormalizedCharms`].
pub fn charms(spell: &NormalizedSpell, n_charms: &NormalizedCharms) -> Charms {
    let apps = apps(spell);
    n_charms
        .iter()
        .map(|(&i, data)| (apps[i as usize].clone(), data.clone()))
        .collect()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpellProverInput {
    pub self_spell_vk: String,
    pub prev_txs: Vec<Tx>,
    pub spell: NormalizedSpell,
    pub beamed_source_utxos_hint: BTreeMap<u32, UtxoId>,
    /// indices of apps in the spell that have contract proofs
    pub app_contract_proofs: BTreeSet<usize>, // proofs are provided in input stream data
}

#[cfg(test)]
mod test {
    #[test]
    fn dummy() {}
}
