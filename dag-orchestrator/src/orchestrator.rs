use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info, warn};
use async_trait::async_trait;

use crate::database::{
    DAGExecutionRepository, DAGTemplateRepository, DAGExecution, DAGNodeExecution,
    DAGStatus, DAGNodeStatus, DAGNode, NodeExecutionUpdate, DAGExecutionStats,
    JobQueueRepository,
};

use types::{QueuedJob, JobOutputs, JobRequest};

/// DAG Orchestrator handles DAG execution orchestration logic
pub struct DAGOrchestrator {
    dag_execution_repo: Arc<dyn DAGExecutionRepository>,
    dag_template_repo: Arc<dyn DAGTemplateRepository>,
    job_repo: Arc<dyn JobQueueRepository>,
}

impl DAGOrchestrator {
    pub fn new(
        dag_execution_repo: Arc<dyn DAGExecutionRepository>,
        dag_template_repo: Arc<dyn DAGTemplateRepository>,
        job_repo: Arc<dyn JobQueueRepository>,
    ) -> Self {
        Self {
            dag_execution_repo,
            dag_template_repo,
            job_repo,
        }
    }

    /// Check if a job is part of a DAG execution
    pub fn is_dag_job(&self, job: &QueuedJob) -> bool {
        job.dag_execution_id.is_some()
    }

    /// Handle successful completion of a DAG job
    pub async fn handle_job_completion(
        &self,
        job: &QueuedJob,
        outputs: &Option<JobOutputs>,
    ) -> Result<(), String> {
        let dag_execution_id = match &job.dag_execution_id {
            Some(id) => id,
            None => {
                warn!(job_id = job.id, "Job missing DAG context in handle_job_completion");
                return Ok(());
            }
        };

        let dag_node_execution_id = match &job.dag_node_execution_id {
            Some(id) => id,
            None => {
                warn!(job_id = job.id, "Job missing DAG node execution ID");
                return Ok(());
            }
        };

        info!(
            job_id = job.id,
            dag_execution_id = dag_execution_id,
            node_execution_id = dag_node_execution_id,
            "Handling DAG job completion"
        );

        // Update node execution status to completed
        self.dag_execution_repo
            .update_node_execution_status(
                dag_node_execution_id,
                DAGNodeStatus::Completed,
                Some(NodeExecutionUpdate {
                    outputs: outputs.clone(),
                    error_message: None,
                    job_id: None,
                    job_topic: None,
                    inputs: None,
                }),
            )
            .await?;

        // Get the node execution to find out which node this was
        let node_execution = self
            .dag_execution_repo
            .get_node_execution(dag_node_execution_id)
            .await?;

        let node = self
            .dag_template_repo
            .get_node(&node_execution.dag_node_id)
            .await?;

        // Get child nodes of this node
        let child_nodes = self
            .dag_template_repo
            .get_child_nodes(&node.id)
            .await?;

        if !child_nodes.is_empty() {
            info!(
                job_id = job.id,
                child_count = child_nodes.len(),
                "Job has child nodes, creating jobs for them"
            );

            // Get DAG execution
            let dag_execution = self
                .dag_execution_repo
                .get_execution(dag_execution_id, false)
                .await?;

            // Create jobs for each child node
            for child_node in child_nodes {
                self.create_job_for_node(
                    dag_execution_id,
                    &child_node.id,
                    &dag_execution.topic,
                    &dag_execution.execution_payload,
                    outputs,
                    dag_execution.priority,
                    &dag_execution.created_by,
                )
                .await?;
            }
        }

        // Check if DAG execution is complete
        self.check_and_update_dag_status(dag_execution_id).await?;

        Ok(())
    }

    /// Handle failure of a DAG job
    pub async fn handle_job_failure(
        &self,
        job: &QueuedJob,
        error_message: &str,
    ) -> Result<(), String> {
        let dag_execution_id = match &job.dag_execution_id {
            Some(id) => id,
            None => {
                warn!(job_id = job.id, "Job missing DAG context in handle_job_failure");
                return Ok(());
            }
        };

        let dag_node_execution_id = match &job.dag_node_execution_id {
            Some(id) => id,
            None => {
                warn!(job_id = job.id, "Job missing DAG node execution ID");
                return Ok(());
            }
        };

        info!(
            job_id = job.id,
            dag_execution_id = dag_execution_id,
            node_execution_id = dag_node_execution_id = dag_node_execution_id,
            "Handling DAG job failure"
        );

        // Update node execution status to failed
        self.dag_execution_repo
            .update_node_execution_status(
                dag_node_execution_id,
                DAGNodeStatus::Failed,
                Some(NodeExecutionUpdate {
                    outputs: None,
                    error_message: Some(error_message.to_string()),
                    job_id: None,
                    job_topic: None,
                    inputs: None,
                }),
            )
            .await?;

        // Mark DAG execution as failed
        self.dag_execution_repo
            .update_execution_status(
                dag_execution_id,
                DAGStatus::Failed,
                Some(&format!("Node {} failed: {}", job.job_name.as_ref().unwrap_or(&job.id), error_message)),
            )
            .await?;

        // Mark all descendant nodes as skipped
        let node_execution = self
            .dag_execution_repo
            .get_node_execution(dag_node_execution_id)
            .await?;

        self.mark_descendants_as_skipped(dag_execution_id, &node_execution.dag_node_id)
            .await?;

        Ok(())
    }

