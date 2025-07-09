use crate::tx::groth16_vk;
use anyhow::anyhow;
use ark_bls12_381::Bls12_381;
use ark_ff::{Field, ToConstraintField};
use ark_groth16::{prepare_verifying_key, Groth16, Proof, VerifyingKey};
use ark_serialize::CanonicalDeserialize;
use ark_snark::SNARK;
use sha2::{Digest, Sha256};

pub fn verify_groth16_proof(
    proof: &[u8],
    public_inputs: &[u8],
    spell_version: u32,
) -> anyhow::Result<()> {
    let vk_bytes = groth16_vk(spell_version)?;
    let vk = VerifyingKey::deserialize_compressed(vk_bytes)
        .map_err(|e| anyhow!("Failed to deserialize verifying key: {}", e))?;
    let pvk = prepare_verifying_key::<Bls12_381>(&vk);
    let proof = Proof::deserialize_compressed(proof)?;
    let field_elements = Sha256::digest(public_inputs)
        .to_field_elements()
        .expect("non-empty vector");
    Groth16::<Bls12_381>::verify_with_processed_vk(&pvk, &[field_elements[0]], &proof)?;
    Ok(())
}
