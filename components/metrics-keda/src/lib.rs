// SPDX-FileCopyrightText: Copyright (c) 2025 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Library for implementing a KEDA external scaler for LLM worker metrics.
//!
//! This library provides a GRPC service that implements the KEDA external scaler
//! interface, allowing Kubernetes to scale LLM workers based on their load.

pub mod externalscaler {
    tonic::include_proto!("externalscaler");
}

use dynamo_llm::kv_router::protocols::ForwardPassMetrics;
use dynamo_llm::kv_router::scheduler::Endpoint;
use dynamo_llm::kv_router::scoring::ProcessedEndpoints;
use dynamo_runtime::{
    component::Component, error, service::EndpointInfo, utils::Duration, Result,
};
use externalscaler::external_scaler_server::{ExternalScaler, ExternalScalerServer};
use externalscaler::{
    GetMetricSpecResponse, GetMetricsRequest, GetMetricsResponse, IsActiveResponse, MetricSpec,
    MetricValue, ScaledObjectRef,
};
use metrics::{collect_endpoints, extract_metrics, postprocess_metrics, LLMWorkerLoadCapacityConfig};
use std::sync::{Arc, Mutex};
use std::time::{Duration as StdDuration, Instant};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream};
use tonic::{transport::Server, Request, Response, Status};

/// ScaledObject metadata key for load average threshold
const LOAD_AVG_THRESHOLD_KEY: &str = "loadAvgThreshold";

/// Default load average threshold if not specified
const DEFAULT_LOAD_AVG_THRESHOLD: f64 = 0.7;

/// Supported metrics for scaling
#[derive(Debug, Clone, PartialEq)]
enum MetricName {
    LoadAvg,
}

impl MetricName {
    /// Convert to string representation used in KEDA
    fn as_str(&self) -> &'static str {
        match self {
            Self::LoadAvg => "llm_load_avg",
        }
    }

    /// Parse from string, returns None if unsupported
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "llm_load_avg" => Some(Self::LoadAvg),
            _ => None,
        }
    }
}

/// Latest snapshot of collected metrics
struct MetricsSnapshot {
    processed: ProcessedEndpoints,
}

/// KEDA External Scaler for LLM worker metrics
pub struct LLMMetricsScaler {
    component: Component,
    config: LLMWorkerLoadCapacityConfig,
    // Thresholds configurable via KEDA ScaledObject metadata
    pub load_threshold: f64,
    // Latest metrics snapshot
    metrics_snapshot: Arc<Mutex<Option<MetricsSnapshot>>>,
}

impl LLMMetricsScaler {
    /// Create a new LLMMetricsScaler
    pub fn new(component: Component, config: LLMWorkerLoadCapacityConfig) -> Self {
        Self {
            component,
            config,
            load_threshold: 0.7, // Default threshold
            metrics_snapshot: Arc::new(Mutex::new(None)),
        }
    }

    /// Set the cache TTL (in seconds)
    pub fn with_cache_ttl(self, _ttl_seconds: u64) -> Self {
        // This method is kept for API compatibility but no longer does anything
        // since we no longer use TTL-based caching
        self
    }

    /// Create a new tonic server with this scaler
    pub fn into_server(
        self,
    ) -> externalscaler::external_scaler_server::ExternalScalerServer<LLMMetricsScaler> {
        ExternalScalerServer::new(self)
    }

    /// Start a background task that periodically collects metrics
    pub fn start_metrics_monitor(&self, check_interval: Duration) {
        let component = self.component.clone();
        let config = self.config.clone();
        let metrics_snapshot = self.metrics_snapshot.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(check_interval);
            loop {
                interval.tick().await;

                // Collect metrics
                match collect_worker_metrics(&component, &config).await {
                    Ok(processed) => {
                        // Update metrics snapshot
                        {
                            let mut snapshot = metrics_snapshot.lock().unwrap();
                            *snapshot = Some(MetricsSnapshot {
                                processed: processed.clone(),
                            });
                        }

                        // Log current metrics
                        tracing::debug!(
                            "Updated metrics snapshot: load_avg={}, endpoints={}",
                            processed.load_avg,
                            processed.endpoints.len()
                        );
                    }
                    Err(e) => {
                        tracing::warn!("Failed to collect worker metrics: {}", e);
                        // Continue with the next iteration - we'll keep using the last successful snapshot
                    }
                }
            }
        });
    }

    /// Get current metrics snapshot or return default metrics if none exists
    async fn get_current_metrics(&self) -> Result<ProcessedEndpoints, Status> {
        // Try to get metrics from snapshot
        {
            let snapshot = self.metrics_snapshot.lock().unwrap();
            if let Some(snapshot) = &*snapshot {
                return Ok(snapshot.processed.clone());
            }
        }

        // No snapshot available, return default metrics (empty with 0 values)
        tracing::debug!("No metrics snapshot available, returning default metrics");
        Ok(ProcessedEndpoints::default()) // This sets empty endpoints, load_avg=0, load_std=0
    }
}

