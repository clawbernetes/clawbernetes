/**
 * Clawbernetes Tools Index
 *
 * Exports all tool factories for the OpenClaw plugin.
 */

export * from "./cluster.js";
export * from "./workload.js";
export * from "./observability.js";
export * from "./molt.js";

import type { ClawbernnetesTool } from "../types.js";
import { createClusterTools } from "./cluster.js";
import { createWorkloadTools } from "./workload.js";
import { createObservabilityTools } from "./observability.js";
import { createMoltTools } from "./molt.js";

/**
 * Create all Clawbernetes tools.
 */
export function createAllTools(): ClawbernnetesTool[] {
  return [
    ...createClusterTools(),
    ...createWorkloadTools(),
    ...createObservabilityTools(),
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
  molt: {
    name: "MOLT Marketplace",
    description: "P2P GPU capacity marketplace",
    tools: ["molt_offers", "molt_offer_create", "molt_bid", "molt_spot_prices"],
  },
} as const;
