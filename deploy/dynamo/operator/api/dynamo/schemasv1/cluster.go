/*
 * SPDX-FileCopyrightText: Copyright (c) 2025 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

package schemasv1

import (
	"github.com/ai-dynamo/dynamo/deploy/dynamo/operator/api/dynamo/modelschemas"
)

type ClusterSchema struct {
	ResourceSchema
	Creator     *UserSchema `json:"creator"`
	Description string      `json:"description"`
}

type ClusterListSchema struct {
	BaseListSchema
	Items []*ClusterSchema `json:"items"`
}

type ClusterFullSchema struct {
	ClusterSchema
	Organization    *OrganizationSchema                `json:"organization"`
	KubeConfig      *string                            `json:"kube_config"`
	Config          **modelschemas.ClusterConfigSchema `json:"config"`
	GrafanaRootPath string                             `json:"grafana_root_path"`
}

type UpdateClusterSchema struct {
	Description *string                            `json:"description"`
	KubeConfig  *string                            `json:"kube_config"`
	Config      **modelschemas.ClusterConfigSchema `json:"config"`
}

type CreateClusterSchema struct {
	Description string                            `json:"description"`
	KubeConfig  string                            `json:"kube_config"`
	Config      *modelschemas.ClusterConfigSchema `json:"config"`
	Name        string                            `json:"name"`
}
