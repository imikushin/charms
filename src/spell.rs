#[cfg(feature = "prover")]
use crate::tx::bitcoin_tx::add_spell;
use crate::{
    app,
    tx::{bitcoin_tx, txs_by_txid},
    utils,
    utils::{BoxedSP1Prover, Shared},
    SPELL_CHECKER_BINARY,
};
use anyhow::{anyhow, ensure, Error};
#[cfg(not(feature = "prover"))]
use bitcoin::consensus::encode::deserialize_hex;
#[cfg(feature = "prover")]
use bitcoin::FeeRate;
use bitcoin::{address::NetworkUnchecked, hashes::Hash, Address, Amount, OutPoint};
#[cfg(not(feature = "prover"))]
use charms_client::bitcoin_tx::BitcoinTx;
pub use charms_client::{
    to_tx, NormalizedCharms, NormalizedSpell, NormalizedTransaction, Proof, SpellProverInput,
    CURRENT_VERSION,
};
use charms_client::{
    tx::{EnchantedTx, Tx},
    SPELL_VK,
};
use charms_data::{util, App, Charms, Data, Transaction, TxId, UtxoId, B32};
#[cfg(not(feature = "prover"))]
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as, IfIsHumanReadable};
use sp1_sdk::{SP1ProofMode, SP1Stdin};
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    sync::Arc,
};

/// Charm as represented in a spell.
/// Map of `$KEY: data`.
pub type KeyedCharms = BTreeMap<String, Data>;

/// UTXO as represented in a spell.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Input {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utxo_id: Option<UtxoId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub charms: Option<KeyedCharms>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Output {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(alias = "sats", skip_serializing_if = "Option::is_none")]
    pub amount: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub charms: Option<KeyedCharms>,
}

/// Defines how spells are represented in their source form and in CLI outputs,
/// in both human-friendly (JSON/YAML) and machine-friendly (CBOR) formats.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Spell {
    /// Version of the protocol.
    pub version: u32,

    /// Apps used in the spell. Map of `$KEY: App`.
    /// Keys are arbitrary strings. They just need to be unique (inside the spell).
    pub apps: BTreeMap<String, App>,

    /// Public inputs to the apps for this spell. Map of `$KEY: Data`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_inputs: Option<BTreeMap<String, Data>>,

    /// Private inputs to the apps for this spell. Map of `$KEY: Data`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_inputs: Option<BTreeMap<String, Data>>,

    /// Transaction inputs.
    pub ins: Vec<Input>,
    /// Reference inputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refs: Option<Vec<Input>>,
    /// Transaction outputs.
    pub outs: Vec<Output>,
}

impl Spell {
    /// New empty spell.
    pub fn new() -> Self {
        Self {
            version: CURRENT_VERSION,
            apps: BTreeMap::new(),
            public_inputs: None,
            private_inputs: None,
            ins: vec![],
            refs: None,
            outs: vec![],
        }
    }

    /// Get a [`Transaction`] for the spell.
    pub fn to_tx(&self) -> anyhow::Result<Transaction> {
        let ins = self.strings_of_charms(&self.ins)?;
        let empty_vec = vec![];
        let refs = self.strings_of_charms(self.refs.as_ref().unwrap_or(&empty_vec))?;
        let outs = self
            .outs
            .iter()
            .map(|output| self.charms(&output.charms))
            .collect::<Result<_, _>>()?;

        Ok(Transaction { ins, refs, outs })
    }

    fn strings_of_charms(&self, inputs: &Vec<Input>) -> anyhow::Result<BTreeMap<UtxoId, Charms>> {
        inputs
            .iter()
            .map(|input| {
                let utxo_id = input
                    .utxo_id
                    .as_ref()
                    .ok_or(anyhow!("missing input utxo_id"))?;
                let charms = self.charms(&input.charms)?;
                Ok((utxo_id.clone(), charms))
            })
            .collect::<Result<_, _>>()
    }

    fn charms(&self, charms_opt: &Option<KeyedCharms>) -> anyhow::Result<Charms> {
        charms_opt
            .as_ref()
            .ok_or(anyhow!("missing charms field"))?
            .iter()
            .map(|(k, v)| {
                let app = self.apps.get(k).ok_or(anyhow!("missing app {}", k))?;
                Ok((app.clone(), Data::from(v)))
            })
            .collect::<Result<Charms, _>>()
    }

