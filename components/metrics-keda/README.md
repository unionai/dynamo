# KEDA External Scaler for Dynamo LLM Workers

This component provides a KEDA External Scaler implementation that allows Kubernetes to autoscale LLM worker pods based on their utilization metrics.

## Overview

The `metrics-keda` component implements the [KEDA External Scaler](https://keda.sh/docs/2.12/concepts/external-scalers/) interface, which enables Kubernetes Event-driven Autoscaling of LLM workers based on:

- LLM worker load average
- KV cache block utilization
- Request slot usage

## Usage

```bash
# Run with default settings
metrics-keda

# Run with custom settings
metrics-keda \
  --component-name llm-worker \
  --endpoint-name kv-router \
  --host 0.0.0.0 \
  --port 9090 \
  --threshold 0.7 \
  --check-interval 5 \
  --cache-ttl 5
```

## Command-line options

| Option | Description | Default |
|--------|-------------|---------|
| `--component-name` | Name of the component to monitor | `llm-worker` |
| `--endpoint-name` | Name of the endpoint to monitor | `kv-router` |
| `--host` | Host address to listen on | `0.0.0.0` |
| `--port` | Port to listen on | `9090` |
| `--threshold` | Default load threshold (0.0-1.0) | `0.7` |
| `--check-interval` | Metrics check interval in seconds | `5` |
| `--cache-ttl` | Maximum age of cached metrics in seconds | `5` |

## Deployment

### 1. Deploy the External Scaler

Deploy the external scaler as a service in your Kubernetes cluster:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: dynamo-metrics-keda
  namespace: dynamo-system
spec:
  replicas: 1
  selector:
    matchLabels:
      app: dynamo-metrics-keda
  template:
    metadata:
      labels:
        app: dynamo-metrics-keda
    spec:
      containers:
      - name: metrics-keda
        image: dynamo/metrics-keda:latest
        args:
        - --component-name=llm-worker
        - --endpoint-name=kv-router
        - --threshold=0.7
        - --check-interval=5
        - --cache-ttl=5
        ports:
        - containerPort: 9090
---
apiVersion: v1
kind: Service
metadata:
  name: dynamo-metrics-keda
  namespace: dynamo-system
spec:
  selector:
    app: dynamo-metrics-keda
  ports:
  - port: 9090
    targetPort: 9090
```

### 2. Create a ScaledObject

Create a KEDA ScaledObject to define the scaling behavior:

```yaml
apiVersion: keda.sh/v1alpha1
kind: ScaledObject
metadata:
  name: dynamo-llm-worker-scaler
  namespace: dynamo-system
spec:
  scaleTargetRef:
    name: llm-worker-deployment
  pollingInterval: 15
  cooldownPeriod: 30
  minReplicaCount: 1
  maxReplicaCount: 10
  triggers:
  - type: external
    metadata:
      scalerAddress: dynamo-metrics-keda:9090
      component: llm-worker
      endpoint: kv-router
      threshold: "0.7"
```

## Available Metrics

The external scaler exposes the following metrics:

| Metric | Description |
|--------|-------------|
| `llm_load_avg` | Average load across all LLM workers |
| `llm_kv_blocks_usage_percent` | Percentage of KV cache blocks in use |

## Architecture

The KEDA External Scaler integrates with Dynamo's existing metrics collection system:

1. The scaler collects metrics from worker pods using the same approach as the metrics component
2. It processes the metrics to determine the overall load on the system
3. When queried by KEDA, it provides scaling decisions based on configured thresholds
4. It also supports push-based scaling by streaming activation events when thresholds are exceeded

### Performance Optimization

To improve performance and reduce load on the worker pods, the metrics-keda component:

1. Collects metrics asynchronously in a background task at a configurable interval
2. Caches the collected metrics for a configurable TTL period
3. Returns cached metrics for KEDA queries when available, avoiding redundant API calls
4. Only fetches fresh metrics when the cache is stale or empty

This caching approach significantly reduces the load on the worker pods and NATS infrastructure, especially when there are multiple KEDA scaling events occurring in quick succession.

This enables Kubernetes to autoscale LLM worker pods based on real-time utilization metrics while maintaining system performance.
