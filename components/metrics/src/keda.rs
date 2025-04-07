use tonic::{Request, Response, Status};
use crate::PrometheusMetrics;
use std::sync::Arc;

// Include the proto module directly in keda.rs
pub mod proto {
    pub mod externalscaler {
        tonic::include_proto!("externalscaler");
    }
}

use self::proto::externalscaler::{GetMetricSpecResponse, GetMetricsRequest, GetMetricsResponse, IsActiveResponse, MetricSpec, MetricValue, ScaledObjectRef};
use self::proto::externalscaler::external_scaler_server::{ExternalScaler, ExternalScalerServer};

// KEDA External Scaler
pub struct KedaScaler {
    metrics: Arc<PrometheusMetrics>,
    component_name: String,
    endpoint_name: String,
}

impl KedaScaler {
    pub fn new(metrics: Arc<PrometheusMetrics>, component_name: String, endpoint_name: String) -> Self {
        Self {
            metrics,
            component_name,
            endpoint_name,
        }
    }

    pub fn into_server(self) -> ExternalScalerServer<KedaScaler> {
        ExternalScalerServer::new(self)
    }

    async fn get_current_metrics(&self) -> Result<f64, Status> {
        // Directly access the load_avg metric using the component and endpoint names
        let value = self.metrics.load_avg
            .with_label_values(&[&self.component_name, &self.endpoint_name])
            .get();

        tracing::debug!("Current load_avg metric: {}", value);
        Ok(value)
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
        let metric_values = match metric_name_str.as_str() {
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