    /// Get a [`NormalizedSpell`] and apps' private inputs for the spell.
    pub fn normalized(&self) -> anyhow::Result<(NormalizedSpell, BTreeMap<App, Data>)> {
        let empty_map = BTreeMap::new();
        let keyed_public_inputs = self.public_inputs.as_ref().unwrap_or(&empty_map);

        let keyed_apps = &self.apps;
        let apps: BTreeSet<App> = keyed_apps.values().cloned().collect();
        let app_to_index: BTreeMap<App, usize> = apps.iter().cloned().zip(0..).collect();
        ensure!(apps.len() == keyed_apps.len(), "duplicate apps");

        let app_public_inputs: BTreeMap<App, Data> = app_inputs(keyed_apps, keyed_public_inputs);

        let ins: Vec<UtxoId> = self
            .ins
            .iter()
            .map(|utxo| utxo.utxo_id.clone().ok_or(anyhow!("missing input utxo_id")))
            .collect::<Result<_, _>>()?;
        ensure!(
            ins.iter().collect::<BTreeSet<_>>().len() == ins.len(),
            "duplicate inputs"
        );
        let ins = Some(ins);

        let empty_vec = vec![];
        let self_refs = self.refs.as_ref().unwrap_or(&empty_vec);
        let refs: BTreeSet<UtxoId> = self_refs
            .iter()
            .map(|utxo| utxo.utxo_id.clone().ok_or(anyhow!("missing input utxo_id")))
            .collect::<Result<_, _>>()?;
        ensure!(refs.len() == self_refs.len(), "duplicate reference inputs");

        let empty_charm = KeyedCharms::new();

        let outs: Vec<NormalizedCharms> = self
            .outs
            .iter()
            .map(|utxo| {
                let n_charms = utxo
                    .charms
                    .as_ref()
                    .unwrap_or(&empty_charm)
                    .iter()
                    .map(|(k, v)| {
                        let app = keyed_apps.get(k).ok_or(anyhow!("missing app key"))?;
                        let i: usize = *app_to_index
                            .get(app)
                            .expect("app should be in app_to_index");
                        Ok((i, Data::from(v)))
                    })
                    .collect::<Result<NormalizedCharms, Error>>()?;
                Ok(n_charms)
            })
            .collect::<Result<_, Error>>()?;

        let norm_spell = NormalizedSpell {
            version: self.version,
            tx: NormalizedTransaction { ins, refs, outs },
            app_public_inputs,
        };

        let keyed_private_inputs = self.private_inputs.as_ref().unwrap_or(&empty_map);
        let app_private_inputs = app_inputs(keyed_apps, keyed_private_inputs);

        Ok((norm_spell, app_private_inputs))
    }

    /// De-normalize a normalized spell.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn denormalized(norm_spell: &NormalizedSpell) -> Self {
        let apps = (0..)
            .zip(norm_spell.app_public_inputs.keys())
            .map(|(i, app)| (utils::str_index(&i), app.clone()))
            .collect();

        let public_inputs = match (0..)
            .zip(norm_spell.app_public_inputs.values())
            .filter_map(|(i, data)| match data {
                data if data.is_empty() => None,
                data => Some((
                    utils::str_index(&i),
                    data.value().ok().expect("Data should be a Value"),
                )),
            })
            .collect::<BTreeMap<_, _>>()
        {
            map if map.is_empty() => None,
            map => Some(map),
        };

        let Some(norm_spell_ins) = &norm_spell.tx.ins else {
            unreachable!("spell must have inputs");
        };
        let ins = norm_spell_ins
            .iter()
            .map(|utxo_id| Input {
                utxo_id: Some(utxo_id.clone()),
                charms: None,
            })
            .collect();

        let refs = match norm_spell
            .tx
            .refs
            .iter()
            .map(|utxo_id| Input {
                utxo_id: Some(utxo_id.clone()),
                charms: None,
            })
            .collect::<Vec<_>>()
        {
            refs if refs.is_empty() => None,
            refs => Some(refs),
        };

        let outs = norm_spell
            .tx
            .outs
            .iter()
            .map(|n_charms| Output {
                address: None,
                amount: None,
                charms: match n_charms
                    .iter()
                    .map(|(i, data)| {
                        (
                            utils::str_index(i),
                            data.value().ok().expect("Data should be a Value"),
                        )
                    })
                    .collect::<KeyedCharms>()
                {
                    charms if charms.is_empty() => None,
                    charms => Some(charms),
                },
            })
            .collect();

        Self {
            version: norm_spell.version,
            apps,
            public_inputs,
            private_inputs: None,
            ins,
            refs,
            outs,
        }
    }
}

