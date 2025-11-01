use k8s_openapi::api::batch::v1::Job;
use k8s_openapi::api::core::v1::{
    Container, EnvVar, PodSpec, ResourceRequirements as K8sResourceRequirements,
    Volume, VolumeMount,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use kube::{
    api::{Api, DeleteParams, PostParams},
    Client, Config,
};
use std::collections::BTreeMap;
use tracing::{error, info, warn};

use crate::error::{KubernetesError, Result};
use types::{JobExecutionSpec, K8sJobCondition, K8sJobStatus};

/// Kubernetes client for managing job orchestration
pub struct K8sClient {
    client: Client,
    jobs_api: Api<Job>,
    pods_api: Api<k8s_openapi::api::core::v1::Pod>,
}

impl K8sClient {
    /// Create a new Kubernetes client
    pub async fn new() -> Result<Self> {
        let config = if std::env::var("K8S_IN_CLUSTER")
            .unwrap_or_default()
            .eq_ignore_ascii_case("true")
        {
            Config::incluster()
                .map_err(|e| KubernetesError::ConfigError(format!("Failed to load in-cluster config: {}", e)))?
        } else {
            Config::from_kubeconfig(&Default::default()).await
                .map_err(|e| KubernetesError::ConfigError(format!("Failed to load kubeconfig: {}", e)))?
        };

        let client = Client::try_from(config)
            .map_err(|e| KubernetesError::ConfigError(format!("Failed to create client: {}", e)))?;

        Ok(Self {
            jobs_api: Api::default_namespaced(client.clone()),
            pods_api: Api::default_namespaced(client.clone()),
            client,
        })
    }

    /// Create a Kubernetes job from a JobExecutionSpec
    pub async fn create_job(&self, spec: &JobExecutionSpec) -> Result<Job> {
        let output_collector_image = std::env::var("OUTPUT_COLLECTOR_IMAGE")
            .unwrap_or_else(|_| "us-docker.pkg.dev/sonic-glazing-475623-j4/job-orchestrator/output-collector:latest".to_string());
        
        let api_service_url = std::env::var("API_SERVICE_URL")
            .unwrap_or_else(|_| "http://job-orchestrator-api".to_string());

        let ttl_seconds = std::env::var("K8S_JOB_TTL_SECONDS")
            .unwrap_or_else(|_| "3600".to_string())
            .parse::<i32>()
            .unwrap_or(3600);

        let labels = {
            let mut map = BTreeMap::new();
            map.insert("app.kubernetes.io/managed-by".to_string(), "job-orchestrator".to_string());
            map.insert("job-orchestrator.io/job-id".to_string(), spec.job_id.clone());
            for (k, v) in &spec.labels {
                map.insert(k.clone(), v.clone());
            }
            map
        };

        let env_vars: Vec<EnvVar> = spec
            .environment_variables
            .iter()
            .map(|(name, value)| EnvVar {
                name: name.clone(),
                value: Some(value.clone()),
                ..Default::default()
            })
            .collect();

        let job_container = Container {
            name: "job".to_string(),
            image: Some(spec.image_path.clone()),
            command: spec.command.clone(),
            args: spec.args.clone(),
            env: Some(env_vars),
            resources: Some(K8sResourceRequirements {
                requests: Some({
                    let mut reqs = BTreeMap::new();
                    reqs.insert("cpu".to_string(), Quantity(spec.resources.cpu.clone()));
                    reqs.insert("memory".to_string(), Quantity(spec.resources.memory.clone()));
                    reqs
                }),
                limits: Some({
                    let mut lims = BTreeMap::new();
                    lims.insert("cpu".to_string(), Quantity(spec.resources.cpu.clone()));
                    lims.insert("memory".to_string(), Quantity(spec.resources.memory.clone()));
                    lims
                }),
                ..Default::default()
            }),
            volume_mounts: Some(vec![VolumeMount {
                name: "job-output".to_string(),
                mount_path: "/output".to_string(),
                ..Default::default()
            }]),
            ..Default::default()
        };

        let output_collector_container = Container {
            name: "output-collector".to_string(),
            image: Some(output_collector_image),
            image_pull_policy: Some("IfNotPresent".to_string()),
            env: Some(vec![
                EnvVar {
                    name: "JOB_ID".to_string(),
                    value: Some(spec.job_id.clone()),
                    ..Default::default()
                },
                EnvVar {
                    name: "API_URL".to_string(),
                    value: Some(api_service_url),
                    ..Default::default()
                },
                EnvVar {
                    name: "OUTPUT_FILE".to_string(),
                    value: Some("/output/result.json".to_string()),
                    ..Default::default()
                },
                EnvVar {
                    name: "MAX_WAIT_SECONDS".to_string(),
                    value: Some("300".to_string()),
                    ..Default::default()
                },
                EnvVar {
                    name: "POLL_INTERVAL_MS".to_string(),
                    value: Some("2000".to_string()),
                    ..Default::default()
                },
            ]),
            volume_mounts: Some(vec![VolumeMount {
                name: "job-output".to_string(),
                mount_path: "/output".to_string(),
                read_only: Some(true),
                ..Default::default()
            }]),
            resources: Some(K8sResourceRequirements {
                requests: Some({
                    let mut reqs = BTreeMap::new();
                    reqs.insert("cpu".to_string(), Quantity("10m".to_string()));
                    reqs.insert("memory".to_string(), Quantity("32Mi".to_string()));
                    reqs
                }),
                limits: Some({
                    let mut lims = BTreeMap::new();
                    lims.insert("cpu".to_string(), Quantity("50m".to_string()));
                    lims.insert("memory".to_string(), Quantity("64Mi".to_string()));
                    lims
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let job_manifest = Job {
            metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                name: Some(spec.job_name.clone()),
                namespace: Some(spec.namespace.clone()),
                labels: Some(labels.clone()),
                ..Default::default()
            }
            .into(),
            spec: Some(k8s_openapi::api::batch::v1::JobSpec {
                ttl_seconds_after_finished: Some(ttl_seconds),
                backoff_limit: Some(
                    std::env::var("K8S_JOB_BACKOFF_LIMIT")
                        .unwrap_or_else(|_| "0".to_string())
                        .parse::<i32>()
                        .unwrap_or(0),
                ),
                template: k8s_openapi::api::core::v1::PodTemplateSpec {
                    metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                        labels: Some(labels),
                        ..Default::default()
                    }
                    .into(),
                    spec: Some(PodSpec {
                        restart_policy: Some("Never".to_string()),
                        volumes: Some(vec![Volume {
                            name: "job-output".to_string(),
                            empty_dir: Some(Default::default()),
                            ..Default::default()
                        }]),
                        containers: vec![job_container, output_collector_container],
                        ..Default::default()
                    }),
                },
                ..Default::default()
            }),
            ..Default::default()
        };

        let jobs_api = Api::<Job>::namespaced(
            self.client.clone(),
            &spec.namespace,
        );

        match jobs_api.create(&PostParams::default(), &job_manifest).await {
            Ok(job) => {
                info!(
                    job_name = spec.job_name,
                    namespace = spec.namespace,
                    "Created Kubernetes job"
                );
                Ok(job)
            }
            Err(e) => {
                error!(error = ?e, spec = ?spec, "Failed to create Kubernetes job");
                Err(KubernetesError::CreateJobError(format!("{}", e)))
            }
        }
    }

    /// Get the status of a Kubernetes job
    pub async fn get_job_status(&self, job_name: &str, namespace: &str) -> Result<K8sJobStatus> {
        let jobs_api = Api::<Job>::namespaced(self.client.clone(), namespace);

        match jobs_api.get_status(job_name).await {
            Ok(job) => {
                let status = job.status.unwrap_or_default();
                Ok(K8sJobStatus {
                    active: status.active.unwrap_or(0),
                    succeeded: status.succeeded.unwrap_or(0),
                    failed: status.failed.unwrap_or(0),
                    start_time: status.start_time.map(|t| t.0.to_rfc3339()),
                    completion_time: status.completion_time.map(|t| t.0.to_rfc3339()),
                    conditions: status.conditions.map(|conds| {
                        conds
                            .into_iter()
                            .map(|c| K8sJobCondition {
                                condition_type: c.type_.clone(),
                                status: c.status.clone(),
                                last_probe_time: c.last_probe_time.map(|t| t.0.to_rfc3339()),
                                last_transition_time: c.last_transition_time.map(|t| t.0.to_rfc3339()),
                                reason: c.reason.clone(),
                                message: c.message.clone(),
                            })
                            .collect()
                    }),
                })
            }
            Err(e) => {
                error!(error = ?e, job_name, namespace, "Failed to get job status");
                Err(KubernetesError::GetJobStatusError(format!("{}", e)))
            }
        }
    }

    /// Delete a Kubernetes job
    pub async fn delete_job(&self, job_name: &str, namespace: &str) -> Result<()> {
        let jobs_api = Api::<Job>::namespaced(self.client.clone(), namespace);

        match jobs_api
            .delete(job_name, &DeleteParams::background())
            .await
        {
            Ok(_) => {
                info!(job_name, namespace, "Deleted Kubernetes job");
                Ok(())
            }
            Err(kube::Error::Api(e)) if e.code == 404 => {
                // Job already deleted, ignore
                Ok(())
            }
            Err(e) => {
                error!(error = ?e, job_name, namespace, "Failed to delete job");
                Err(KubernetesError::DeleteJobError(format!("{}", e)))
            }
        }
    }

    /// Get pod logs for a job
    pub async fn get_pod_logs(&self, job_name: &str, namespace: &str) -> Result<String> {
        let pods_api = Api::<k8s_openapi::api::core::v1::Pod>::namespaced(
            self.client.clone(),
            namespace,
        );

        // Find pods for this job
        let label_selector = format!("job-name={}", job_name);
        match pods_api.list(&kube::api::ListParams::default().labels(&label_selector)).await {
            Ok(pod_list) => {
                if let Some(pod) = pod_list.items.first() {
                    if let Some(pod_name) = pod.metadata.name.as_ref() {
                        // Get logs from the "job" container (not the output-collector)
                        match pods_api.logs(pod_name, &kube::api::LogParams {
                            container: Some("job".to_string()),
                            ..Default::default()
                        }).await {
                            Ok(logs) => Ok(logs),
                            Err(e) => {
                                warn!(error = ?e, job_name, namespace, "Failed to get pod logs");
                                Ok(String::new())
                            }
                        }
                    } else {
                        Ok(String::new())
                    }
                } else {
                    Ok(String::new())
                }
            }
            Err(e) => {
                warn!(error = ?e, job_name, namespace, "Failed to list pods");
                Ok(String::new())
            }
        }
    }
}
