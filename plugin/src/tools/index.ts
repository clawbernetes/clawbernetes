/**
 * Clawbernetes Tools Index
 *
 * Exports all tool factories for the OpenClaw plugin.
 */

export * from "./cluster.js";
export * from "./workload.js";
export * from "./observability.js";
export * from "./molt.js";
export * from "./auth.js";
export * from "./deploy.js";
export * from "./tenancy.js";
export * from "./secrets.js";
export * from "./pki.js";
export * from "./operations.js";
export * from "./service.js";
export * from "./storage.js";

import type { ClawbernnetesTool } from "../types.js";
import { createClusterTools } from "./cluster.js";
import { createWorkloadTools } from "./workload.js";
import { createObservabilityTools } from "./observability.js";
import { createMoltTools } from "./molt.js";
import { createAuthTools } from "./auth.js";
import { createDeployTools } from "./deploy.js";
import { createTenancyTools } from "./tenancy.js";
import { createSecretsTools } from "./secrets.js";
import { createPkiTools } from "./pki.js";
import { createOperationsTools } from "./operations.js";
import { createServiceTools } from "./service.js";
import { createStorageTools } from "./storage.js";

/**
 * Create all Clawbernetes tools.
 */
export function createAllTools(): ClawbernnetesTool[] {
  return [
    ...createClusterTools(),
    ...createWorkloadTools(),
    ...createObservabilityTools(),
    ...createAuthTools(),
    ...createDeployTools(),
    ...createTenancyTools(),
    ...createSecretsTools(),
    ...createPkiTools(),
    ...createOperationsTools(),
    ...createServiceTools(),
    ...createStorageTools(),
    ...createMoltTools(),
  ];
}

/**
 * Tool categories for documentation and discovery.
 */
export const TOOL_CATEGORIES = {
  cluster: {
    name: "Cluster & Nodes",
    description: "Cluster status and node management",
    tools: ["cluster_status", "node_list", "node_get", "node_drain", "node_cordon", "node_uncordon"],
  },
  workload: {
    name: "Workloads",
    description: "GPU workload submission and management",
    tools: [
      "workload_submit",
      "workload_get",
      "workload_list",
      "workload_stop",
      "workload_scale",
      "workload_logs",
    ],
  },
  observability: {
    name: "Observability",
    description: "Metrics, logs, and alerts",
    tools: ["metrics_query", "logs_search", "alert_create", "alert_list", "alert_silence"],
  },
  auth: {
    name: "Auth & RBAC",
    description: "User, role, and API key management",
    tools: [
      "user_create",
      "user_get",
      "user_list",
      "user_delete",
      "role_assign",
      "role_revoke",
      "role_list",
      "permission_check",
      "api_key_generate",
      "api_key_list",
      "api_key_revoke",
    ],
  },
  deploy: {
    name: "Deployments",
    description: "Intent-based deployments with canary and rollback",
    tools: [
      "deploy_intent",
      "deploy_status",
      "deploy_list",
      "deploy_promote",
      "deploy_rollback",
      "deploy_abort",
    ],
  },
  tenancy: {
    name: "Multi-Tenancy",
    description: "Tenant and namespace management with quotas",
    tools: [
      "tenant_create",
      "tenant_get",
      "tenant_list",
      "tenant_delete",
      "namespace_create",
      "namespace_get",
      "namespace_list",
      "namespace_delete",
      "quota_set",
      "usage_report",
    ],
  },
  secrets: {
    name: "Secrets",
    description: "Encrypted secrets management",
    tools: [
      "secret_put",
      "secret_get",
      "secret_delete",
      "secret_list",
      "secret_rotate",
      "secret_metadata",
    ],
  },
  pki: {
    name: "PKI / Certificates",
    description: "Certificate authority and mTLS",
    tools: ["cert_issue", "cert_get", "cert_list", "cert_revoke", "cert_rotate", "ca_status"],
  },
  operations: {
    name: "Operations",
    description: "Autoscaling, preemption, and rollback",
    tools: [
      "autoscale_pool_create",
      "autoscale_pool_list",
      "autoscale_evaluate",
      "autoscale_status",
      "preemption_register",
      "preemption_request",
      "preemption_list",
      "rollback_record",
      "rollback_plan",
      "rollback_history",
      "rollback_trigger_check",
    ],
  },
  service: {
    name: "Service Discovery",
    description: "Service mesh and load balancing",
    tools: [
      "service_register",
      "service_get",
      "service_list",
      "service_deregister",
      "endpoint_add",
      "endpoint_list",
      "endpoint_select",
    ],
  },
  storage: {
    name: "Storage",
    description: "Volume and claim management",
    tools: [
      "storage_class_create",
      "storage_class_list",
      "volume_provision",
      "volume_get",
      "volume_list",
      "claim_create",
      "claim_list",
      "claim_bind",
      "reconcile_claims",
    ],
  },
  molt: {
    name: "MOLT Marketplace",
    description: "P2P GPU capacity marketplace with escrow",
    tools: [
      "molt_offers",
      "molt_offer_create",
      "molt_order_create",
      "molt_find_matches",
      "molt_escrow_create",
      "molt_escrow_fund",
      "molt_escrow_release",
      "molt_escrow_refund",
      "molt_bid",
      "molt_spot_prices",
    ],
  },
} as const;

/**
 * Total tool count for documentation.
 */
export const TOTAL_TOOLS = Object.values(TOOL_CATEGORIES).reduce(
  (sum, cat) => sum + cat.tools.length,
  0
);
