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

use anyhow;
use clap::Parser;
use dynamo_runtime::{logging, utils::Duration, DistributedRuntime, Result, Runtime, Worker};
use metrics::LLMWorkerLoadCapacityConfig;
use metrics_keda::LLMMetricsScaler;
use std::net::SocketAddr;
use tonic::transport::Server;

/// Command line arguments for the KEDA metrics scaler
#[derive(Parser, Debug)]
#[clap(
    name = "metrics-keda",
    about = "KEDA External Scaler for LLM worker metrics"
)]
struct Args {
    /// Component name to monitor
    #[clap(long, default_value = "llm-worker")]
    component_name: String,

    /// Endpoint name to monitor
    #[clap(long, default_value = "kv-router")]
    endpoint_name: String,

    /// Host address to listen on
    #[clap(long, default_value = "0.0.0.0")]
    host: String,

    /// Port to listen on
    #[clap(long, default_value = "9090")]
    port: u16,

    /// Default load threshold (0.0-1.0)
    #[clap(long, default_value = "0.7")]
    threshold: f64,

    /// Metrics check interval in seconds
    #[clap(long, default_value = "5")]
    check_interval: u64,

    /// Cache TTL in seconds
    #[clap(long, default_value = "5")]
    cache_ttl: u64,
}

fn main() -> Result<()> {
    // Initialize logging
    logging::init();

    // Parse command line arguments
    let args = Args::parse();

    // Initialize the worker
    let worker = Worker::from_settings()?;
    worker.execute(|runtime| async {
        app(runtime, args).await
    })
}

async fn app(runtime: Runtime, args: Args) -> Result<()> {
    // Create distributed runtime
    let distributed = DistributedRuntime::from_settings(runtime).await?;

    // Create namespace and component
    let namespace = distributed.namespace("dynamo")?;
    let component = namespace.component("metrics")?;

    // Create metrics configuration
    let config = LLMWorkerLoadCapacityConfig {
        component_name: args.component_name,
        endpoint_name: args.endpoint_name,
    };

    // Create the KEDA scaler service with cache TTL
    let mut scaler = LLMMetricsScaler::new(component.clone(), config)
        .with_cache_ttl(args.cache_ttl);

    // Set load threshold from CLI arguments
    scaler.load_threshold = args.threshold;

    // Start background metrics monitor
    let check_interval = Duration::from_secs(args.check_interval);
    scaler.start_metrics_monitor(check_interval);

    // Create server address
    let addr = format!("{}:{}", args.host, args.port);
    let socket_addr: SocketAddr = addr.parse()?;

    // Start the gRPC server
    tracing::info!("Starting KEDA external scaler server on {}", socket_addr);
    tracing::info!("Using metrics cache TTL of {}s", args.cache_ttl);

    // Create the server and await completion
    Server::builder()
        .add_service(scaler.into_server())
        .serve(socket_addr)
        .await
        .map_err(|e| anyhow::anyhow!("Server error: {}", e))?;

    Ok(())
}
