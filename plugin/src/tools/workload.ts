/**
 * Workload Tools
 *
 * Tools for GPU workload submission, management, and scaling.
 */

import { Type } from "@sinclair/typebox";
import type { ClawbernnetesTool, ToolResult, WorkloadSpec } from "../types.js";
import { getClient } from "../client.js";

function jsonResult(data: unknown): ToolResult {
  return { type: "json", content: JSON.stringify(data, null, 2) };
}

function errorResult(message: string): ToolResult {
  return { type: "error", content: message };
}

// ─────────────────────────────────────────────────────────────
// workload_submit
// ─────────────────────────────────────────────────────────────

const WorkloadSubmitSchema = Type.Object({
  name: Type.String({ description: "Workload name" }),
  image: Type.String({ description: "Container image (e.g., nvidia/cuda:12.0-base)" }),
  gpus: Type.Number({ description: "Number of GPUs required", minimum: 1 }),
  command: Type.Optional(Type.Array(Type.String(), { description: "Command to run" })),
  args: Type.Optional(Type.Array(Type.String(), { description: "Command arguments" })),
  env: Type.Optional(
    Type.Record(Type.String(), Type.String(), { description: "Environment variables" })
  ),
  gpuMemoryMb: Type.Optional(Type.Number({ description: "Minimum GPU memory required (MB)" })),
  cpuCores: Type.Optional(Type.Number({ description: "CPU cores required" })),
  memoryMb: Type.Optional(Type.Number({ description: "Memory required (MB)" })),
  priority: Type.Optional(Type.Number({ description: "Priority (higher = more important)", minimum: 0, maximum: 100 })),
  preemptible: Type.Optional(Type.Boolean({ description: "Can this workload be preempted?" })),
  maxRuntimeSeconds: Type.Optional(Type.Number({ description: "Maximum runtime before auto-stop" })),
  labels: Type.Optional(
    Type.Record(Type.String(), Type.String(), { description: "Labels for filtering/grouping" })
  ),
});

