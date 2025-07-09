use crate::is_correct;
use charms_client::{
    finality::U32_BTC_FINALITY_VK, NetworkFinalityProofs, NormalizedSpell, SpellCheckerProverInput,
    SpellProverInput,
};
use charms_data::util;
use sha2::{Digest, Sha256};

pub fn main() {
    // Read an input to the program.
    let input_vec = sp1_zkvm::io::read_vec();
    let input: SpellCheckerProverInput = util::read(input_vec.as_slice()).unwrap();

    let output = run(input);

    eprintln!("about to commit");

    // Commit to the public values of the program.
    let output_vec = util::write(&output).unwrap();
    sp1_zkvm::io::commit_slice(output_vec.as_slice());
}

fn verify_finality_proofs(proofs: Vec<NetworkFinalityProofs>) -> bool {
    for proof in proofs {
        match proof {
            NetworkFinalityProofs::Bitcoin(btc_finality_data) => {
                eprintln!(
                    "Handling Bitcoin finality proof for tx: {:?}",
                    btc_finality_data.expected_tx
                );

                let pv_values_serialized: Vec<u8> = serde_cbor::to_vec(&btc_finality_data).unwrap();
                let finality_pv_digest: [u8; 32] = Sha256::digest(pv_values_serialized).into();

                sp1_zkvm::lib::verify::verify_sp1_proof(&U32_BTC_FINALITY_VK, &finality_pv_digest);

                eprintln!("Bitcoin Tx finality proof is correct!");
            }
            _ => {
                unimplemented!()
            }
        }
    }

    true
}

pub(crate) fn run(input: SpellCheckerProverInput) -> (String, NormalizedSpell) {
    let SpellProverInput {
        self_spell_vk,
        prev_txs,
        spell,
        app_prover_output,
        tx_ins_beamed_source_utxos,
    } = input.spell_input;

    // Verify finality proofs are correct
    if let Some(proofs) = input.finality_input {
        assert!(verify_finality_proofs(proofs));
    }

    // Check the spell that we're proving is correct.
    assert!(is_correct(
        &spell,
        &prev_txs,
        app_prover_output,
        &self_spell_vk,
        &tx_ins_beamed_source_utxos,
    ));

    eprintln!("Spell is correct!");

    (self_spell_vk, spell)
}

#[cfg(test)]
mod test {
    #[test]
    fn dummy() {}
}