    async fn create_job_for_node(
        &self,
        dag_execution_id: &str,
        node_id: &str,
        topic: &str,
        execution_payload: &HashMap<String, serde_json::Value>,
        parent_outputs: &Option<JobOutputs>,
        priority: i32,
        created_by: &str,
    ) -> Result<(), String> {
        // Get node definition
        let node = self.dag_template_repo.get_node(node_id).await?;

        // Get node execution record
        let node_execution = self
            .dag_execution_repo
            .get_node_execution_by_dag_and_node(dag_execution_id, node_id)
            .await?;

        // Resolve inputs for this node
        let inputs = self.resolve_node_inputs(
            node.input_mapping.as_ref(),
            execution_payload,
            parent_outputs.as_ref(),
        );

        // Create job queue entry
        let job_id = self
            .job_repo
            .enqueue_job(&JobRequest {
                topic: topic.to_string(),
                image_id: node.image_id.clone(),
                job_name: Some(node.node_name.clone()),
                command: node.command.clone(),
                args: node.args.clone(),
                environment_variables: node.environment_variables.clone(),
                resource_overrides: node.resource_overrides.clone(),
                priority: Some(priority),
                max_retries: Some(node.max_retries),
                created_by: created_by.to_string(),
                inputs: Some(inputs.clone()),
                labels: None,
            })
            .await
            .map_err(|e| format!("Failed to enqueue job: {}", e))?;

        // Update node execution with job ID and inputs, mark as READY
        self.dag_execution_repo
            .update_node_execution_status(
                &node_execution.id,
                DAGNodeStatus::Ready,
                Some(NodeExecutionUpdate {
                    outputs: None,
                    error_message: None,
                    job_id: Some(job_id.clone()),
                    job_topic: Some(topic.to_string()),
                    inputs: Some(inputs),
                }),
            )
            .await?;

        // Note: Updating job's DAG context would need to be done through the job repository
        // This is a limitation that would need to be addressed in the actual implementation

        info!(
            node_id = node_id,
            job_id = job_id,
            dag_execution_id = dag_execution_id,
            "Created job for child node"
        );

        Ok(())
    }

    fn resolve_node_inputs(
        &self,
        input_mapping: Option<&HashMap<String, crate::database::InputMappingSpec>>,
        execution_payload: &HashMap<String, serde_json::Value>,
        parent_outputs: Option<&JobOutputs>,
    ) -> types::io::JobInputs {
        let mut resolved_inputs = types::io::JobInputs::new();

        if let Some(mapping) = input_mapping {
            for (input_name, spec) in mapping {
                match spec.source {
                    crate::database::InputSource::Payload => {
                        if let Some(key) = &spec.payload_key {
                            if let Some(value) = execution_payload.get(key) {
                                resolved_inputs.insert(input_name.clone(), value.clone());
                            }
                        }
                    }
                    crate::database::InputSource::Parent => {
                        if let Some(outputs) = parent_outputs {
                            if let Some(key) = &spec.output_key {
                                if let Some(output_value) = outputs.get(key) {
                                    // Extract the value from the JobIO structure if needed
                                    // For now, just use the value directly
                                    resolved_inputs.insert(input_name.clone(), output_value.clone());
                                }
                            }
                        }
                    }
                    crate::database::InputSource::Constant => {
                        if let Some(value) = &spec.value {
                            resolved_inputs.insert(input_name.clone(), value.clone());
                        }
                    }
                }
            }
        }

        resolved_inputs
    }

    async fn check_and_update_dag_status(&self, dag_execution_id: &str) -> Result<(), String> {
        let stats = self
            .dag_execution_repo
            .get_execution_stats(dag_execution_id)
            .await?;

        // Check if all nodes are in terminal states
        let total_terminal =
            stats.completed_nodes + stats.failed_nodes + stats.skipped_nodes;

        if total_terminal == stats.total_nodes {
            // DAG is done
            if stats.failed_nodes > 0 {
                self.dag_execution_repo
                    .update_execution_status(
                        dag_execution_id,
                        DAGStatus::Failed,
                        Some(&format!("{} node(s) failed", stats.failed_nodes)),
                    )
                    .await?;
                info!(dag_execution_id, "DAG execution completed with failures");
            } else {
                self.dag_execution_repo
                    .update_execution_status(dag_execution_id, DAGStatus::Completed, None)
                    .await?;
                info!(dag_execution_id, "DAG execution completed successfully");
            }
        } else if stats.running_nodes > 0
            || stats.pending_nodes > 0
            || stats.waiting_nodes > 0
        {
            // Update to running if not already
            let execution = self
                .dag_execution_repo
                .get_execution(dag_execution_id, false)
                .await?;

            if execution.status == DAGStatus::Pending {
                self.dag_execution_repo
                    .update_execution_status(dag_execution_id, DAGStatus::Running, None)
                    .await?;
                info!(dag_execution_id, "DAG execution started");
            }
        }

        Ok(())
    }

    async fn mark_descendants_as_skipped(
        &self,
        dag_execution_id: &str,
        node_id: &str,
    ) -> Result<(), String> {
        let child_nodes = self.dag_template_repo.get_child_nodes(node_id).await?;

        for child_node in child_nodes {
            let node_execution = self
                .dag_execution_repo
                .get_node_execution_by_dag_and_node(dag_execution_id, &child_node.id)
                .await?;

            // Only skip if not already in a terminal state
            if node_execution.status == DAGNodeStatus::Pending
                || node_execution.status == DAGNodeStatus::Waiting
            {
                self.dag_execution_repo
                    .update_node_execution_status(
                        &node_execution.id,
                        DAGNodeStatus::Skipped,
                        Some(NodeExecutionUpdate {
                            outputs: None,
                            error_message: Some("Parent node failed".to_string()),
                            job_id: None,
                            job_topic: None,
                            inputs: None,
                        }),
                    )
                    .await?;

                // Recursively mark descendants
                self.mark_descendants_as_skipped(dag_execution_id, &child_node.id)
                    .await?;
            }
        }

        Ok(())
    }
}