export function createWorkloadSubmitTool(): ClawbernnetesTool {
  return {
    name: "workload_submit",
    label: "Clawbernetes",
    description:
      "Submit a new GPU workload to the cluster. The scheduler will find suitable nodes based on GPU requirements.",
    parameters: WorkloadSubmitSchema,
    execute: async (_id, args, context) => {
      try {
        const client = getClient(context?.config);
        const spec = args as WorkloadSpec;
        const workload = await client.submitWorkload(spec);

        return jsonResult({
          success: true,
          workload: {
            id: workload.id,
            name: workload.spec.name,
            state: workload.state,
            gpus: workload.spec.gpus,
            createdAt: new Date(workload.createdAt).toISOString(),
          },
          message: `Workload ${workload.spec.name} submitted (id: ${workload.id}). State: ${workload.state}`,
        });
      } catch (err) {
        return errorResult(`Failed to submit workload: ${err}`);
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────
// workload_get
// ─────────────────────────────────────────────────────────────

const WorkloadGetSchema = Type.Object({
  workloadId: Type.String({ description: "Workload ID" }),
});

export function createWorkloadGetTool(): ClawbernnetesTool {
  return {
    name: "workload_get",
    label: "Clawbernetes",
    description: "Get detailed information about a specific workload.",
    parameters: WorkloadGetSchema,
    execute: async (_id, args, context) => {
      try {
        const client = getClient(context?.config);
        const { workloadId } = args as { workloadId: string };
        const workload = await client.getWorkload(workloadId);

        if (!workload) {
          return errorResult(`Workload not found: ${workloadId}`);
        }

        return jsonResult({
          id: workload.id,
          name: workload.spec.name,
          state: workload.state,
          nodeId: workload.nodeId,
          spec: workload.spec,
          timing: {
            createdAt: new Date(workload.createdAt).toISOString(),
            startedAt: workload.startedAt
              ? new Date(workload.startedAt).toISOString()
              : null,
            finishedAt: workload.finishedAt
              ? new Date(workload.finishedAt).toISOString()
              : null,
            runtime: workload.startedAt
              ? `${Math.round(((workload.finishedAt ?? Date.now()) - workload.startedAt) / 1000)}s`
              : null,
          },
          exitCode: workload.exitCode,
          error: workload.error,
        });
      } catch (err) {
        return errorResult(`Failed to get workload: ${err}`);
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────
// workload_list
// ─────────────────────────────────────────────────────────────

const WorkloadListSchema = Type.Object({
  state: Type.Optional(
    Type.String({
      description: "Filter by state: pending, scheduled, running, succeeded, failed, cancelled",
    })
  ),
  nodeId: Type.Optional(Type.String({ description: "Filter by node ID" })),
  labels: Type.Optional(
    Type.Record(Type.String(), Type.String(), { description: "Filter by labels" })
  ),
  limit: Type.Optional(Type.Number({ description: "Max results to return", default: 50 })),
});

export function createWorkloadListTool(): ClawbernnetesTool {
  return {
    name: "workload_list",
    label: "Clawbernetes",
    description: "List workloads with optional filtering by state, node, or labels.",
    parameters: WorkloadListSchema,
    execute: async (_id, args, context) => {
      try {
        const client = getClient(context?.config);
        const params = args as {
          state?: string;
          nodeId?: string;
          labels?: Record<string, string>;
          limit?: number;
        };

        const workloads = await client.listWorkloads({
          state: params.state,
          nodeId: params.nodeId,
          labels: params.labels,
        });

        const limited = workloads.slice(0, params.limit ?? 50);

        return jsonResult({
          count: limited.length,
          total: workloads.length,
          workloads: limited.map((w) => ({
            id: w.id,
            name: w.spec.name,
            state: w.state,
            gpus: w.spec.gpus,
            nodeId: w.nodeId,
            createdAt: new Date(w.createdAt).toISOString(),
          })),
        });
      } catch (err) {
        return errorResult(`Failed to list workloads: ${err}`);
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────
// workload_stop
// ─────────────────────────────────────────────────────────────

const WorkloadStopSchema = Type.Object({
  workloadId: Type.String({ description: "Workload ID to stop" }),
  gracePeriodSeconds: Type.Optional(
    Type.Number({ description: "Grace period before force kill (default: 30)" })
  ),
  force: Type.Optional(Type.Boolean({ description: "Force kill immediately" })),
});

export function createWorkloadStopTool(): ClawbernnetesTool {
  return {
    name: "workload_stop",
    label: "Clawbernetes",
    description: "Stop a running workload. Use force=true to kill immediately.",
    parameters: WorkloadStopSchema,
    execute: async (_id, args, context) => {
      try {
        const client = getClient(context?.config);
        const { workloadId, gracePeriodSeconds, force } = args as {
          workloadId: string;
          gracePeriodSeconds?: number;
          force?: boolean;
        };

        const result = await client.stopWorkload(workloadId, { gracePeriodSeconds, force });

        if (result.success) {
          return jsonResult({
            success: true,
            workloadId,
            message: `Workload ${workloadId} stop initiated${force ? " (force)" : ""}.`,
          });
        } else {
          return errorResult(`Failed to stop workload ${workloadId}`);
        }
      } catch (err) {
        return errorResult(`Failed to stop workload: ${err}`);
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────
// workload_scale
// ─────────────────────────────────────────────────────────────

const WorkloadScaleSchema = Type.Object({
  workloadId: Type.String({ description: "Workload ID to scale" }),
  replicas: Type.Number({ description: "Target number of replicas", minimum: 0 }),
});

export function createWorkloadScaleTool(): ClawbernnetesTool {
  return {
    name: "workload_scale",
    label: "Clawbernetes",
    description: "Scale a workload to a specific number of replicas.",
    parameters: WorkloadScaleSchema,
    execute: async (_id, args, context) => {
      try {
        const client = getClient(context?.config);
        const { workloadId, replicas } = args as { workloadId: string; replicas: number };

        const result = await client.scaleWorkload(workloadId, replicas);

        if (result.success) {
          return jsonResult({
            success: true,
            workloadId,
            previousReplicas: result.previousReplicas,
            newReplicas: replicas,
            message: `Scaled workload ${workloadId}: ${result.previousReplicas} → ${replicas} replicas.`,
          });
        } else {
          return errorResult(`Failed to scale workload ${workloadId}`);
        }
      } catch (err) {
        return errorResult(`Failed to scale workload: ${err}`);
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────
// workload_logs
// ─────────────────────────────────────────────────────────────

const WorkloadLogsSchema = Type.Object({
  workloadId: Type.String({ description: "Workload ID" }),
  tail: Type.Optional(Type.Number({ description: "Number of lines from the end", default: 100 })),
  since: Type.Optional(Type.String({ description: "Show logs since (e.g., '5m', '1h', '2024-01-01')" })),
  level: Type.Optional(Type.String({ description: "Filter by log level: debug, info, warn, error" })),
});

export function createWorkloadLogsTool(): ClawbernnetesTool {
  return {
    name: "workload_logs",
    label: "Clawbernetes",
    description: "Get logs from a workload.",
    parameters: WorkloadLogsSchema,
    execute: async (_id, args, context) => {
      try {
        const client = getClient(context?.config);
        const { workloadId, tail, level } = args as {
          workloadId: string;
          tail?: number;
          since?: string;
          level?: string;
        };

        const logs = await client.searchLogs({
          workloadId,
          level,
          limit: tail ?? 100,
        });

        return jsonResult({
          workloadId,
          count: logs.length,
          logs: logs.map((l) => ({
            timestamp: new Date(l.timestamp).toISOString(),
            level: l.level,
            message: l.message,
          })),
        });
      } catch (err) {
        return errorResult(`Failed to get workload logs: ${err}`);
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────
// Export all workload tools
// ─────────────────────────────────────────────────────────────

export function createWorkloadTools(): ClawbernnetesTool[] {
  return [
    createWorkloadSubmitTool(),
    createWorkloadGetTool(),
    createWorkloadListTool(),
    createWorkloadStopTool(),
    createWorkloadScaleTool(),
    createWorkloadLogsTool(),
  ];
}