fn app_inputs(
    keyed_apps: &BTreeMap<String, App>,
    keyed_inputs: &BTreeMap<String, Data>,
) -> BTreeMap<App, Data> {
    keyed_apps
        .iter()
        .map(|(k, app)| {
            (
                app.clone(),
                keyed_inputs.get(k).cloned().unwrap_or_default(),
            )
        })
        .collect()
}

pub trait Prove {
    /// Prove a spell (provided as [`NormalizedSpell`]).
    /// Returns the normalized spell and the proof (which is a Groth16 proof of checking if the
    /// spell is correct inside a zkVM).
    ///
    /// Requires the binaries of the apps used in the spell, the private inputs to the apps, and the
    /// pre-requisite transactions (`prev_txs`).
    fn prove(
        &self,
        norm_spell: NormalizedSpell,
        app_binaries: &BTreeMap<B32, Vec<u8>>,
        app_private_inputs: BTreeMap<App, Data>,
        prev_txs: Vec<Tx>,
        expected_cycles: Option<Vec<u64>>,
    ) -> anyhow::Result<(NormalizedSpell, Proof, u64)>;
}

impl Prove for Prover {
    fn prove(
        &self,
        norm_spell: NormalizedSpell,
        app_binaries: &BTreeMap<B32, Vec<u8>>,
        app_private_inputs: BTreeMap<App, Data>,
        prev_txs: Vec<Tx>,
        _expected_cycles: Option<Vec<u64>>,
    ) -> anyhow::Result<(NormalizedSpell, Proof, u64)> {
        let mut stdin = SP1Stdin::new();

        let prev_spells = charms_client::prev_spells(&prev_txs, SPELL_VK);

        let app_contract_proofs = norm_spell
            .app_public_inputs
            .iter()
            .zip(0usize..)
            .filter_map(|((app, _), i)| app_binaries.get(&app.vk).map(|_| i))
            .collect();
        let prover_input = SpellProverInput {
            self_spell_vk: SPELL_VK.to_string(),
            prev_txs,
            spell: norm_spell.clone(),
            app_contract_proofs,
        };
        let input_vec: Vec<u8> = util::write(&prover_input)?;

        dbg!(input_vec.len());

        stdin.write_vec(input_vec);

        let tx = to_tx(&norm_spell, &prev_spells);
        let app_public_inputs = &norm_spell.app_public_inputs;

        // TODO add a way to pass the expected cycles to the prover, remove this
        // verify that apps execute within expected cycles
        // if expected_cycles.is_some() {
        //     self.app_prover.run_all(
        //         app_binaries,
        //         &tx,
        //         app_public_inputs,
        //         &app_private_inputs,
        //         expected_cycles,
        //     )?;
        // }

        self.app_prover.prove(
            app_binaries,
            tx,
            app_public_inputs,
            app_private_inputs,
            &mut stdin,
        )?;

        let (pk, _) = self.sp1_client.get().setup(SPELL_CHECKER_BINARY);
        // TODO find a way to get cycles count from the prover, remove this
        let (_, report) = self
            .sp1_client
            .get()
            .execute(SPELL_CHECKER_BINARY, &stdin)?;

        let proof = self
            .sp1_client
            .get()
            .prove(&pk, &stdin, SP1ProofMode::Groth16)?;
        let proof = proof.bytes().into_boxed_slice();

        let mut norm_spell2 = norm_spell;
        norm_spell2.tx.ins = None;

        Ok((norm_spell2, proof, report.total_instruction_count()))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn deserialize_keyed_charm() {
        let y = r#"
$TOAD_SUB: 10
$TOAD: 9
"#;

        let charms: KeyedCharms = serde_yaml::from_str(y).unwrap();
        dbg!(&charms);

        let utxo_id_0 =
            UtxoId::from_str("f72700ac56bd4dd61f2ccb4acdf21d0b11bb294fc3efa9012b77903932197d2f:2")
                .unwrap();
        let buf = util::write(&utxo_id_0).unwrap();

        let utxo_id_data: Data = util::read(buf.as_slice()).unwrap();

        let utxo_id: UtxoId = utxo_id_data.value().unwrap();
        assert_eq!(utxo_id_0, dbg!(utxo_id));
    }
}

pub trait ProveSpellTx {
    fn prove_spell_tx(
        &self,
        prove_request: ProveRequest,
    ) -> impl std::future::Future<Output = anyhow::Result<[bitcoin::Transaction; 2]>>;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CharmsFee {
    pub fee_address: Address<NetworkUnchecked>,
    pub fee_rate: u64,
    pub fee_base: u64,
}

serde_with::serde_conv!(TxHex, Tx, |tx: &Tx| tx.hex(), |s: &str| Tx::from_hex(s));

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProveRequest {
    pub spell: Spell,
    #[serde_as(as = "IfIsHumanReadable<BTreeMap<_, Base64>>")]
    pub binaries: BTreeMap<B32, Vec<u8>>,
    #[serde_as(as = "IfIsHumanReadable<Vec<TxHex>>")]
    pub prev_txs: Vec<Tx>,
    pub funding_utxo: OutPoint,
    pub funding_utxo_value: u64,
    pub change_address: Address<NetworkUnchecked>,
    pub fee_rate: f64,
    pub charms_fee: Option<CharmsFee>,
}

pub struct Prover {
    pub app_prover: Arc<app::Prover>,
    pub sp1_client: Arc<Shared<BoxedSP1Prover>>,
    pub charms_fee_settings: Option<CharmsFee>,
    pub charms_prove_api_url: String,
    #[cfg(not(feature = "prover"))]
    pub client: Client,
}

impl ProveSpellTx for Prover {
    #[cfg(feature = "prover")]
    async fn prove_spell_tx(
        &self,
        ProveRequest {
            spell,
            binaries,
            prev_txs,
            funding_utxo,
            funding_utxo_value,
            change_address,
            fee_rate,
            charms_fee,
        }: ProveRequest,
    ) -> anyhow::Result<[bitcoin::Transaction; 2]> {
        let prev_txs_by_id = txs_by_txid(&prev_txs);

        let tx = bitcoin_tx::from_spell(&spell)?;
        ensure!(tx
            .0
            .input
            .iter()
            .all(|input| prev_txs_by_id
                .contains_key(&TxId(input.previous_output.txid.to_byte_array()))));

        let (norm_spell, app_private_inputs) = spell.normalized()?;

        let prev_spells = charms_client::prev_spells(&prev_txs, SPELL_VK);
        let charms_tx = to_tx(&norm_spell, &prev_spells);

        let expected_cycles = self.app_prover.run_all(
            &binaries,
            &charms_tx,
            &norm_spell.app_public_inputs,
            &app_private_inputs,
            None,
        )?;
        let total_app_cycles: u64 = expected_cycles.iter().sum();

        let (norm_spell, proof, spell_cycles) = self.prove(
            norm_spell,
            &binaries,
            app_private_inputs,
            prev_txs,
            Some(expected_cycles),
        )?;

        tracing::info!(
            "proof generated. total app cycles: {}, spell cycles: {}",
            total_app_cycles,
            spell_cycles,
        );

        // Serialize spell into CBOR
        let spell_data = util::write(&(&norm_spell, &proof))?;

        // Parse change address into ScriptPubkey
        let change_pubkey = change_address.assume_checked().script_pubkey();

        let charms_fee_pubkey = charms_fee
            .clone()
            .map(|fee| fee.fee_address.assume_checked().script_pubkey());

        // Calculate fee
        let charms_fee = get_charms_fee(charms_fee, total_app_cycles, spell_cycles);

        // Parse fee rate
        let fee_rate = FeeRate::from_sat_per_kwu((fee_rate * 250.0) as u64);

        // Call the add_spell function
        let transactions = add_spell(
            tx.0,
            &spell_data,
            funding_utxo,
            Amount::from_sat(funding_utxo_value),
            change_pubkey,
            fee_rate,
            &prev_txs_by_id,
            charms_fee_pubkey,
            charms_fee,
        );
        Ok(transactions)
    }

