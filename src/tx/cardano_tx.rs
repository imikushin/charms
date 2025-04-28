use crate::{spell, spell::Spell};
use anyhow::Error;
use cardano_serialization_lib::{
    Address, Coin, Transaction, TransactionBody, TransactionInput, TransactionInputs,
    TransactionOutput, TransactionOutputs, TransactionWitnessSet, Value,
};
use charms_client::{cardano_tx::CardanoTx, tx::Tx};
use charms_data::{TxId, UtxoId};
use std::collections::BTreeMap;

fn tx_input(ins: &[spell::Input]) -> TransactionInputs {
    let mut inputs = TransactionInputs::new();
    for input in ins {
        if let Some(utxo_id) = &input.utxo_id {
            let tx_input = TransactionInput::new(&utxo_id.0 .0.into(), utxo_id.1.into());
            inputs.add(&tx_input);
        }
    }
    inputs
}

fn tx_output(outs: &[spell::Output]) -> anyhow::Result<TransactionOutputs> {
    let mut outputs = TransactionOutputs::new();
    for output in outs {
        if let Some(addr) = &output.address {
            let amount = output.amount.unwrap_or(1000000); // TODO make a constant
            let tx_output = TransactionOutput::new(
                &Address::from_bech32(addr)?,
                &Value::new(&Coin::from(amount)),
            );
            outputs.add(&tx_output);
        }
    }
    Ok(outputs)
}

pub fn from_spell(spell: &Spell) -> anyhow::Result<CardanoTx> {
    let inputs = tx_input(&spell.ins);
    let outputs = tx_output(&spell.outs)?;

    let fee = Coin::zero(); // TODO: calculate fee

    let body = TransactionBody::new_tx_body(&inputs, &outputs, &fee);
    let witness_set = TransactionWitnessSet::new();

    let tx = Transaction::new(&body, &witness_set, None);

    Ok(CardanoTx(tx))
}

fn add_spell(
    tx: Transaction,
    spell_data: &[u8],
    funding_utxo: UtxoId,
    funding_utxo_value: u64,
    change_address: Address,
    prev_txs_by_id: &BTreeMap<TxId, Tx>,
) -> Vec<Transaction> {
    todo!()
}

pub(crate) fn make_transactions(
    spell: &Spell,
    funding_utxo: UtxoId,
    funding_utxo_value: u64,
    change_address: &String,
    prev_txs_by_id: &BTreeMap<TxId, Tx>,
    spell_data: &[u8],
) -> Result<Vec<Tx>, Error> {
    let change_address = Address::from_bech32(change_address)?;
    let tx = from_spell(spell)?;

    let transactions = add_spell(
        tx.0,
        spell_data,
        funding_utxo,
        funding_utxo_value,
        change_address,
        prev_txs_by_id,
    );
    Ok(transactions
        .into_iter()
        .map(|tx| Tx::Cardano(CardanoTx(tx)))
        .collect())
}
