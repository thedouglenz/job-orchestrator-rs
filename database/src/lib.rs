pub mod connection;
pub mod repositories;

// Always compile mocks module (needed for tests in other crates)
pub mod mocks;

pub use connection::{Database, DatabaseConfig};
pub use repositories::{
    PostgresDAGExecutionRepository, PostgresDAGTemplateRepository, PostgresImageRegistryRepository,
    PostgresJobExecutionHistoryRepository, PostgresJobQueueRepository,
};
