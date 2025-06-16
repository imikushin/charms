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
    routing::{get, post},
    Json, Router,
};
use axum_macros::debug_handler;
#[cfg(not(feature = "prover"))]
use charms_client::tx::Tx;
#[cfg(not(feature = "prover"))]
use charms_data::TxId;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use tower_http::cors::{Any, CorsLayer};

pub struct Server {
    pub config: ServerConfig,
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
        let prover = Arc::new(prover);
        Self { config, prover }
    }

    pub async fn serve(&self) -> Result<()> {
        let ServerConfig { ip, port, .. } = &self.config;

        // Build router with CORS middleware
        let app = Router::new();
        #[cfg(not(feature = "prover"))]
        let app = app.route("/spells/{txid}", put(show_spell_for_tx_hex));
        let app = app
            .route("/spells/prove", post(prove_spell))
            .with_state(self.prover.clone())
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

#[debug_handler]
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
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(result))
}

#[cfg(not(feature = "prover"))]
fn show_spell(txid: &str, request: &ShowSpellRequest) -> Result<Spell, StatusCode> {
    let txid = TxId::from_str(txid).map_err(|_| StatusCode::BAD_REQUEST)?;
    let tx = Tx::from_hex(&request.tx_hex).map_err(|_| StatusCode::BAD_REQUEST)?;
    match &tx {
        Tx::Bitcoin(bitcoin_tx) => {
            if bitcoin_tx.tx_id() != txid {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        Tx::Cardano(cardano_tx) => {
            if cardano_tx.tx_id() != txid {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    }

    extract_spell(&tx)
}

#[cfg(not(feature = "prover"))]
fn extract_spell(tx: &Tx) -> Result<Spell, StatusCode> {
    match norm_spell(tx) {
        None => Err(StatusCode::NO_CONTENT),
        Some(spell) => Ok(Spell::denormalized(&spell)),
    }
}
