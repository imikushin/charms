use sp1_prover::components::CpuProverComponents;
use std::sync::OnceLock;
use tokio::sync::OnceCell;

pub(crate) mod logger;
pub mod pool;
#[cfg(feature = "prover")]
pub(crate) mod sp1;

pub type BoxedSP1Prover = Box<dyn sp1_sdk::Prover<CpuProverComponents>>;

/// Create a string representation of the index `i` in the format `$xxxx`.
pub fn str_index(i: &usize) -> String {
    format!("${:04}", i)
}

pub struct AsyncShared<T> {
    pub create: fn() -> T,
    pub instance: OnceCell<T>,
}

impl<T> AsyncShared<T> {
    pub fn new(create: fn() -> T) -> Self {
        Self {
            create,
            instance: OnceCell::new(),
        }
    }

    pub async fn get(&self) -> &T {
        let create = self.create;
        self.instance.get_or_init(|| async { create() }).await
    }
}

pub struct Shared<T> {
    pub create: fn() -> T,
    pub instance: OnceLock<T>,
}

impl<T> Shared<T> {
    pub fn new(create: fn() -> T) -> Self {
        Self {
            create,
            instance: OnceLock::new(),
        }
    }

    pub fn get(&self) -> &T {
        self.instance.get_or_init(|| (self.create)())
    }
}
