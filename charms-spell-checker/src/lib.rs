pub mod app;
pub mod bin;

use crate::app::AppContractVK;
use charms_client::{tx::Tx, NormalizedSpell};
use charms_data::{App, UtxoId};
use std::collections::{BTreeMap, BTreeSet};

/// Check if the spell is correct.
pub(crate) fn is_correct(
    spell: &NormalizedSpell,
    prev_txs: &Vec<Tx>,
    app_contract_vks: &Vec<(App, AppContractVK)>,
    spell_vk: &String,
    tx_ins_beamed_source_utxos: &BTreeMap<UtxoId, UtxoId>,
) -> bool {
    let prev_spells = charms_client::prev_spells(prev_txs, spell_vk);
    if !charms_client::well_formed(spell, &prev_spells, tx_ins_beamed_source_utxos) {
        eprintln!("not well formed");
        return false;
    }
    let Some(prev_txids) = spell.tx.prev_txids() else {
        unreachable!("the spell is well formed: tx.ins MUST be Some");
    };
    let beam_source_txids: BTreeSet<_> =
        tx_ins_beamed_source_utxos.values().map(|u| &u.0).collect();
    if prev_txids.union(&beam_source_txids) != prev_spells.keys().collect() {
        eprintln!("spell.tx.prev_txids() != prev_spells.keys()");
        return false;
    }

    let apps = charms_client::apps(spell);
    if apps.len() != app_contract_vks.len() {
        eprintln!("apps.len() != app_contract_proofs.len()");
        return false;
    }
    let charms_tx = charms_client::to_tx(spell, &prev_spells, tx_ins_beamed_source_utxos);
    if !apps
        .iter()
        .zip(app_contract_vks)
        .all(|(app0, (app, proof))| {
            app == app0 && proof.verify(app, &charms_tx, &spell.app_public_inputs[app])
        })
    {
        eprintln!("app_contract_proofs verification failed");
        return false;
    }

    true
}

#[cfg(test)]
mod test {
    #[test]
    fn dummy() {}
}
