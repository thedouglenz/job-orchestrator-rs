use async_trait::async_trait;
use std::collections::HashMap;
use types::JobOutputs;

// Use JobQueueRepository from executor-core to avoid duplication
pub use executor_core::database::JobQueueRepository;

/// DAG execution repository trait
#[async_trait]
pub trait DAGExecutionRepository: Send + Sync {
    async fn get_execution(
        &self,
        dag_execution_id: &str,
        include_nodes: bool,
    ) -> Result<DAGExecution, String>;
    async fn get_node_execution(&self, node_execution_id: &str)
        -> Result<DAGNodeExecution, String>;
    async fn get_node_execution_by_dag_and_node(
        &self,
        dag_execution_id: &str,
        node_id: &str,
    ) -> Result<DAGNodeExecution, String>;
    async fn update_execution_status(
        &self,
        dag_execution_id: &str,
        status: DAGStatus,
        error_message: Option<&str>,
    ) -> Result<(), String>;
    async fn update_node_execution_status(
        &self,
        node_execution_id: &str,
        status: DAGNodeStatus,
        data: Option<NodeExecutionUpdate>,
    ) -> Result<(), String>;
    async fn get_execution_stats(
        &self,
        dag_execution_id: &str,
    ) -> Result<DAGExecutionStats, String>;
}

/// DAG template repository trait
#[async_trait]
pub trait DAGTemplateRepository: Send + Sync {
    async fn get_node(&self, node_id: &str) -> Result<DAGNode, String>;
    async fn get_child_nodes(&self, node_id: &str) -> Result<Vec<DAGNode>, String>;
}

#[derive(Debug, Clone)]
pub struct DAGExecution {
    pub id: String,
    pub dag_template_id: String,
    pub topic: String,
    pub status: DAGStatus,
    pub priority: i32,
    pub execution_payload: HashMap<String, serde_json::Value>,
    pub created_by: String,
}

#[derive(Debug, Clone)]
pub struct DAGNodeExecution {
    pub id: String,
    pub dag_execution_id: String,
    pub dag_node_id: String,
    pub job_id: Option<String>,
    pub job_topic: Option<String>,
    pub status: DAGNodeStatus,
    pub inputs: Option<types::io::JobInputs>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DAGStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DAGNodeStatus {
    Pending,
    Waiting,
    Ready,
    Running,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone)]
pub struct DAGNode {
    pub id: String,
    pub node_name: String,
    pub image_id: String,
    pub command: Option<Vec<String>>,
    pub args: Option<Vec<String>>,
    pub environment_variables: Option<HashMap<String, String>>,
    pub resource_overrides: Option<types::image::PartialResourceRequirements>,
    pub max_retries: i32,
    pub input_mapping: Option<HashMap<String, InputMappingSpec>>,
    /// Optional UUID identifying a group of sibling nodes that should be executed together
    pub sibling_group_id: Option<String>,
    /// Execution mode for this node's siblings (defaults to Parallel)
    pub execution_mode: types::ExecutionMode,
    /// Maximum number of siblings to execute in parallel (only used for Limited mode)
    pub max_parallel_siblings: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct InputMappingSpec {
    pub source: InputSource,
    pub output_key: Option<String>,
    pub payload_key: Option<String>,
    pub value: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputSource {
    Parent,
    Payload,
    Constant,
}

#[derive(Debug, Clone)]
pub struct NodeExecutionUpdate {
    pub outputs: Option<JobOutputs>,
    pub error_message: Option<String>,
    pub job_id: Option<String>,
    pub job_topic: Option<String>,
    pub inputs: Option<types::io::JobInputs>,
}

#[derive(Debug, Clone)]
pub struct DAGExecutionStats {
    pub dag_execution_id: String,
    pub total_nodes: i32,
    pub completed_nodes: i32,
    pub failed_nodes: i32,
    pub running_nodes: i32,
    pub pending_nodes: i32,
    pub waiting_nodes: i32,
    pub skipped_nodes: i32,
}
