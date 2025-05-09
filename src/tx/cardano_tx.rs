use crate::{spell, spell::Spell};
use anyhow::Error;
use cardano_serialization_lib::{
    min_fee_for_size, Address, Coin, LinearFee, PlutusData, Transaction, TransactionBody,
    TransactionInput, TransactionInputs, TransactionOutput, TransactionOutputs,
    TransactionWitnessSet, Value,
};
use charms_client::{
    cardano_tx::{tx_hash, tx_id, CardanoTx},
    tx::Tx,
};
use charms_data::{TxId, UtxoId};
use std::collections::BTreeMap;

fn tx_input(ins: &[spell::Input]) -> TransactionInputs {
    let mut inputs = TransactionInputs::new();
    for input in ins {
        if let Some(utxo_id) = &input.utxo_id {
            let tx_input = TransactionInput::new(&tx_hash(utxo_id.0), utxo_id.1.into());
            inputs.add(&tx_input);
        }
    }
    inputs
}

pub const ONE_ADA: u64 = 1000000;

fn tx_output(outs: &[spell::Output]) -> anyhow::Result<TransactionOutputs> {
    let mut outputs = TransactionOutputs::new();
    for output in outs {
        if let Some(addr) = &output.address {
            let amount = output.amount.unwrap_or(ONE_ADA);
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

    let fee = Coin::zero();

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
    let tx_body = tx.body();

    let mut tx_inputs = tx_body.inputs();
    let orig_inputs_amount = inputs_total_amount(tx_body.inputs(), prev_txs_by_id);

    let mut tx_outputs = tx_body.outputs();
    let orig_outputs_count = tx_outputs.len() as u64;
    let mut temp_tx_outputs = tx_body.outputs();

    let funding_utxo_input = TransactionInput::new(&tx_hash(funding_utxo.0), funding_utxo.1.into());
    tx_inputs.add(&funding_utxo_input);

    let mut temp_data_output = TransactionOutput::new(&change_address, &Value::new(&Coin::zero()));
    temp_data_output.set_plutus_data(&PlutusData::new_bytes(spell_data.to_vec()));

    temp_tx_outputs.add(&temp_data_output);

    let temp_tx_body = TransactionBody::new_tx_body(&tx_inputs, &temp_tx_outputs, &Coin::one());
    let temp_tx_witness_set = TransactionWitnessSet::new();
    let temp_tx = Transaction::new(&temp_tx_body, &temp_tx_witness_set, None);

    let min_fee_a: u64 = 44; // lovelace/byte
    let min_fee_b: u64 = 155381 + 50000; // lovelace
    let linear_fee = LinearFee::new(&min_fee_a.into(), &min_fee_b.into());

    let fee = min_fee_for_size(temp_tx.to_bytes().len() + 100, &linear_fee).unwrap();

    let change = Coin::from(funding_utxo_value + orig_inputs_amount - orig_outputs_count * ONE_ADA)
        .checked_sub(&fee)
        .unwrap();

    let mut data_output = TransactionOutput::new(&change_address, &Value::new(&change));
    data_output.set_plutus_data(&PlutusData::new_bytes(spell_data.to_vec()));

    tx_outputs.add(&data_output);

    let tx_body = TransactionBody::new_tx_body(&tx_inputs, &tx_outputs, &fee);
    let tx_witness_set = TransactionWitnessSet::new();
    let tx = Transaction::new(&tx_body, &tx_witness_set, None);

    vec![tx]
}

fn inputs_total_amount(tx_inputs: TransactionInputs, prev_txs_by_id: &BTreeMap<TxId, Tx>) -> u64 {
    tx_inputs
        .into_iter()
        .map(|tx_input| {
            let tx_id = tx_id(tx_input.transaction_id());
            let Some(Tx::Cardano(CardanoTx(tx))) = prev_txs_by_id.get(&tx_id) else {
                unreachable!("we should already have the tx in the map")
            };
            let prev_tx_out = tx.body().outputs().get(tx_input.index() as usize);
            let amount: u64 = prev_tx_out.amount().coin().into();
            amount
        })
        .sum()
}

pub fn make_transactions(
    spell: &Spell,
    funding_utxo: UtxoId,
    funding_utxo_value: u64,
    change_address: &String,
    spell_data: &[u8],
    prev_txs_by_id: &BTreeMap<TxId, Tx>,
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
