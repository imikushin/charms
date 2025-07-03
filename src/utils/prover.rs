use sp1_core_machine::io::SP1Stdin;
use sp1_prover::{components::CpuProverComponents, SP1Prover, SP1ProvingKey, SP1VerifyingKey};
use sp1_sdk::{CpuProver, EnvProver, Prover, SP1ProofMode, SP1ProofWithPublicValues};

pub trait CharmsSP1Prover: Send + Sync {
    fn inner(&self) -> &SP1Prover<CpuProverComponents>;
    fn setup(&self, elf: &[u8]) -> (SP1ProvingKey, SP1VerifyingKey);
    fn prove(
        &self,
        pk: &SP1ProvingKey,
        stdin: &SP1Stdin,
        kind: SP1ProofMode,
    ) -> anyhow::Result<(SP1ProofWithPublicValues, u64)>;
}

impl CharmsSP1Prover for CpuProver {
    fn inner(&self) -> &SP1Prover<CpuProverComponents> {
        <Self as Prover<CpuProverComponents>>::inner(self)
    }

    fn setup(&self, elf: &[u8]) -> (SP1ProvingKey, SP1VerifyingKey) {
        let (pk, _, _, vk) = <Self as Prover<CpuProverComponents>>::inner(self).setup(elf);
        (pk, vk)
    }

    fn prove(
        &self,
        pk: &SP1ProvingKey,
        stdin: &SP1Stdin,
        kind: SP1ProofMode,
    ) -> anyhow::Result<(SP1ProofWithPublicValues, u64)> {
        let proof = self.prove(pk, stdin).mode(kind).run()?;
        Ok((proof, 0))
    }
}

impl CharmsSP1Prover for EnvProver {
    fn inner(&self) -> &SP1Prover<CpuProverComponents> {
        <Self as Prover<CpuProverComponents>>::inner(self)
    }

    fn setup(&self, elf: &[u8]) -> (SP1ProvingKey, SP1VerifyingKey) {
        let (pk, _, _, vk) = <Self as Prover<CpuProverComponents>>::inner(self).setup(elf);
        (pk, vk)
    }

    fn prove(
        &self,
        pk: &SP1ProvingKey,
        stdin: &SP1Stdin,
        kind: SP1ProofMode,
    ) -> anyhow::Result<(SP1ProofWithPublicValues, u64)> {
        let proof = self.prove(pk, stdin).mode(kind).run()?;
        Ok((proof, 0))
    }
}
