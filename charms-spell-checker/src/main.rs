use charms_client::{NormalizedSpell, SpellProverInput};
use charms_data::util;
use charms_spell_checker::is_correct;
use std::io::{Read, Write};

pub fn main() {
    // Read an input to the program.
    let input: SpellProverInput = util::read(std::io::stdin()).unwrap();

    let output = run(input);

    eprintln!("about to commit");

    // Commit to the public values of the program.
    let output_vec = util::write(&output).unwrap();
    std::io::stdout().write_all(&output_vec).unwrap();
}

fn run(input: SpellProverInput) -> (String, NormalizedSpell) {
    let SpellProverInput {
        self_spell_vk,
        prev_txs,
        spell,
        tx_ins_beamed_source_utxos,
        app_input,
    } = input;

    // Check the spell that we're proving is correct.
    assert!(is_correct(
        &spell,
        &prev_txs,
        app_input,
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
