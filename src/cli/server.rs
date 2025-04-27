#[cfg(not(feature = "prover"))]
use crate::spell::Spell;
#[cfg(not(feature = "prover"))]
use crate::tx::norm_spell;
use crate::{
    cli::ServerConfig,
    spell::{ProveRequest, ProveSpellTx, Prover},
    utils::AsyncShared,
};
use anyhow::Result;
#[cfg(not(feature = "prover"))]
use axum::{extract::Path, routing::put};
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get /* , post */},
    Json, Router,
};
#[cfg(not(feature = "prover"))]
use bitcoin::consensus::encode::deserialize_hex;
use bitcoin::consensus::encode::serialize_hex;
#[cfg(not(feature = "prover"))]
use bitcoincore_rpc::{jsonrpc::Error::Rpc, Auth, Client, RpcApi};
#[cfg(not(feature = "prover"))]
use charms_client::bitcoin_tx::BitcoinTx;
#[cfg(not(feature = "prover"))]
use charms_client::tx::Tx;
use serde::{Deserialize, Serialize};
#[cfg(not(feature = "prover"))]
use std::str::FromStr;
use std::{sync::Arc, time::Duration};
use tower_http::cors::{Any, CorsLayer};

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

/// Creates a permissive CORS configuration layer for the API server.
///
/// This configuration:
/// - Allows requests from any origin
/// - Allows all HTTP methods
/// - Allows all headers to be sent
/// - Exposes all headers to the client
/// - Sets a max age of 1 hour (3600 seconds) for preflight requests
fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers(Any)
        .max_age(Duration::from_secs(3600))
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
            // .route("/spells/{txid}", get(show_spell_by_txid))
            // .with_state(self.rpc.clone())
            .route("/spells/{txid}", put(show_spell_for_tx_hex));
        let app = app
            // .route("/spells/prove", post(prove_spell))
            // .with_state(self.prover.clone())
            .route("/ready", get(|| async { "OK" }))
            .layer(cors_layer());

        // Run server
        let addr = format!("{}:{}", ip, port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        tracing::info!("Server running on {}", &addr);

        axum::serve(listener, app).await?;
        Ok(())
    }
}

// Handlers

// #[cfg(not(feature = "prover"))]
// #[tracing::instrument(level = "debug", skip_all)]
// async fn show_spell_by_txid(
//     State(rpc): State<Arc<Client>>,
//     Path(txid): Path<String>,
// ) -> Result<Json<Spell>, StatusCode> {
//     get_spell(rpc, &txid).map(Json)
// }

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
) -> Result<Json<Vec<String>>, StatusCode> {
    let result = prover
        .get()
        .await
        .prove_spell_tx(payload)
        .await
        .map(|txs| txs.iter().map(serialize_hex).collect::<Vec<_>>())
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

// #[cfg(not(feature = "prover"))]
// fn get_spell(rpc: Arc<Client>, txid: &str) -> Result<Spell, StatusCode> {
//     let txid = bitcoin::Txid::from_str(txid).map_err(|_| StatusCode::BAD_REQUEST)?;
//
//     match rpc.get_raw_transaction(&txid, None) {
//         Ok(tx) => extract_spell(&Tx::Bitcoin(BitcoinTx(tx))),
//         Err(e) => match e {
//             bitcoincore_rpc::Error::JsonRpc(Rpc(rpc_error)) if rpc_error.code == -5 => {
//                 Err(StatusCode::NOT_FOUND)
//             }
//             _ => {
//                 tracing::warn!("Error: {:?}", e);
//                 Err(StatusCode::INTERNAL_SERVER_ERROR)
//             }
//         },
//     }
// }

#[cfg(not(feature = "prover"))]
fn show_spell(txid: &str, request: &ShowSpellRequest) -> Result<Spell, StatusCode> {
    let txid = bitcoin::Txid::from_str(txid).map_err(|_| StatusCode::BAD_REQUEST)?;
    let tx: bitcoin::Transaction =
        deserialize_hex(&request.tx_hex).map_err(|_| StatusCode::BAD_REQUEST)?;
    if tx.compute_txid() != txid {
        return Err(StatusCode::BAD_REQUEST);
    }
    extract_spell(&Tx::Bitcoin(BitcoinTx(tx)))
}

#[cfg(not(feature = "prover"))]
fn extract_spell(tx: &Tx) -> Result<Spell, StatusCode> {
    match norm_spell(tx) {
        None => Err(StatusCode::NO_CONTENT),
        Some(spell) => Ok(Spell::denormalized(&spell)),
    }
}
