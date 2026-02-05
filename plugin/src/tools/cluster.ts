/**
 * Cluster Tools
 *
 * Tools for cluster and node management operations.
 */

import { Type } from "@sinclair/typebox";
import type { ClawbernnetesTool, ToolResult } from "../types.js";
import { getClient } from "../client.js";

function jsonResult(data: unknown): ToolResult {
  return {
    type: "json",
    content: JSON.stringify(data, null, 2),
  };
}

function errorResult(message: string): ToolResult {
  return {
    type: "error",
    content: message,
  };
}

// ─────────────────────────────────────────────────────────────
// cluster_status
// ─────────────────────────────────────────────────────────────

const ClusterStatusSchema = Type.Object({
  cluster: Type.Optional(Type.String({ description: "Cluster name (uses default if omitted)" })),
});

export function createClusterStatusTool(): ClawbernnetesTool {
  return {
    name: "cluster_status",
    label: "Clawbernetes",
    description:
      "Get the current status of the Clawbernetes cluster including node count, GPU availability, and workload summary.",
    parameters: ClusterStatusSchema,
    execute: async (_id, _args, context) => {
      try {
        const client = getClient(context?.config);
        const status = await client.getClusterStatus();
        return jsonResult({
          cluster: status.name,
          healthy: status.healthy,
          nodes: status.nodes,
          gpus: status.gpus,
          workloads: status.workloads,
          summary: `Cluster ${status.name}: ${status.nodes.ready}/${status.nodes.total} nodes ready, ${status.gpus.available}/${status.gpus.total} GPUs available, ${status.workloads.running} workloads running`,
        });
      } catch (err) {
        return errorResult(`Failed to get cluster status: ${err}`);
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────
// node_list
// ─────────────────────────────────────────────────────────────

const NodeListSchema = Type.Object({
  status: Type.Optional(
    Type.String({
      description: "Filter by status: ready, not_ready, cordoned, draining, offline",
    })
  ),
  labels: Type.Optional(
    Type.Record(Type.String(), Type.String(), {
      description: "Filter by labels (key=value pairs)",
    })
  ),
  gpuModel: Type.Optional(Type.String({ description: "Filter by GPU model (e.g., H100, A100)" })),
});

export function createNodeListTool(): ClawbernnetesTool {
  return {
    name: "node_list",
    label: "Clawbernetes",
    description:
      "List all nodes in the cluster with optional filtering by status, labels, or GPU model.",
    parameters: NodeListSchema,
    execute: async (_id, args, context) => {
      try {
        const client = getClient(context?.config);
        const params = args as { status?: string; labels?: Record<string, string>; gpuModel?: string };
        let nodes = await client.listNodes({
          status: params.status,
          labels: params.labels,
        });

        // Filter by GPU model if specified
        if (params.gpuModel) {
          const model = params.gpuModel.toLowerCase();
          nodes = nodes.filter((n) =>
            n.gpus.some((g) => g.model.toLowerCase().includes(model))
          );
        }

        return jsonResult({
          count: nodes.length,
          nodes: nodes.map((n) => ({
            id: n.id,
            name: n.name,
            status: n.status,
            gpus: n.gpus.length,
            gpuModels: [...new Set(n.gpus.map((g) => g.model))],
            region: n.region,
            lastHeartbeat: new Date(n.lastHeartbeat).toISOString(),
          })),
        });
      } catch (err) {
        return errorResult(`Failed to list nodes: ${err}`);
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────
// node_get
// ─────────────────────────────────────────────────────────────

const NodeGetSchema = Type.Object({
  nodeId: Type.String({ description: "Node ID or name" }),
});

export function createNodeGetTool(): ClawbernnetesTool {
  return {
    name: "node_get",
    label: "Clawbernetes",
    description: "Get detailed information about a specific node including GPU status and workloads.",
    parameters: NodeGetSchema,
    execute: async (_id, args, context) => {
      try {
        const client = getClient(context?.config);
        const { nodeId } = args as { nodeId: string };
        const node = await client.getNode(nodeId);

        if (!node) {
          return errorResult(`Node not found: ${nodeId}`);
        }

        return jsonResult({
          id: node.id,
          name: node.name,
          status: node.status,
          platform: node.platform,
          region: node.region,
          zone: node.zone,
          resources: {
            cpuCores: node.cpuCores,
            memoryMb: node.memoryMb,
            gpus: node.gpus.map((g) => ({
              id: g.id,
              model: g.model,
              vendor: g.vendor,
              memoryMb: g.memoryMb,
              utilization: g.utilizationPercent,
              temperature: g.temperatureCelsius,
            })),
          },
          labels: node.labels,
          connectedAt: new Date(node.connectedAt).toISOString(),
          lastHeartbeat: new Date(node.lastHeartbeat).toISOString(),
        });
      } catch (err) {
        return errorResult(`Failed to get node: ${err}`);
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────
// node_drain
// ─────────────────────────────────────────────────────────────

const NodeDrainSchema = Type.Object({
  nodeId: Type.String({ description: "Node ID or name to drain" }),
  gracePeriodSeconds: Type.Optional(
    Type.Number({ description: "Grace period for workload migration (default: 300)" })
  ),
  force: Type.Optional(
    Type.Boolean({ description: "Force drain even if workloads cannot be migrated" })
  ),
});

export function createNodeDrainTool(): ClawbernnetesTool {
  return {
    name: "node_drain",
    label: "Clawbernetes",
    description:
      "Drain a node by migrating all workloads to other nodes. Use before maintenance or decommissioning.",
    parameters: NodeDrainSchema,
    execute: async (_id, args, context) => {
      try {
        const client = getClient(context?.config);
        const { nodeId, gracePeriodSeconds, force } = args as {
          nodeId: string;
          gracePeriodSeconds?: number;
          force?: boolean;
        };

        const result = await client.drainNode(nodeId, { gracePeriodSeconds, force });

        if (result.success) {
          return jsonResult({
            success: true,
            nodeId,
            migratedWorkloads: result.migratedWorkloads,
            message: `Successfully drained node ${nodeId}. Migrated ${result.migratedWorkloads} workloads.`,
          });
        } else {
          return errorResult(`Failed to drain node ${nodeId}`);
        }
      } catch (err) {
        return errorResult(`Failed to drain node: ${err}`);
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────
// node_cordon / node_uncordon
// ─────────────────────────────────────────────────────────────

const NodeCordonSchema = Type.Object({
  nodeId: Type.String({ description: "Node ID or name" }),
});

export function createNodeCordonTool(): ClawbernnetesTool {
  return {
    name: "node_cordon",
    label: "Clawbernetes",
    description:
      "Cordon a node to prevent new workloads from being scheduled on it. Existing workloads continue running.",
    parameters: NodeCordonSchema,
    execute: async (_id, args, context) => {
      try {
        const client = getClient(context?.config);
        const { nodeId } = args as { nodeId: string };
        const result = await client.cordonNode(nodeId);

        if (result.success) {
          return jsonResult({
            success: true,
            nodeId,
            message: `Node ${nodeId} cordoned. No new workloads will be scheduled.`,
          });
        } else {
          return errorResult(`Failed to cordon node ${nodeId}`);
        }
      } catch (err) {
        return errorResult(`Failed to cordon node: ${err}`);
      }
    },
  };
}

export function createNodeUncordonTool(): ClawbernnetesTool {
  return {
    name: "node_uncordon",
    label: "Clawbernetes",
    description: "Uncordon a node to allow new workloads to be scheduled on it again.",
    parameters: NodeCordonSchema,
    execute: async (_id, args, context) => {
      try {
        const client = getClient(context?.config);
        const { nodeId } = args as { nodeId: string };
        const result = await client.uncordonNode(nodeId);

        if (result.success) {
          return jsonResult({
            success: true,
            nodeId,
            message: `Node ${nodeId} uncordoned. Workloads can now be scheduled.`,
          });
        } else {
          return errorResult(`Failed to uncordon node ${nodeId}`);
        }
      } catch (err) {
        return errorResult(`Failed to uncordon node: ${err}`);
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────
// Export all cluster tools
// ─────────────────────────────────────────────────────────────

export function createClusterTools(): ClawbernnetesTool[] {
  return [
    createClusterStatusTool(),
    createNodeListTool(),
    createNodeGetTool(),
    createNodeDrainTool(),
    createNodeCordonTool(),
    createNodeUncordonTool(),
  ];
}
