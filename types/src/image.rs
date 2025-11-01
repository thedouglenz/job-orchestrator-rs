use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceRequirements {
    pub cpu: String,
    pub memory: String,
}

impl ResourceRequirements {
    pub fn new(cpu: impl Into<String>, memory: impl Into<String>) -> Self {
        Self {
            cpu: cpu.into(),
            memory: memory.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialResourceRequirements {
    pub cpu: Option<String>,
    pub memory: Option<String>,
}
