use crate::{
    cli,
    cli::WalletListParams,
    spell::{KeyedCharms, Spell},
    tx,
    utils::str_index,
};
use anyhow::{ensure, Result};
use bitcoin::{address::NetworkUnchecked, hashes::Hash, Address, OutPoint, Transaction};
use charms_data::{App, Data, TxId, UtxoId};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    process::{Command, Stdio},
};

pub trait List {
    fn list(&self, params: WalletListParams) -> Result<()>;
}

pub struct WalletCli {
    // pub app_prover: Rc<app::Prover>,
    // pub sp1_client: Rc<Box<dyn Prover<CpuProverComponents>>>,
    // pub spell_prover: Rc<spell::Prover>,
}

#[derive(Debug, Deserialize)]
struct BListUnspentItem {
    txid: String,
    vout: u32,
    amount: f64,
    confirmations: u32,
    solvable: bool,
}

#[derive(Debug, Serialize)]
struct OutputWithCharms {
    confirmations: u32,
    sats: u64,
    charms: BTreeMap<String, Data>,
}

type ParsedCharms = BTreeMap<App, Data>;

#[derive(Debug, Serialize)]
struct AppsAndCharmsOutputs {
    apps: BTreeMap<String, App>,
    outputs: BTreeMap<UtxoId, OutputWithCharms>,
}

impl List for WalletCli {
    fn list(&self, params: WalletListParams) -> Result<()> {
        let b_cli = Command::new("bitcoin-cli")
            .args(&["listunspent", "0"]) // include outputs with 0 confirmations
            .stdout(Stdio::piped())
            .spawn()?;
        let output = b_cli.wait_with_output()?;
        let b_list_unspent: Vec<BListUnspentItem> = serde_json::from_slice(&output.stdout)?;

        let unspent_charms_outputs = outputs_with_charms(b_list_unspent)?;

        cli::print_output(&unspent_charms_outputs, params.json)?;
        Ok(())
    }
}

fn outputs_with_charms(b_list_unspent: Vec<BListUnspentItem>) -> Result<AppsAndCharmsOutputs> {
    let txid_set = b_list_unspent
        .iter()
        .map(|item| item.txid.clone())
        .collect::<BTreeSet<_>>();
    let spells = txs_with_spells(txid_set.into_iter())?;
    let utxos_with_charms: BTreeMap<UtxoId, (BListUnspentItem, ParsedCharms)> =
        utxos_with_charms(spells, b_list_unspent);
    let apps = collect_apps(&utxos_with_charms);

    Ok(AppsAndCharmsOutputs {
        apps: enumerate_apps(&apps),
        outputs: pretty_outputs(utxos_with_charms, &apps),
    })
}