/// Helper function to extract threshold from ScaledObjectRef metadata
fn get_threshold(metadata: &std::collections::HashMap<String, String>) -> f64 {
    metadata
        .get("threshold")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.7)
}

/// Helper function to extract metric name from ScaledObjectRef metadata
fn get_metric_name(metadata: &std::collections::HashMap<String, String>) -> MetricName {
    metadata
        .get("metricName")
        .and_then(|s| MetricName::from_str(s))
        .unwrap_or(MetricName::LoadAvg)
}

/// Helper function to collect worker metrics
async fn collect_worker_metrics(
    component: &Component,
    config: &LLMWorkerLoadCapacityConfig,
) -> Result<ProcessedEndpoints> {
    // Use the same endpoint path/subject as the metrics component
    let endpoint = component.endpoint("kv-metrics");
    let service_subject = endpoint.subject();

    // Collect endpoints
    let endpoints = collect_endpoints(component, &service_subject, Duration::from_millis(300)).await?;

    // Extract and process metrics
    let metrics = extract_metrics(&endpoints);
    let processed = postprocess_metrics(&metrics, &endpoints);

    Ok(processed)
}

/// Implement the KEDA ExternalScaler interface
#[tonic::async_trait]
impl ExternalScaler for LLMMetricsScaler {
    /// Check if scaling is needed
    /// Always returns true to prevent scaling to zero
    async fn is_active(
        &self,
        request: Request<ScaledObjectRef>,
    ) -> Result<Response<IsActiveResponse>, Status> {
        let scaled_obj = request.get_ref();

        tracing::debug!(
            "IsActive check for {}/{} - always returning true to prevent scaling to zero",
            scaled_obj.namespace,
            scaled_obj.name
        );

        // Always return true to prevent scaling to zero
        Ok(Response::new(IsActiveResponse { result: true }))
    }

    /// Stream active status changes to KEDA
    type StreamIsActiveStream = ReceiverStream<Result<IsActiveResponse, Status>>;

    async fn stream_is_active(
        &self,
        request: Request<ScaledObjectRef>,
    ) -> Result<Response<Self::StreamIsActiveStream>, Status> {
        // Log the request details
        let scaled_obj = request.get_ref();
        tracing::debug!(
            "StreamIsActive called for {}/{} but not implemented",
            scaled_obj.namespace,
            scaled_obj.name
        );

        // This implementation doesn't support push-based scaling
        Err(Status::unimplemented(
            "StreamIsActive is not implemented for this external scaler. Use pull-based scaling instead."
        ))
    }

    /// Get metric specifications for the HPA
    async fn get_metric_spec(
        &self,
        request: Request<ScaledObjectRef>,
    ) -> Result<Response<GetMetricSpecResponse>, Status> {
        let scaled_obj = request.get_ref();
        let metadata = &scaled_obj.scaler_metadata;

        // Extract target threshold from metadata, use default if not specified
        let load_avg_threshold = metadata
            .get(LOAD_AVG_THRESHOLD_KEY)
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(DEFAULT_LOAD_AVG_THRESHOLD);

        tracing::debug!(
            "Providing metric specs for scaled object: {}/{} with threshold: {}",
            scaled_obj.namespace,
            scaled_obj.name,
            load_avg_threshold
        );

        // Create metric specs with the custom threshold
        // Note: target_size is deprecated in KEDA but we must set it in the Rust struct
        let metrics = vec![
            MetricSpec {
                metric_name: MetricName::LoadAvg.as_str().to_string(),
                target_size: 0,  // Deprecated in KEDA but required in the Rust struct
                target_size_float: load_avg_threshold,
            },
        ];

        Ok(Response::new(GetMetricSpecResponse {
            metric_specs: metrics,
        }))
    }

    /// Get current metric values
    async fn get_metrics(
        &self,
        request: Request<GetMetricsRequest>,
    ) -> Result<Response<GetMetricsResponse>, Status> {
        let request = request.get_ref();
        let metric_name_str = &request.metric_name;

        // Parse metric name from request
        let metric_name = match MetricName::from_str(metric_name_str) {
            Some(name) => name,
            None => {
                return Err(Status::invalid_argument(format!(
                    "Unknown metric: {}",
                    metric_name_str
                )))
            }
        };

        // Get metrics from snapshot or return default if none exists
        let processed = self.get_current_metrics().await?;

        // Return the appropriate metric based on the request
        // Note: metric_value is deprecated in KEDA but we must set it in the Rust struct
        let metric_values = match metric_name {
            MetricName::LoadAvg => {
                vec![MetricValue {
                    metric_name: MetricName::LoadAvg.as_str().to_string(),
                    metric_value: 0,  // Deprecated in KEDA but required in the Rust struct
                    metric_value_float: processed.load_avg,
                }]
            }
        };

        Ok(Response::new(GetMetricsResponse { metric_values }))
    }
}
