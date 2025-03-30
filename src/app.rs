use crate::utils::BoxedSP1Prover;
use anyhow::ensure;
use charms_data::{is_simple_transfer, util, App, Data, Transaction, B32};
use sp1_sdk::{
    HashableKey, ProverClient, SP1Context, SP1Proof, SP1ProofMode, SP1Stdin, SP1VerifyingKey,
};
use std::{collections::BTreeMap, mem, sync::Arc};

pub struct Prover {
    pub sp1_client: Arc<BoxedSP1Prover>,
}

impl Prover {
    pub fn vk(&self, binary: &[u8]) -> [u8; 32] {
        let (_pk, vk) = self.sp1_client.setup(&binary);
        app_vk(vk)
    }
}

fn app_vk(sp1_vk: SP1VerifyingKey) -> [u8; 32] {
    unsafe {
        let vk: [u32; 8] = sp1_vk.hash_u32();
        mem::transmute(vk)
    }
}

impl Prover {
    pub fn new() -> Self {
        Self {
            sp1_client: Arc::new(Box::new(ProverClient::builder().cpu().build())),
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
                let (pk, vk) = self.sp1_client.setup(binary);
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
            let app_proof = self
                .sp1_client
                .prove(pk, &app_stdin, SP1ProofMode::Compressed)?;

            let SP1Proof::Compressed(compressed_proof) = app_proof.proof else {
                unreachable!()
            };
            tracing::info!("app proof generated: {}", app);
            spell_stdin.write_proof(*compressed_proof, vk.vk.clone());
        }

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub(crate) fn run_all(
        &self,
        app_binaries: &BTreeMap<B32, Vec<u8>>,
        tx: &Transaction,
        app_public_inputs: &BTreeMap<App, Data>,
        app_private_inputs: &BTreeMap<App, Data>,
        expected_cycles: Option<Vec<u64>>,
    ) -> anyhow::Result<Vec<u64>> {
        let app_cycles = app_public_inputs
            .iter()
            .zip(0usize..)
            .map(|((app, x), i)| {
                let mut app_stdin = SP1Stdin::new();
                let empty = Data::empty();
                let w = app_private_inputs.get(app).unwrap_or(&empty);
                app_stdin.write_vec(util::write(&(app, tx, x, w))?);
                let expected_cycles = expected_cycles.as_ref().map(|v| v[i]);
                match app_binaries.get(&app.vk) {
                    Some(app_binary) => {
                        let sp1_context = expected_cycles
                            .map(|v| SP1Context::builder().max_cycles(v).build())
                            .unwrap_or_default();

                        tracing::info!("running app: {:?}", app);

                        let (committed_values, report) =
                            self.sp1_client
                                .inner()
                                .execute(app_binary, &app_stdin, sp1_context)?;

                        let cycles = report.total_instruction_count();
                        if let Some(expected_cycles) = expected_cycles {
                            ensure!(
                                cycles == expected_cycles,
                                "wrong number of cycles running {:?}: {} (actual) != {} (expected)",
                                app,
                                cycles,
                                expected_cycles
                            );
                        }

                        let com: (App, Transaction, Data) =
                            util::read(committed_values.to_vec().as_slice())?;
                        ensure!(
                            (&com.0, &com.1, &com.2) == (app, tx, x),
                            "committed data mismatch"
                        );

                        eprintln!("✅  app contract satisfied: {}", app);

                        Ok(cycles)
                    }
                    None => {
                        tracing::info!("checking simple transfer for: {}", app);
                        if let Some(expected_cycles) = expected_cycles {
                            ensure!(
                                expected_cycles == 0,
                                "missing binary for {:?}: 0 != {} (expected) cycles",
                                app,
                                expected_cycles
                            );
                        }
                        ensure!(is_simple_transfer(app, tx));
                        eprintln!("✅  simple transfer ok: {}", app);
                        Ok(0)
                    }
                }
            })
            .collect::<anyhow::Result<_>>()?;

        Ok(app_cycles)
    }

    pub fn run(
        &self,
        app_binary: &[u8],
        app: &App,
        tx: &Transaction,
        x: &Data,
        w: &Data,
    ) -> anyhow::Result<()> {
        let (_pk, vk) = self.sp1_client.setup(app_binary);
        ensure!(app.vk == B32(app_vk(vk)), "app.vk mismatch");

        let mut app_stdin = SP1Stdin::new();
        app_stdin.write_vec(util::write(&(app, tx, x, w))?);
        let (committed_values, _report) = self.sp1_client.execute(app_binary, &app_stdin)?;
        let com: (App, Transaction, Data) = util::read(committed_values.to_vec().as_slice())?;
        ensure!(
            (&com.0, &com.1, &com.2) == (app, tx, x),
            "committed data mismatch"
        );
        Ok(())
    }
}