    #[cfg(not(feature = "prover"))]
    #[tracing::instrument(level = "info", skip_all)]
    async fn prove_spell_tx(
        &self,
        prove_request: ProveRequest,
    ) -> anyhow::Result<[bitcoin::Transaction; 2]> {
        let prove_request = self.add_fee(prove_request);
        let prev_txs_by_id = txs_by_txid(&prove_request.prev_txs);

        let tx = bitcoin_tx::from_spell(&prove_request.spell)?;
        // let encoded_tx = EncodedTx::Bitcoin(BitcoinTx(tx.clone()));
        ensure!(tx
            .0
            .input
            .iter()
            .all(|input| prev_txs_by_id
                .contains_key(&TxId(input.previous_output.txid.to_byte_array()))));

        let (norm_spell, app_private_inputs) = prove_request.spell.normalized()?;

        let prev_spells = charms_client::prev_spells(&prove_request.prev_txs, SPELL_VK);
        let charms_tx = to_tx(&norm_spell, &prev_spells);

        let expected_cycles = self.app_prover.run_all(
            &prove_request.binaries,
            &charms_tx,
            &norm_spell.app_public_inputs,
            &app_private_inputs,
            None,
        )?;
        let total_app_cycles: u64 = expected_cycles.iter().sum();

        let charms_fee =
            get_charms_fee(prove_request.charms_fee.clone(), total_app_cycles, 8000000).to_sat();

        let total_sats_in =
            tx.0.input
                .iter()
                .map(|i| {
                    prev_txs_by_id
                        .get(&TxId(i.previous_output.txid.to_byte_array()))
                        .map(|prev_tx| {
                            let Tx::Bitcoin(BitcoinTx(prev_tx)) = prev_tx else {
                                unreachable!()
                            };
                            prev_tx.output[i.previous_output.vout as usize].value
                        })
                        .unwrap_or_default()
                })
                .sum::<Amount>()
                .to_sat();
        let total_sats_out = tx.0.output.iter().map(|o| o.value).sum::<Amount>().to_sat();

        let funding_utxo_sats = prove_request.funding_utxo_value;

        ensure!(
            total_sats_in + funding_utxo_sats > total_sats_out + charms_fee,
            "total input value must be greater than total output value plus charms fee"
        );

        tracing::info!(
            "tx input sats: {}, funding utxo sats: {}, total output sats: {}, charms fee estimate: {}",
            total_sats_in,
            funding_utxo_sats,
            total_sats_out,
            charms_fee
        );

        let client = &self.client;
        let response = client
            .post(&self.charms_prove_api_url)
            .json(&prove_request)
            .send()
            .await?;
        let [commit_tx, spell_tx]: [String; 2] = response.json().await?;
        let transactions = [deserialize_hex(&commit_tx)?, deserialize_hex(&spell_tx)?];
        Ok(transactions)
    }
}

impl Prover {
    #[cfg(not(feature = "prover"))]
    fn add_fee(&self, prove_request: ProveRequest) -> ProveRequest {
        let mut prove_request = prove_request;
        prove_request.charms_fee = self.charms_fee_settings.clone();
        prove_request
    }
}

fn get_charms_fee(
    charms_fee: Option<CharmsFee>,
    total_app_cycles: u64,
    spell_cycles: u64,
) -> Amount {
    charms_fee
        .as_ref()
        .map(|_| {
            Amount::from_sat(
                (total_app_cycles + spell_cycles) * fee_sats_per_megacycle() / 1000000
                    + fee_sats_base(),
            )
        })
        .unwrap_or_default()
}

fn fee_sats_per_megacycle() -> u64 {
    env::var("CHARMS_FEE_RATE")
        .map(|s| s.parse::<u64>().unwrap())
        .unwrap_or(1000)
}

fn fee_sats_base() -> u64 {
    env::var("CHARMS_FEE_BASE")
        .map(|s| s.parse::<u64>().unwrap())
        .unwrap_or(1000)
}

pub fn align_spell_to_tx(
    norm_spell: NormalizedSpell,
    tx: &bitcoin::Transaction,
) -> anyhow::Result<NormalizedSpell> {
    let mut norm_spell = norm_spell;
    let spell_ins = norm_spell.tx.ins.as_ref().ok_or(anyhow!("no inputs"))?;

    ensure!(
        spell_ins.len() <= tx.input.len(),
        "spell inputs exceed transaction inputs"
    );
    ensure!(
        norm_spell.tx.outs.len() <= tx.output.len(),
        "spell outputs exceed transaction outputs"
    );

    for i in 0..spell_ins.len() {
        let utxo_id = &spell_ins[i];
        let out_point = tx.input[i].previous_output;
        ensure!(
            utxo_id.0 == TxId(out_point.txid.to_byte_array()),
            "input {} txid mismatch: {} != {}",
            i,
            utxo_id.0,
            out_point.txid
        );
        ensure!(
            utxo_id.1 == out_point.vout,
            "input {} vout mismatch: {} != {}",
            i,
            utxo_id.1,
            out_point.vout
        );
    }

    for i in spell_ins.len()..tx.input.len() {
        let out_point = tx.input[i].previous_output;
        let utxo_id = UtxoId(TxId(out_point.txid.to_byte_array()), out_point.vout);
        norm_spell.tx.ins.get_or_insert_with(Vec::new).push(utxo_id);
    }

    Ok(norm_spell)
}
