use crate::utils::{BoxedSP1Prover, Shared};
use charms_app_runner::AppRunner;
use charms_data::{util, App, Data, Transaction, B32};
use sp1_sdk::{ProverClient, SP1Proof, SP1ProofMode, SP1Stdin};
use std::{collections::BTreeMap, sync::Arc};

pub struct Prover {
    pub sp1_client: Arc<Shared<BoxedSP1Prover>>,
    pub runner: AppRunner,
}

impl Prover {
    pub fn new() -> Self {
        Self {
            sp1_client: Arc::new(Shared::new(|| {
                Box::new(ProverClient::builder().cpu().build())
            })),
            runner: AppRunner::new(),
        }
    }

    pub(crate) fn prove(
        &self,
        app_binaries: &BTreeMap<B32, Vec<u8>>,
        tx: Transaction,
        app_public_inputs: &BTreeMap<App, Data>,
        app_private_inputs: BTreeMap<App, Data>,
        spell_stdin: &mut SP1Stdin,
    ) -> anyhow::Result<()> {
        let pk_vks = app_binaries
            .iter()
            .map(|(vk_hash, binary)| {
                let (pk, vk) = self.sp1_client.get().setup(binary);
                (vk_hash, (pk, vk))
            })
            .collect::<BTreeMap<_, _>>();

        for (app, x) in app_public_inputs {
            let Some((pk, vk)) = pk_vks.get(&app.vk) else {
                tracing::info!("app binary not provided: {}", app);
                continue;
            };
            tracing::info!("proving app: {}", app);
            let mut app_stdin = SP1Stdin::new();
            let empty = Data::empty();
            let w = app_private_inputs.get(app).unwrap_or(&empty);
            app_stdin.write_vec(util::write(&(app, &tx, x, w))?);
            let app_proof =
                self.sp1_client
                    .get()
                    .prove(pk, &app_stdin, SP1ProofMode::Compressed)?;

            let SP1Proof::Compressed(compressed_proof) = app_proof.proof else {
                unreachable!()
            };
            tracing::info!("app proof generated: {}", app);
            spell_stdin.write_proof(*compressed_proof, vk.vk.clone());
        }

        Ok(())
    }
}
