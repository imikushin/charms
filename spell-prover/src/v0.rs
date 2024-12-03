use crate::{AppContractProof, NormalizedSpell, SpellProof};
use charms_data::{
    nft_state_preserved, token_amounts_balanced, App, Data, Transaction, NFT, TOKEN,
};
use serde::{Deserialize, Serialize};
use sp1_primitives::io::SP1PublicValues;
use sp1_verifier::Groth16Verifier;

#[derive(Serialize, Deserialize)]
pub struct V0SpellProof {
    pub vk: [u8; 32],
    pub proof: Option<Box<[u8]>>,
}

fn to_public_values<T: Serialize>(t: &T) -> SP1PublicValues {
    let mut pv = SP1PublicValues::new();
    pv.write(t);
    pv
}

impl<'a> SpellProof for V0SpellProof {
    fn verify(&self, n_spell: &NormalizedSpell) -> bool {
        match &self.proof {
            Some(proof) => Groth16Verifier::verify(
                proof,
                to_public_values(&(&self.vk, n_spell)).as_slice(),
                &format!("0x{}", hex::encode(&self.vk)),
                *sp1_verifier::GROTH16_VK_BYTES,
            )
            .is_ok(),
            None => n_spell.tx.outs.iter().all(|n_charm| n_charm.is_empty()),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct V0AppContractProof {
    pub vk: Option<[u8; 32]>,
    pub proof: Option<Box<[u8]>>,
}

impl AppContractProof for V0AppContractProof {
    fn verify(&self, app: &App, tx: &Transaction, x: &Data) -> bool {
        match &self.proof {
            Some(proof) => {
                let Some(vk) = &self.vk else { unreachable!() };
                Groth16Verifier::verify(
                    proof,
                    to_public_values(&(app, tx, x)).as_slice(),
                    &format!("0x{}", hex::encode(vk)),
                    *sp1_verifier::GROTH16_VK_BYTES,
                )
                .is_ok()
            }
            None => match app.tag {
                TOKEN => token_amounts_balanced(app, &tx),
                NFT => nft_state_preserved(app, &tx),
                _ => false,
            },
        }
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn dummy() {}
}
