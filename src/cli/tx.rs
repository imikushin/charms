use crate::{
    cli,
    cli::{BITCOIN, CARDANO},
    tx,
};
use anyhow::Result;
use bitcoin::{consensus::encode::serialize_hex, Transaction};
use charms_client::{bitcoin_tx::BitcoinTx, cardano_tx::CardanoTx, tx::Tx, BitcoinFinalityInput};
use charms_data::TxId;
use std::{fs::File, io::Write, path::PathBuf, process::Command};

pub fn tx_show_spell(chain: String, tx: String, json: bool) -> Result<()> {
    let tx = match chain.as_str() {
        BITCOIN => Tx::Bitcoin(BitcoinTx::from_hex(&tx)?),
        CARDANO => Tx::Cardano(CardanoTx::from_hex(&tx)?),
        _ => unimplemented!(),
    };

    match tx::spell(&tx) {
        Some(spell) => cli::print_output(&spell, json)?,
        None => eprintln!("No spell found in the transaction"),
    }

    Ok(())
}

pub(crate) fn get_prev_txs(tx: &Transaction) -> Result<Vec<String>> {
    let cmd_output = Command::new("bash")
        .args(&[
            "-c", format!("bitcoin-cli decoderawtransaction {} | jq -r '.vin[].txid' | sort | uniq | xargs -I {{}} bitcoin-cli getrawtransaction {{}} | paste -sd, -", serialize_hex(tx)).as_str()
        ])
        .output()?;
    String::from_utf8(cmd_output.stdout)?
        .split(',')
        .map(|s| Ok(s.to_string()))
        .collect()
}

/// Uses the `bitcoin-cli` in the shell to fetch a block from the block root and a partial merkle
/// tree proof from the tx id and block root serializes the data to json and saves to given path
/// (CLI calls this with a constant path)
pub(crate) fn fetch_btc_finality_proof_input(
    tx_id: String,
    block_root: String,
    finality_data_path: PathBuf,
) -> Result<()> {
    let fetch_block = Command::new("bash")
        .args(&[
            "-c",
            format!("bitcoin-cli getblock {} false", block_root).as_str(),
        ])
        .output()?;

    let block = String::from_utf8(fetch_block.stdout).expect("Coudln't fetch block!");

    let fetch_txoutproof = Command::new("bash")
        .args(&[
            "-c",
            format!("bitcoin-cli gettxoutproof '[\"{}\"]' {}", tx_id, block_root).as_str(),
        ])
        .output()?;

    let txoutproof = String::from_utf8(fetch_txoutproof.stdout).expect("Coudln't fetch block!");

    let btc_finality_data = BitcoinFinalityInput {
        expected_tx: TxId::from_str(tx_id.as_str()).expect("Failed parsing TxId"),
        pmt_proof: txoutproof.as_bytes().to_vec(),
        block_bytes: block.as_bytes().to_vec(),
    };

    // Write to json file
    let json = serde_json::to_string_pretty(&btc_finality_data)?;
    let mut file = File::create(finality_data_path)?;
    file.write_all(json.as_bytes())?;

    Ok(())
}
