use crate::{
    app, cli,
    cli::{wallet, wallet::MIN_SATS, SpellCastParams, SpellCheckParams, SpellProveParams},
    spell,
    spell::{ProveRequest, ProveSpellTx, Spell},
    tx, SPELL_VK,
};
use anyhow::{ensure, Error, Result};
use bitcoin::{
    consensus::encode::{deserialize_hex, serialize_hex},
    Transaction,
};
use sp1_prover::components::CpuProverComponents;
use sp1_sdk::Prover;
use std::{future::Future, sync::Arc};

pub trait Check {
    fn check(&self, params: SpellCheckParams) -> Result<()>;
}

pub trait Prove {
    fn prove(&self, params: SpellProveParams) -> impl Future<Output = Result<()>>;
}

pub trait Cast {
    fn cast(&self, params: SpellCastParams) -> impl Future<Output = Result<()>>;
}

pub struct SpellCli {
    pub app_prover: Arc<app::Prover>,
    pub sp1_client: Arc<Box<dyn Prover<CpuProverComponents>>>,
    pub spell_prover: Arc<spell::Prover>,
}

impl Prove for SpellCli {
    async fn prove(
        &self,
        SpellProveParams {
            spell,
            prev_txs,
            app_bins,
            funding_utxo,
            funding_utxo_value,
            change_address,
            fee_rate,
        }: SpellProveParams,
    ) -> Result<()> {
        // Parse funding UTXO early: to fail fast
        let funding_utxo = cli::tx::parse_outpoint(&funding_utxo)?;

        ensure!(fee_rate >= 1.0, "fee rate must be >= 1.0");

        let spell: Spell = serde_yaml::from_slice(&std::fs::read(spell)?)?;

        let prev_txs = prev_txs
            .into_iter()
            .map(|tx| Ok(deserialize_hex::<Transaction>(&tx)?))
            .collect::<Result<_>>()?;

        let binaries = cli::app::binaries_by_vk(&self.app_prover, app_bins)?;

        let transactions = self
            .spell_prover
            .prove_spell_tx(ProveRequest {
                spell,
                binaries,
                prev_txs,
                funding_utxo,
                funding_utxo_value,
                change_address,
                fee_rate,
                charms_fee: None,
            })
            .await?;

        // Convert transactions to hex and create JSON array
        let hex_txs: Vec<String> = transactions.iter().map(|tx| serialize_hex(tx)).collect();

        // Print JSON array of transaction hexes
        println!("{}", serde_json::to_string(&hex_txs)?);

        Ok(())
    }
}

impl Check for SpellCli {
    #[tracing::instrument(level = "debug", skip(self, spell, app_bins))]
    fn check(&self, SpellCheckParams { spell, app_bins }: SpellCheckParams) -> Result<()> {
        let mut spell: Spell = serde_yaml::from_slice(&std::fs::read(spell)?)?;
        for u in spell.outs.iter_mut() {
            u.sats.get_or_insert(crate::cli::wallet::MIN_SATS);
        }

        // make sure spell inputs all have utxo_id
        ensure!(
            spell.ins.iter().all(|u| u.utxo_id.is_some()),
            "all spell inputs must have utxo_id"
        );

        let tx = tx::from_spell(&spell);

        let prev_txs = cli::tx::get_prev_txs(&tx)?;

        let prev_spells = charms_client::prev_spells(&prev_txs, &SPELL_VK);

        let (norm_spell, app_private_inputs) = spell.normalized()?;

        ensure!(
            charms_client::well_formed(&norm_spell, &prev_spells),
            "spell is not well-formed"
        );

        let binaries = cli::app::binaries_by_vk(&self.app_prover, app_bins)?;

        let charms_tx = spell.to_tx()?;
        self.app_prover.run_all(
            &binaries,
            &charms_tx,
            &norm_spell.app_public_inputs,
            &app_private_inputs,
            None,
        )?;

        Ok(())
    }
}

impl Cast for SpellCli {
    async fn cast(
        &self,
        SpellCastParams {
            spell,
            app_bins,
            funding_utxo,
            fee_rate,
        }: SpellCastParams,
    ) -> Result<()> {
        // Parse funding UTXO early: to fail fast
        let funding_utxo = cli::tx::parse_outpoint(&funding_utxo)?;

        ensure!(fee_rate >= 1.0, "fee rate must be >= 1.0");
        let mut spell: Spell = serde_yaml::from_slice(&std::fs::read(spell)?)?;

        spell_pre_checks(&spell)?;

        for u in spell.outs.iter_mut() {
            u.sats.get_or_insert(MIN_SATS);
        }

        let prev_txs = gather_prev_txs(&spell)?;

        let funding_utxo_value = wallet::funding_utxo_value(&funding_utxo)?;
        let change_address = wallet::new_change_address()?;

        let binaries = cli::app::binaries_by_vk(&self.app_prover, app_bins)?;

        let [commit_tx, spell_tx] = self
            .spell_prover
            .prove_spell_tx(ProveRequest {
                spell,
                binaries,
                prev_txs,
                funding_utxo,
                funding_utxo_value,
                change_address,
                fee_rate,
                charms_fee: None,
            })
            .await?;

        let signed_commit_tx_hex = wallet::sign_tx(&serialize_hex(&commit_tx))?;
        let signed_spell_tx_hex = wallet::sign_spell_tx(&serialize_hex(&spell_tx), &commit_tx)?;

        // Print JSON array of transaction hexes
        println!(
            "{}",
            serde_json::to_string(&[signed_commit_tx_hex, signed_spell_tx_hex])?
        );

        Ok(())
    }
}

#[tracing::instrument(level = "debug", skip(spell))]
fn gather_prev_txs(spell: &Spell) -> Result<Vec<Transaction>, Error> {
    let tx = tx::from_spell(&spell);
    let prev_txs = cli::tx::get_prev_txs(&tx)?;
    Ok(prev_txs)
}

#[tracing::instrument(level = "debug", skip(spell))]
fn spell_pre_checks(spell: &Spell) -> Result<(), Error> {
    // make sure spell inputs all have utxo_id
    ensure!(
        spell.ins.iter().all(|u| u.utxo_id.is_some()),
        "all spell inputs must have utxo_id"
    );

    // make sure spell outputs all have addresses
    ensure!(
        spell.outs.iter().all(|u| u.address.is_some()),
        "all spell outputs must have addresses"
    );
    Ok(())
}
