`charms-spell-checker` is not a spelling checker: it's a validator for spells.

It runs in a zkVM to produce recursive proofs of correctness for spells — metadata on transactions that
specifies what charms are created in the transaction outputs.

### Building

The guest Dockerfile pins a published zkVM toolchain.

`charms-spell-checker` validates spells. `charms-proof-wrapper` verifies that proof and is the program proved as a
Groth16 SNARK. The image derives `SPELL_CHECKER_VK` from the spell-checker ELF and writes it into the wrapper before
that guest is compiled. It also writes both serialized SP1 verifying keys, and writes the wrapper key into
`charms-lib` as `SPELL_VK`. From the repository root, build the image and copy the ELFs into `src/bin`, where the
host crate embeds them, and copy the keys and sources back with them. `Prover::new` checks those cached keys against
the constants. The script prints the type of each ELF:

```sh
charms-spell-checker/build-guests.sh
```
