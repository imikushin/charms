use crate::{
    cli::ServerConfig,
    spell::{ProveRequest, ProveSpellTx, Prover},
    utils::AsyncShared,
};
#[cfg(not(feature = "prover"))]
use crate::{spell::Spell, tx::norm_spell};
use anyhow::Result;
use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Json, Router,
};
#[cfg(not(feature = "prover"))]
use axum::{extract::Path, routing::put};
#[cfg(not(feature = "prover"))]
use bitcoin::consensus::encode::deserialize_hex;
use bitcoin::consensus::encode::serialize_hex;
#[cfg(not(feature = "prover"))]
use bitcoincore_rpc::{jsonrpc::Error::Rpc, Auth, Client, RpcApi};
use serde::{Deserialize, Serialize};
#[cfg(not(feature = "prover"))]
use std::str::FromStr;
use std::sync::Arc;

pub struct Server {
    pub config: ServerConfig,
    #[cfg(not(feature = "prover"))]
    pub rpc: Arc<Client>,
    pub prover: Arc<AsyncShared<Prover>>,
}

// Types
#[derive(Debug, Serialize, Deserialize)]
struct ShowSpellRequest {
    tx_hex: String,
}

impl Server {
    pub fn new(config: ServerConfig, prover: AsyncShared<Prover>) -> Self {
        #[cfg(not(feature = "prover"))]
        let rpc = Arc::new(bitcoind_client(
            config.rpc_url.clone(),
            config.rpc_user.clone(),
            config.rpc_password.clone(),
        ));
        let prover = Arc::new(prover);
        Self {
            config,
            #[cfg(not(feature = "prover"))]
            rpc,
            prover,
        }
    }

    pub async fn serve(&self) -> Result<()> {
        let ServerConfig { ip, port, .. } = &self.config;

        // Build router with CORS middleware
        let app = Router::new();
        #[cfg(not(feature = "prover"))]
        let app = app
            .route("/spells/{txid}", get(show_spell_by_txid))
            .with_state(self.rpc.clone())
            .route("/spells/{txid}", put(show_spell_for_tx_hex));
        let app = app
            .route("/spells/prove", post(prove_spell))
            .with_state(self.prover.clone())
            .route("/ready", get(|| async { "OK" }))
            .layer(middleware::from_fn(cors_middleware));

        // Run server
        let addr = format!("{}:{}", ip, port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        tracing::info!("Server running on {}", &addr);

        axum::serve(listener, app).await?;
        Ok(())
    }
}

async fn cors_middleware(request: axum::http::Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;

    let headers = response.headers_mut();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, PUT, POST, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Content-Type"),
    );

    response
}

// Handlers
#[cfg(not(feature = "prover"))]
#[tracing::instrument(level = "debug", skip_all)]
async fn show_spell_by_txid(
    State(rpc): State<Arc<Client>>,
    Path(txid): Path<String>,
) -> Result<Json<Spell>, StatusCode> {
    get_spell(rpc, &txid).map(Json)
}

#[cfg(not(feature = "prover"))]
#[tracing::instrument(level = "debug", skip_all)]
async fn show_spell_for_tx_hex(
    Path(txid): Path<String>,
    Json(payload): Json<ShowSpellRequest>,
) -> Result<Json<Spell>, StatusCode> {
    show_spell(&txid, &payload).map(Json)
}

#[tracing::instrument(level = "debug", skip_all)]
async fn prove_spell(
    State(prover): State<Arc<AsyncShared<Prover>>>,
    Json(payload): Json<ProveRequest>,
) -> Result<Json<[String; 2]>, StatusCode> {
    let result = prover
        .get()
        .await
        .prove_spell_tx(payload)
        .await
        .map(|[tx0, tx1]| [serialize_hex(&tx0), serialize_hex(&tx1)])
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(result))
}

#[cfg(not(feature = "prover"))]
fn bitcoind_client(rpc_url: String, rpc_user: String, rpc_password: String) -> Client {
    Client::new(
        &rpc_url,
        Auth::UserPass(rpc_user.clone(), rpc_password.clone()),
    )
    .expect("Should create RPC client")
}

#[cfg(not(feature = "prover"))]
fn get_spell(rpc: Arc<Client>, txid: &str) -> Result<Spell, StatusCode> {
    let txid = bitcoin::Txid::from_str(txid).map_err(|_| StatusCode::BAD_REQUEST)?;

    match rpc.get_raw_transaction(&txid, None) {
        Ok(tx) => extract_spell(&tx),
        Err(e) => match e {
            bitcoincore_rpc::Error::JsonRpc(Rpc(rpc_error)) if rpc_error.code == -5 => {
                Err(StatusCode::NOT_FOUND)
            }
            _ => {
                tracing::warn!("Error: {:?}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        },
    }
}

#[cfg(not(feature = "prover"))]
fn show_spell(txid: &str, request: &ShowSpellRequest) -> Result<Spell, StatusCode> {
    let txid = bitcoin::Txid::from_str(txid).map_err(|_| StatusCode::BAD_REQUEST)?;
    let tx: bitcoin::Transaction =
        deserialize_hex(&request.tx_hex).map_err(|_| StatusCode::BAD_REQUEST)?;
    if tx.compute_txid() != txid {
        return Err(StatusCode::BAD_REQUEST);
    }
    extract_spell(&tx)
}

#[cfg(not(feature = "prover"))]
fn extract_spell(tx: &bitcoin::Transaction) -> Result<Spell, StatusCode> {
    match norm_spell(&tx) {
        None => Err(StatusCode::NO_CONTENT),
        Some(spell) => Ok(Spell::denormalized(&spell)),
    }
}