fn txs_with_spells(txid_iter: impl Iterator<Item = String>) -> Result<BTreeMap<TxId, Spell>> {
    let txs_with_spells = txid_iter
        .map(|txid| {
            let tx: Transaction = get_tx(&txid)?;
            Ok(tx)
        })
        .map(|tx_result: Result<Transaction>| {
            let tx = tx_result?;
            let spell_opt = tx::spell(&tx);
            Ok(spell_opt.map(|spell| (TxId(tx.compute_txid().to_byte_array()), spell)))
        })
        .filter_map(|tx_result| match tx_result {
            Ok(Some(v)) => Some(Ok(v)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        })
        .collect::<Result<_>>()?;

    Ok(txs_with_spells)
}

fn utxos_with_charms(
    spells: BTreeMap<TxId, Spell>,
    b_list_unspent: Vec<BListUnspentItem>,
) -> BTreeMap<UtxoId, (BListUnspentItem, ParsedCharms)> {
    b_list_unspent
        .into_iter()
        .filter(|item| item.solvable)
        .filter_map(|b_utxo| {
            let txid =
                TxId::from_str(&b_utxo.txid).expect("txids from bitcoin-cli should be valid");
            let i = b_utxo.vout;
            spells
                .get(&txid)
                .and_then(|spell| spell.outs.get(i as usize).map(|u| (u, &spell.apps)))
                .and_then(|(u, apps)| u.charms.as_ref().map(|keyed_charms| (keyed_charms, apps)))
                .map(|(keyed_charms, apps)| {
                    (UtxoId(txid, i), (b_utxo, parsed_charms(keyed_charms, apps)))
                })
        })
        .collect()
}

fn parsed_charms(keyed_charms: &KeyedCharms, apps: &BTreeMap<String, App>) -> ParsedCharms {
    keyed_charms
        .iter()
        .filter_map(|(k, v)| apps.get(k).map(|app| (app.clone(), v.clone())))
        .collect()
}

fn collect_apps(
    strings_of_charms: &BTreeMap<UtxoId, (BListUnspentItem, ParsedCharms)>,
) -> BTreeMap<App, String> {
    let apps: BTreeSet<App> = strings_of_charms
        .iter()
        .flat_map(|(_utxo, (_sats, charms))| charms.keys())
        .cloned()
        .collect();
    apps.into_iter()
        .zip(0..)
        .map(|(app, i)| (app, str_index(&i)))
        .collect()
}

fn enumerate_apps(apps: &BTreeMap<App, String>) -> BTreeMap<String, App> {
    apps.iter()
        .map(|(app, i)| (i.clone(), app.clone()))
        .collect()
}

fn pretty_outputs(
    utxos_with_charms: BTreeMap<UtxoId, (BListUnspentItem, ParsedCharms)>,
    apps: &BTreeMap<App, String>,
) -> BTreeMap<UtxoId, OutputWithCharms> {
    utxos_with_charms
        .into_iter()
        .map(|(utxo_id, (utxo, charms))| {
            let charms = charms
                .iter()
                .map(|(app, value)| (apps[app].clone(), value.clone()))
                .collect();
            let confirmations = utxo.confirmations;
            let sats = (utxo.amount * 100000000f64) as u64;
            (
                utxo_id.clone(),
                OutputWithCharms {
                    confirmations,
                    sats,
                    charms,
                },
            )
        })
        .collect()
}

fn get_tx(txid: &str) -> Result<Transaction> {
    let b_cli = Command::new("bitcoin-cli")
        .args(&["getrawtransaction", txid])
        .stdout(Stdio::piped())
        .spawn()?;
    let output = b_cli.wait_with_output()?;
    ensure!(
        output.status.success(),
        "bitcoin-cli getrawtransaction failed"
    );
    let tx_hex = String::from_utf8(output.stdout)?;
    let tx_hex = tx_hex.trim();
    let tx = bitcoin::consensus::encode::deserialize_hex(&(tx_hex))?;
    Ok(tx)
}

pub const MIN_SATS: u64 = 1000;

pub(crate) fn sign_spell_tx(spell_tx_hex: &String, commit_tx: &Transaction) -> Result<String> {
    let cmd_line = format!(
        r#"bitcoin-cli signrawtransactionwithwallet {} '[{{"txid":"{}","vout":0,"scriptPubKey":"{}","amount":{}}}]' | jq -r '.hex'"#,
        spell_tx_hex,
        commit_tx.compute_txid(),
        &commit_tx.output[0].script_pubkey.to_hex_string(),
        commit_tx.output[0].value.to_btc()
    );
    let cmd_out = Command::new("bash")
        .args(&["-c", cmd_line.as_str()])
        .output()?;
    Ok(String::from_utf8(cmd_out.stdout)?.trim().to_string())
}

pub(crate) fn sign_tx(tx_hex: &str) -> Result<String> {
    let cmd_out = Command::new("bash")
        .args(&[
            "-c",
            format!(
                "bitcoin-cli signrawtransactionwithwallet {} | jq -r '.hex'",
                tx_hex
            )
            .as_str(),
        ])
        .output()?;
    Ok(String::from_utf8(cmd_out.stdout)?.trim().to_string())
}

pub(crate) fn new_change_address() -> Result<Address<NetworkUnchecked>> {
    let cmd_out = Command::new("bitcoin-cli")
        .args(&["getrawchangeaddress"])
        .output()?;
    Ok(String::from_utf8(cmd_out.stdout)?
        .trim()
        .to_string()
        .parse()?)
}

pub(crate) fn funding_utxo_value(utxo: &OutPoint) -> Result<u64> {
    let cmd = format!(
        "bitcoin-cli gettxout {} {} | jq -r '.value*100000000 | round'",
        utxo.txid, utxo.vout
    );
    let cmd_out = Command::new("bash").args(&["-c", &cmd]).output()?;
    Ok(String::from_utf8(cmd_out.stdout)?.trim().parse()?)
}
