# SPDX-FileCopyrightText: Copyright (c) 2025 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
# SPDX-License-Identifier: Apache-2.0
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
# http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

import subprocess
from datetime import timedelta
from pathlib import Path

from dynamo import sdk
from dynamo.sdk import depends, service
from dynamo.sdk.lib.config import ServiceConfig
from dynamo.sdk.lib.image import DYNAMO_IMAGE
from pydantic import BaseModel

from components.worker import VllmWorker


def get_metrics_binary_path():
    sdk_path = Path(sdk.__file__)
    binary_path = sdk_path.parent / "cli/bin/metrics"
    if not binary_path.exists():
        return "metrics"
    else:
        return str(binary_path)


class MetricsConfig(BaseModel):
    host: str = "0.0.0.0"
    port: int = 9091
    poll_interval: timedelta = timedelta(seconds=1)


@service(
    resources={"cpu": "1", "memory": "2Gi"},
    workers=1,
    image=DYNAMO_IMAGE,
)
class Metrics:
    worker = depends(VllmWorker)

    def __init__(self):
        config = ServiceConfig.get_instance()
        metrics_config = MetricsConfig.model_validate(config.get("Metrics", {}))
        worker_ns, worker_name = VllmWorker.dynamo_address()

        print("Starting Metrics server")
        metrics_binary = get_metrics_binary_path()
        process = subprocess.Popen(
            [
                metrics_binary,
                "--host",
                metrics_config.host,
                "--port",
                str(metrics_config.port),
                "--poll-interval",
                str(int(metrics_config.poll_interval.total_seconds())),
                "--namespace",
                worker_ns,
                "--component",
                worker_name,
                "--endpoint",
                "load_metrics",
            ],
            stdout=None,
            stderr=None,
        )
        try:
            process.wait()
        except KeyboardInterrupt:
            process.terminate()
            process.wait()
