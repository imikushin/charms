use anyhow::anyhow;
use async_trait::async_trait;
use reqwest::{Request, Response};
use sp1_core_machine::{io::SP1Stdin, reduce::SP1ReduceProof, utils::SP1CoreProverError};
use sp1_cuda::{
    block_on,
    proto::api::{ProverServiceClient, ReadyRequest},
    CompressRequestPayload, ProveCoreRequestPayload, SetupRequestPayload, SetupResponsePayload,
    ShrinkRequestPayload, WrapRequestPayload,
};
use sp1_prover::{
    InnerSC, OuterSC, SP1CoreProof, SP1ProvingKey, SP1RecursionProverError, SP1VerifyingKey,
};
use std::time::{Duration, Instant};
use twirp::{
    async_trait,
    reqwest::{self},
    url::Url,
    Client, Middleware, Next,
};

/// A remote client to [sp1_prover::SP1Prover] that runs inside a container.
///
/// This is currently used to provide experimental support for GPU hardware acceleration.
///
/// **WARNING**: This is an experimental feature and may not work as expected.
pub struct SP1CudaProver {
    /// The gRPC client to communicate with the container.
    client: Client,
}

impl SP1CudaProver {
    /// Creates a new [SP1Prover] that runs inside a Docker container and returns a
    /// [SP1ProverClient] that can be used to communicate with the container.
    pub fn new() -> anyhow::Result<Self> {
        let gpu_service_url = gpu_service_url();

        let client = Client::new(
            Url::parse(&gpu_service_url).expect("failed to parse url"),
            reqwest::Client::new(),
            vec![Box::new(LoggingMiddleware)],
        )?;

        Ok(Self { client })
    }

    #[tracing::instrument(level = "info", skip_all)]
    pub fn ready(&self) -> anyhow::Result<()> {
        block_on(async {
            // Check if the container is ready
            let timeout = Duration::from_secs(30);
            let start_time = Instant::now();

            tracing::info!("waiting for proving server to be ready");
            loop {
                if start_time.elapsed() > timeout {
                    return Err("Timeout: proving server did not become ready within 30 seconds. Please check your network settings.".to_string());
                }

                match self.client.ready(ReadyRequest {}).await {
                    Ok(response) if response.ready => {
                        break;
                    }
                    Ok(_) => {
                        tracing::info!("proving server is not ready, retrying...");
                    }
                    Err(e) => {
                        tracing::warn!("Error checking server readiness: {}", e);
                    }
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Ok(())
        }).map_err(|e| anyhow!(e))?;
        Ok(())
    }

    /// Executes the [sp1_prover::SP1Prover::setup] method inside the container.
    #[tracing::instrument(level = "info", skip_all)]
    pub fn setup(&self, elf: &[u8]) -> anyhow::Result<(SP1ProvingKey, SP1VerifyingKey)> {
        let payload = SetupRequestPayload { elf: elf.to_vec() };
        let request = sp1_cuda::proto::api::SetupRequest {
            data: bincode::serialize(&payload).unwrap(),
        };
        let response = block_on(async { self.client.setup(request).await }).unwrap();
        let payload: SetupResponsePayload = bincode::deserialize(&response.result).unwrap();
        Ok((payload.pk, payload.vk))
    }

    /// Executes the [sp1_prover::SP1Prover::prove_core] method inside the container.
    ///
    /// You will need at least 24GB of VRAM to run this method.
    #[tracing::instrument(level = "info", skip_all)]
    pub fn prove_core(&self, stdin: &SP1Stdin) -> Result<SP1CoreProof, SP1CoreProverError> {
        let payload = ProveCoreRequestPayload {
            stdin: stdin.clone(),
        };
        let request = sp1_cuda::proto::api::ProveCoreRequest {
            data: bincode::serialize(&payload).unwrap(),
        };
        let response = block_on(async { self.client.prove_core(request).await }).unwrap();
        let proof: SP1CoreProof = bincode::deserialize(&response.result).unwrap();
        Ok(proof)
    }

    /// Executes the [sp1_prover::SP1Prover::compress] method inside the container.
    ///
    /// You will need at least 24GB of VRAM to run this method.
    #[tracing::instrument(level = "info", skip_all)]
    pub fn compress(
        &self,
        vk: &SP1VerifyingKey,
        proof: SP1CoreProof,
        deferred_proofs: Vec<SP1ReduceProof<InnerSC>>,
    ) -> Result<SP1ReduceProof<InnerSC>, SP1RecursionProverError> {
        let payload = CompressRequestPayload {
            vk: vk.clone(),
            proof,
            deferred_proofs,
        };
        let request = sp1_cuda::proto::api::CompressRequest {
            data: bincode::serialize(&payload).unwrap(),
        };

        let response = block_on(async { self.client.compress(request).await }).unwrap();
        let proof: SP1ReduceProof<InnerSC> = bincode::deserialize(&response.result).unwrap();
        Ok(proof)
    }

    /// Executes the [sp1_prover::SP1Prover::shrink] method inside the container.
    ///
    /// You will need at least 24GB of VRAM to run this method.
    #[tracing::instrument(level = "info", skip_all)]
    pub fn shrink(
        &self,
        reduced_proof: SP1ReduceProof<InnerSC>,
    ) -> Result<SP1ReduceProof<InnerSC>, SP1RecursionProverError> {
        let payload = ShrinkRequestPayload {
            reduced_proof: reduced_proof.clone(),
        };
        let request = sp1_cuda::proto::api::ShrinkRequest {
            data: bincode::serialize(&payload).unwrap(),
        };

        let response = block_on(async { self.client.shrink(request).await }).unwrap();
        let proof: SP1ReduceProof<InnerSC> = bincode::deserialize(&response.result).unwrap();
        Ok(proof)
    }

    /// Executes the [sp1_prover::SP1Prover::wrap_bn254] method inside the container.
    ///
    /// You will need at least 24GB of VRAM to run this method.
    #[tracing::instrument(level = "info", skip_all)]
    pub fn wrap_bn254(
        &self,
        reduced_proof: SP1ReduceProof<InnerSC>,
    ) -> Result<SP1ReduceProof<OuterSC>, SP1RecursionProverError> {
        let payload = WrapRequestPayload {
            reduced_proof: reduced_proof.clone(),
        };
        let request = sp1_cuda::proto::api::WrapRequest {
            data: bincode::serialize(&payload).unwrap(),
        };

        let response = block_on(async { self.client.wrap(request).await }).unwrap();
        let proof: SP1ReduceProof<OuterSC> = bincode::deserialize(&response.result).unwrap();
        Ok(proof)
    }
}

struct LoggingMiddleware;

#[async_trait]
impl Middleware for LoggingMiddleware {
    async fn handle(&self, req: Request, next: Next<'_>) -> sp1_cuda::Result<Response> {
        tracing::debug!("{:?}", req);
        let response = next.run(req).await;
        match response {
            Ok(response) => {
                tracing::info!("{:?}", response);
                Ok(response)
            }
            Err(e) => Err(e),
        }
    }
}

fn gpu_service_url() -> String {
    std::env::var("SP1_GPU_SERVICE_URL").unwrap_or("http://localhost:3000/twirp/".to_string())
}
