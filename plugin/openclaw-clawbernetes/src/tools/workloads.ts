/**
 * claw_workloads — list all workloads across all connected clawnodes.
 *
 * Fans out workload.list to every node, merges results into a single
 * flat list with the originating nodeId attached.
 */

import { invokeAll, listNodes, type InvokeOptions } from "../invoke.js";

interface WorkloadEntry {
  nodeId: string;
  workloadId: string;
  image: string;
  state: string;
  gpus: number;
  created: string;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function registerWorkloads(api: any, invokeTimeoutMs: number): void {
  const opts: InvokeOptions = {
    gatewayUrl: api.config?.gatewayUrl ?? "http://127.0.0.1:18789",
    timeoutMs: invokeTimeoutMs,
    logger: api.logger,
  };

  api.registerTool({
    name: "claw_workloads",
    label: "Fleet Workloads",
    description:
      "List all workloads across all connected clawnodes. " +
      "Returns a flat array with nodeId, workloadId, image, state, GPU count, and creation time. " +
      "Optionally filter by state or image name.",
    parameters: {
      type: "object",
      properties: {
        state: {
          type: "string",
          description: "Filter by workload state: running, pending, stopped, failed",
        },
        image: {
          type: "string",
          description: "Filter by image name (substring match)",
        },
        nodeId: {
          type: "string",
          description: "Filter to a specific node",
        },
      },
    },
    async execute(_id: string, args: Record<string, unknown>) {
      try {
        let nodes = await listNodes(opts);
        if (nodes.length === 0) {
          return toolResult([]);
        }

        // If nodeId filter provided, only query that node
        const nodeIdFilter = typeof args.nodeId === "string" ? args.nodeId : undefined;
        if (nodeIdFilter) {
          nodes = nodes.filter((n) => n.nodeId === nodeIdFilter);
        }

        const nodeIds = nodes.map((n) => n.nodeId);
        const results = await invokeAll(opts, nodeIds, "workload.list");

        const workloads: WorkloadEntry[] = [];

        for (const nodeId of nodeIds) {
          const res = results.get(nodeId);
          if (!res?.ok) continue;

          const payload = res.payload;
          const items = Array.isArray(payload) ? payload : (typeof payload === "object" && payload !== null && Array.isArray((payload as Record<string, unknown>).workloads) ? (payload as Record<string, unknown>).workloads as unknown[] : []);

          for (const item of items) {
            if (typeof item !== "object" || item === null) continue;
            const w = item as Record<string, unknown>;

            workloads.push({
              nodeId,
              workloadId: String(w.workload_id ?? w.id ?? "unknown"),
              image: String(w.image ?? "unknown"),
              state: String(w.state ?? w.status ?? "unknown"),
              gpus: typeof w.gpus === "number" ? w.gpus : (typeof w.gpu_count === "number" ? w.gpu_count : 0),
              created: String(w.created ?? w.created_at ?? ""),
            });
          }
        }

        // Apply filters
        let filtered = workloads;
        const stateFilter = typeof args.state === "string" ? args.state.toLowerCase() : undefined;
        if (stateFilter) {
          filtered = filtered.filter((w) => w.state.toLowerCase() === stateFilter);
        }
        const imageFilter = typeof args.image === "string" ? args.image.toLowerCase() : undefined;
        if (imageFilter) {
          filtered = filtered.filter((w) => w.image.toLowerCase().includes(imageFilter));
        }

        return toolResult(filtered);
      } catch (err) {
        return toolError(err);
      }
    },
  });
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
