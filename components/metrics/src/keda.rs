use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use crate::proto::externalscaler::{GetMetricSpecResponse, GetMetricsRequest, GetMetricsResponse, IsActiveResponse, MetricSpec, MetricValue, ScaledObjectRef};
use crate::proto::externalscaler::external_scaler_server::{ExternalScaler, ExternalScalerServer};
use metrics::PrometheusMetricsCollector;

// KEDA External Scaler
pub struct KedaScaler {
    metrics_collector: Arc<Mutex<PrometheusMetricsCollector>>,
}

impl KedaScaler {
    pub fn new(metrics_collector: Arc<Mutex<PrometheusMetricsCollector>>) -> Self {
        Self { metrics_collector }
    }

    pub fn into_server(self) -> ExternalScalerServer<KedaScaler> {
        ExternalScalerServer::new(self)
    }

    async fn get_current_metrics(&self) -> Result<f64, Status> {
        // Retrieve the llm_load_avg metric from the metrics_collector
        let collector = self.metrics_collector.lock().await;
        // Placeholder for actual metric retrieval logic
        Ok(0.0) // Replace with actual logic to get llm_load_avg
    }
}

// Implement the KEDA ExternalScaler interface
#[tonic::async_trait]
impl ExternalScaler for KedaScaler {
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
        Ok(Response::new(IsActiveResponse { result: true }))
    }

    type StreamIsActiveStream = tokio_stream::wrappers::ReceiverStream<Result<IsActiveResponse, Status>>;

    async fn stream_is_active(
        &self,
        request: Request<ScaledObjectRef>,
    ) -> Result<Response<Self::StreamIsActiveStream>, Status> {
        let scaled_obj = request.get_ref();
        tracing::debug!(
            "StreamIsActive called for {}/{} but not implemented",
            scaled_obj.namespace,
            scaled_obj.name
        );
        Err(Status::unimplemented(
            "StreamIsActive is not implemented for this external scaler. Use pull-based scaling instead."
        ))
    }

    async fn get_metric_spec(
        &self,
        request: Request<ScaledObjectRef>,
    ) -> Result<Response<GetMetricSpecResponse>, Status> {
        let scaled_obj = request.get_ref();
        let metadata = &scaled_obj.scaler_metadata;
        let load_avg_threshold = metadata
            .get("loadAvgThreshold")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.7);
        tracing::debug!(
            "Providing metric specs for scaled object: {}/{} with threshold: {}",
            scaled_obj.namespace,
            scaled_obj.name,
            load_avg_threshold
        );
        let metrics = vec![
            MetricSpec {
                metric_name: "llm_load_avg".to_string(),
                target_size: 0,
                target_size_float: load_avg_threshold,
            },
        ];
        Ok(Response::new(GetMetricSpecResponse {
            metric_specs: metrics,
        }))
    }

    async fn get_metrics(
        &self,
        request: Request<GetMetricsRequest>,
    ) -> Result<Response<GetMetricsResponse>, Status> {
        let request = request.get_ref();
        let metric_name_str = &request.metric_name;
        let load_avg = self.get_current_metrics().await?;
        let metric_values = match metric_name_str {
            "llm_load_avg" => {
                vec![MetricValue {
                    metric_name: "llm_load_avg".to_string(),
                    metric_value: 0,
                    metric_value_float: load_avg,
                }]
            }
            _ => {
                return Err(Status::invalid_argument(format!(
                    "Unknown metric: {}",
                    metric_name_str
                )))
            }
        };
        Ok(Response::new(GetMetricsResponse { metric_values }))
    }
}
