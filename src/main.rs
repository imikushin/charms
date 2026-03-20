#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    unsafe {
        // We don't want to be in mock mode accidentally.
        std::env::remove_var("MOCK");
    }
    charms::cli::run().await.map_err(|e| {
        tracing::error!("Error: {}", e);
        e
    })
}

#[cfg(target_arch = "wasm32")]
fn main() -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    rt.block_on(charms::cli::run()).map_err(|e| {
        eprintln!("Error: {}", e);
        e
    })
}
