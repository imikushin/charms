pub mod app;
pub mod bin;

use crate::app::AppContractVK;
use charms_client::{tx::Tx, NormalizedSpell};
use charms_data::{check, App, UtxoId};
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

    check!(charms_client::well_formed(
        spell,
        &prev_spells,
        tx_ins_beamed_source_utxos
    ));

    let Some(prev_txids) = spell.tx.prev_txids() else {
        unreachable!("the spell is well formed: tx.ins MUST be Some");
    };
    let all_prev_txids: BTreeSet<_> = tx_ins_beamed_source_utxos
        .values()
        .map(|u| &u.0)
        .chain(prev_txids)
        .collect();
    check!(all_prev_txids == prev_spells.keys().collect());

    let apps = charms_client::apps(spell);
    check!(apps.len() == app_contract_vks.len());

    let charms_tx = charms_client::to_tx(spell, &prev_spells, tx_ins_beamed_source_utxos);
    check!(apps
        .iter()
        .zip(app_contract_vks)
        .all(|(app0, (app, proof))| {
            app == app0 && proof.verify(app, &charms_tx, &spell.app_public_inputs[app])
        }));

    true
}

#[cfg(test)]
mod test {
    #[test]
    fn dummy() {}
}
