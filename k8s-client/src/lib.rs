pub mod client;
pub mod error;

pub use client::K8sClient;
pub use error::{KubernetesError, Result};
