use crate::app;
use anyhow::{anyhow, ensure, Error};
use charms_data::{App, Data, TxId, UtxoId, VkHash};
use ciborium::Value;
use serde::{Deserialize, Serialize};
use sp1_sdk::{HashableKey, ProverClient, SP1Stdin};
use spell_prover::{
    NormalizedCharm, NormalizedSpell, NormalizedTransaction, Proof, SpellProverInput,
};
use std::collections::{BTreeMap, BTreeSet};

/// Charm as represented in a spell.
/// Map of `$TICKER: data`
pub type KeyedCharm = BTreeMap<String, Value>;

/// UTXO as represented in a spell.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Utxo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utxo_id: Option<UtxoId>,
    pub charm: KeyedCharm,
}

/// Defines how spells are represented on the wire,
/// in both human-friendly (JSON/YAML) and machine-friendly (CBOR) formats.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Spell {
    pub version: u32,

    pub apps: BTreeMap<String, App>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_inputs: Option<BTreeMap<String, Value>>,

    pub ins: Vec<Utxo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refs: Option<Vec<Utxo>>,
    pub outs: Vec<Utxo>,

    /// folded proof of all validation predicates plus all pre-requisite spells
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof: Option<Box<[u8]>>,
}

impl Spell {
    pub fn new() -> Self {
        Self {
            version: 0,
            apps: BTreeMap::new(),
            public_inputs: None,
            ins: vec![],
            refs: None,
            outs: vec![],
            proof: None,
        }
    }

    pub fn normalized(&self) -> anyhow::Result<NormalizedSpell> {
        let empty_map = BTreeMap::new();
        let keyed_public_inputs = self.public_inputs.as_ref().unwrap_or(&empty_map);

        let keyed_apps = &self.apps;
        let apps: BTreeSet<App> = keyed_apps.values().cloned().collect();
        let app_to_index: BTreeMap<App, usize> = apps.iter().cloned().zip(0..).collect();
        ensure!(apps.len() == keyed_apps.len(), "duplicate apps");

        let app_public_inputs: BTreeMap<App, Data> = keyed_apps
            .iter()
            .map(|(k, app)| {
                (
                    app.clone(),
                    keyed_public_inputs
                        .get(k)
                        .map(|v| Data::from(v))
                        .unwrap_or_default(),
                )
            })
            .collect();

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

        let outs: Vec<NormalizedCharm> = self
            .outs
            .iter()
            .map(|utxo| {
                let charm = utxo
                    .charm
                    .iter()
                    .map(|(k, v)| {
                        let app = keyed_apps.get(k).ok_or(anyhow!("missing app key"))?;
                        let i: usize = *app_to_index
                            .get(app)
                            .expect("app should be in app_to_index");
                        Ok((i, Data::from(v)))
                    })
                    .collect::<Result<NormalizedCharm, Error>>()?;
                Ok(charm)
            })
            .collect::<Result<_, Error>>()?;

        let norm_spell = NormalizedSpell {
            version: self.version,
            tx: NormalizedTransaction { ins, refs, outs },
            app_public_inputs,
        };
        Ok(norm_spell)
    }
}

impl From<&NormalizedSpell> for Spell {
    fn from(norm_spell: &NormalizedSpell) -> Self {
        let apps = norm_spell
            .app_public_inputs
            .iter()
            .zip(0..)
            .map(|((app, _), i)| (format!("${}", i), app.clone()))
            .collect();
        let public_inputs: BTreeMap<String, Value> = norm_spell
            .app_public_inputs
            .iter()
            .zip(0..)
            .map(|((app, data), i)| (format!("${}", i), data.try_into().unwrap()))
            .collect();
        let public_inputs = Some(public_inputs);

        let ins = todo!(); // need to have the hosting tx to get utxo_ids of inputs

        Self {
            version: norm_spell.version,
            apps,
            public_inputs,
            ins,
            refs: None,
            outs: vec![],
            proof: None,
        }
    }
}

pub const SPELL_PROVER_BINARY: &[u8] =
    include_bytes!("../spell-prover/elf/riscv32im-succinct-zkvm-elf");

pub fn prove(
    norm_spell: NormalizedSpell,
    prev_spell_proofs: BTreeMap<TxId, (NormalizedSpell, Option<Proof>)>,
    app_binaries: &BTreeMap<VkHash, Vec<u8>>,
) -> anyhow::Result<(NormalizedSpell, Proof)> {
    let client = ProverClient::new();
    let (pk, vk) = client.setup(SPELL_PROVER_BINARY);
    let mut stdin = SP1Stdin::new();

    let prev_spells = prev_spells(&prev_spell_proofs);

    let prover_input = SpellProverInput {
        self_spell_vk: vk.bytes32(),
        prev_spell_proofs: prev_spell_proofs.into_iter().collect(),
        spell: norm_spell.clone(),
        app_contract_proofs: norm_spell
            .app_public_inputs
            .iter()
            .map(|(app, _)| (app.clone(), true)) // TODO only pass true if we have a proof
            .collect(),
    };
    let input_vec: Vec<u8> = {
        let mut buf = vec![];
        ciborium::into_writer(&prover_input, &mut buf).unwrap();
        buf
    };

    dbg!(input_vec.len());

    stdin.write_vec(input_vec);
    app::Prover::new().prove(app_binaries, &norm_spell, &prev_spells, &mut stdin);

    let proof = client.prove(&pk, stdin).groth16().run()?;
    let proof = proof.bytes().into_boxed_slice();

    let mut norm_spell = norm_spell;
    norm_spell.tx.ins = None;

    Ok((norm_spell, proof))
}

fn prev_spells(
    prev_spell_proofs: &BTreeMap<TxId, (NormalizedSpell, Option<Proof>)>,
) -> BTreeMap<TxId, NormalizedSpell> {
    prev_spell_proofs
        .iter()
        .map(|(txid, (spell, _))| (txid.clone(), spell.clone()))
        .collect()
}

#[cfg(test)]
mod test {
    use super::*;
    use charms_data::*;

    use proptest::prelude::*;

    use ciborium::Value;
    use hex;

    #[test]
    fn deserialize_keyed_charm() {
        let y = r#"
$TOAD_SUB: 10
$TOAD: 9
"#;

        let charm = serde_yaml::from_str::<KeyedCharm>(y).unwrap();
        dbg!(&charm);

        let utxo_id =
            UtxoId::from_str("f72700ac56bd4dd61f2ccb4acdf21d0b11bb294fc3efa9012b77903932197d2f:2")
                .unwrap();
        let mut buf = vec![];
        ciborium::ser::into_writer(&utxo_id, &mut buf).unwrap();

        let utxo_id_value: Value = ciborium::de::from_reader(buf.as_slice()).unwrap();

        let utxo_id: UtxoId = dbg!(utxo_id_value).deserialized().unwrap();
        dbg!(utxo_id);
    }

    #[test]
    fn empty_postcard() {
        use postcard;

        let value: Vec<u8> = vec![];
        let buf = postcard::to_stdvec(&value).unwrap();
        dbg!(buf.len());
        dbg!(buf);

        let mut cbor_buf = vec![];
        let value: Vec<u8> = vec![];
        ciborium::into_writer(&value, &mut cbor_buf).unwrap();
        dbg!(cbor_buf.len());
        dbg!(cbor_buf);
    }
}
