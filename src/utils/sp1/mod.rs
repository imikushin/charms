//! # SP1 CUDA Prover
//!
//! A prover that uses the CUDA to execute and prove programs.

use anyhow::Result;
use sp1_core_machine::io::SP1Stdin;
use sp1_prover::{components::CpuProverComponents, SP1Prover};
use std::sync::Mutex;

use crate::utils::sp1::cuda::SP1CudaProver;
use sp1_sdk::{
    install::groth16_circuit_artifacts_dir, Prover, SP1Proof, SP1ProofMode,
    SP1ProofWithPublicValues, SP1ProvingKey, SP1VerifyingKey,
};

#[cfg(feature = "prover")]
pub(crate) mod cuda;

/// A prover that uses the CPU for execution and the CUDA for proving.
pub struct CudaProver {
    pub(crate) cpu_prover: SP1Prover<CpuProverComponents>,
    pub(crate) cuda_prover: Mutex<SP1CudaProver>,
}

impl CudaProver {
    /// Creates a new [`CudaProver`].
    pub fn new(cpu_prover: SP1Prover) -> Self {
        let cuda_prover = SP1CudaProver::new().expect("Failed to initialize CUDA prover");
        let cuda_prover = Mutex::new(cuda_prover);
        Self {
            cpu_prover,
            cuda_prover,
        }
    }
}

impl Prover<CpuProverComponents> for CudaProver {
    fn inner(&self) -> &SP1Prover<CpuProverComponents> {
        &self.cpu_prover
    }

    #[tracing::instrument(level = "info", skip_all)]
    fn setup(&self, elf: &[u8]) -> (SP1ProvingKey, SP1VerifyingKey) {
        let (pk, _, _, vk) = self.cpu_prover.setup(elf);
        (pk, vk)
    }

    #[tracing::instrument(level = "info", skip_all)]
    fn prove(
        &self,
        pk: &SP1ProvingKey,
        stdin: &SP1Stdin,
        kind: SP1ProofMode,
    ) -> Result<SP1ProofWithPublicValues> {
        let cuda_prover = &*self.cuda_prover.lock().unwrap();

        cuda_prover.ready()?;

        cuda_prover.setup(&pk.elf)?;

        // Generate the core proof.
        let proof = cuda_prover.prove_core(stdin)?;
        if kind == SP1ProofMode::Core {
            return Ok(SP1ProofWithPublicValues {
                proof: SP1Proof::Core(proof.proof.0),
                public_values: proof.public_values,
                sp1_version: self.version().to_string(),
            });
        }

        // Generate the compressed proof.
        let deferred_proofs = stdin
            .proofs
            .iter()
            .map(|(reduce_proof, _)| reduce_proof.clone())
            .collect();
        let public_values = proof.public_values.clone();
        let reduce_proof = cuda_prover.compress(&pk.vk, proof, deferred_proofs)?;
        if kind == SP1ProofMode::Compressed {
            return Ok(SP1ProofWithPublicValues {
                proof: SP1Proof::Compressed(Box::new(reduce_proof)),
                public_values,
                sp1_version: self.version().to_string(),
            });
        }

        // Generate the shrink proof.
        let compress_proof = cuda_prover.shrink(reduce_proof)?;

        // Genenerate the wrap proof.
        let outer_proof = cuda_prover.wrap_bn254(compress_proof)?;

        if kind == SP1ProofMode::Groth16 {
            let groth16_bn254_artifacts = groth16_circuit_artifacts_dir();

            let proof = self
                .cpu_prover
                .wrap_groth16_bn254(outer_proof, &groth16_bn254_artifacts);
            return Ok(SP1ProofWithPublicValues {
                proof: SP1Proof::Groth16(proof),
                public_values,
                sp1_version: self.version().to_string(),
            });
        }

        unimplemented!("Unsupported proof mode: {:?}", kind);
    }
}
