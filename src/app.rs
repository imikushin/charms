use crate::{
    cli::app::new,
    utils::{BoxedSP1Prover, Shared},
};
use anyhow::ensure;
use charms_data::{is_simple_transfer, util, App, Data, Transaction, B32};
use sha2::{Digest, Sha256};
use sp1_sdk::{
    HashableKey, ProverClient, SP1Context, SP1Proof, SP1ProofMode, SP1Stdin, SP1VerifyingKey,
};
use std::{
    collections::BTreeMap,
    os::fd::{AsFd, AsRawFd},
    sync::Arc,
};
use wasmi::{Engine, Linker, Module, Store};
use wasmi_wasi::{
    add_to_linker,
    wasi_common::pipe::{ReadPipe, WritePipe},
    WasiCtx, WasiCtxBuilder,
};

pub struct Prover {
    pub sp1_client: Arc<Shared<BoxedSP1Prover>>,
    pub engine: Engine,
}

impl Prover {
    pub fn vk(&self, binary: &[u8]) -> B32 {
        let hash = Sha256::digest(binary);
        B32(hash.into())
    }
}

impl Prover {
    pub fn new() -> Self {
        Self {
            sp1_client: Arc::new(Shared::new(|| {
                Box::new(ProverClient::builder().cpu().build())
            })),
            engine: Engine::default(),
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

                        tracing::info!("running app: {}", app);

                        let (committed_values, _digest, report) = self
                            .sp1_client
                            .get()
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

    #[tracing::instrument(level = "info", skip(self, app_binary, tx, x, w))]
    pub fn run(
        &self,
        app_binary: &[u8],
        app: &App,
        tx: &Transaction,
        x: &Data,
        w: &Data,
    ) -> anyhow::Result<()> {
        let vk = self.vk(app_binary);
        ensure!(app.vk == vk, "app.vk mismatch");

        let mut linker = Linker::new(&self.engine);
        add_to_linker(&mut linker, |ctx| ctx)?;

        let stdin = ReadPipe::from(util::write(&(app, tx, x, w))?);

        let wasi_ctx = WasiCtxBuilder::new()
            .stdin(Box::new(stdin))
            .inherit_stderr()
            .build();
        let mut store = Store::new(&self.engine, wasi_ctx);

        let module = Module::new(&self.engine, app_binary)?;

        let instance = linker.instantiate(&mut store, &module)?.start(&mut store)?;

        let Some(main_func) = instance.get_func(&store, "_start") else {
            unreachable!("we should have a main function")
        };
        main_func.typed::<(), ()>(&store)?.call(&mut store, ())?;

        Ok(())
    }
}
