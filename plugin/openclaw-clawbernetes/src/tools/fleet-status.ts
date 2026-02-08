/**
 * claw_fleet_status — aggregate cluster state in a single tool call.
 *
 * Fans out node.health + node.capabilities to every connected clawnode,
 * then aggregates into a fleet-level summary.
 */

import { invokeAll, listNodes, type InvokeOptions } from "../invoke.js";

interface GpuSummary {
  total: number;
  available: number;
  models: Record<string, number>;
}

interface MemorySummary {
  totalBytes: number;
  availableBytes: number;
}

interface FleetStatusResult {
  nodes: number;
  healthy: number;
  unhealthy: number;
  gpus: GpuSummary;
  workloads: number;
  memory: MemorySummary;
  nodeDetails: Array<{
    nodeId: string;
    healthy: boolean;
    gpuCount?: number;
    workloadCount?: number;
  }>;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function registerFleetStatus(api: any, invokeTimeoutMs: number): void {
  const opts: InvokeOptions = {
    gatewayUrl: api.config?.gatewayUrl ?? "http://127.0.0.1:18789",
    timeoutMs: invokeTimeoutMs,
    logger: api.logger,
  };

  api.registerTool({
    name: "claw_fleet_status",
    label: "Fleet Status",
    description:
      "Get aggregate fleet status across all connected clawnodes. " +
      "Returns node count, health summary, total GPUs, memory, and workload counts. " +
      "No parameters required.",
    parameters: {
      type: "object",
      properties: {
        nodeFilter: {
          type: "string",
          description: "Optional: filter nodes by name prefix",
        },
      },
    },
    async execute(_id: string, args: Record<string, unknown>) {
      try {
        const nodes = await listNodes(opts);
        if (nodes.length === 0) {
          return toolResult({ nodes: 0, healthy: 0, unhealthy: 0, gpus: { total: 0, available: 0, models: {} }, workloads: 0, memory: { totalBytes: 0, availableBytes: 0 }, nodeDetails: [] });
        }

        const filter = typeof args.nodeFilter === "string" ? args.nodeFilter : undefined;
        const filteredNodes = filter
          ? nodes.filter((n) => n.nodeId.startsWith(filter) || n.name?.startsWith(filter))
          : nodes;

        const nodeIds = filteredNodes.map((n) => n.nodeId);

        // Fan out health + capabilities in parallel
        const [healthResults, capResults] = await Promise.all([
          invokeAll(opts, nodeIds, "node.health"),
          invokeAll(opts, nodeIds, "node.capabilities"),
        ]);

        const result: FleetStatusResult = {
          nodes: nodeIds.length,
          healthy: 0,
          unhealthy: 0,
          gpus: { total: 0, available: 0, models: {} },
          workloads: 0,
          memory: { totalBytes: 0, availableBytes: 0 },
          nodeDetails: [],
        };

        for (const nodeId of nodeIds) {
          const health = healthResults.get(nodeId);
          const caps = capResults.get(nodeId);

          const isHealthy = (health?.ok && isHealthyPayload(health.payload)) === true;
          if (isHealthy) result.healthy++;
          else result.unhealthy++;

          const detail: FleetStatusResult["nodeDetails"][number] = {
            nodeId,
            healthy: isHealthy,
          };

          if (caps?.ok && typeof caps.payload === "object" && caps.payload !== null) {
            const p = caps.payload as Record<string, unknown>;
            const gpuCount = typeof p.gpu_count === "number" ? p.gpu_count : 0;
            const gpuAvail = typeof p.gpu_available === "number" ? p.gpu_available : gpuCount;
            const gpuModel = typeof p.gpu_model === "string" ? p.gpu_model : "unknown";
            const totalMem = typeof p.memory_total === "number" ? p.memory_total : 0;
            const availMem = typeof p.memory_available === "number" ? p.memory_available : 0;
            const workloads = typeof p.workload_count === "number" ? p.workload_count : 0;

            result.gpus.total += gpuCount;
            result.gpus.available += gpuAvail;
            result.gpus.models[gpuModel] = (result.gpus.models[gpuModel] ?? 0) + gpuCount;
            result.memory.totalBytes += totalMem;
            result.memory.availableBytes += availMem;
            result.workloads += workloads;

            detail.gpuCount = gpuCount;
            detail.workloadCount = workloads;
          }

          result.nodeDetails.push(detail);
        }

        return toolResult(result);
      } catch (err) {
        return toolError(err);
      }
    },
  });
}

function isHealthyPayload(payload: unknown): boolean {
  if (typeof payload === "object" && payload !== null) {
    const p = payload as Record<string, unknown>;
    return p.healthy === true || p.status === "healthy" || p.status === "ok";
  }
  return false;
}

function toolResult(data: unknown) {
  return {
    content: [{ type: "text", text: JSON.stringify(data, null, 2) }],
    details: data,
  };
}

function toolError(err: unknown) {
  return {
    content: [{ type: "text", text: `Error: ${err instanceof Error ? err.message : String(err)}` }],
    isError: true,
  };
}
