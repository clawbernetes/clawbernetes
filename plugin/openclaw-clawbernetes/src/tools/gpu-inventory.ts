/**
 * claw_gpu_inventory — list all GPUs across all connected clawnodes.
 *
 * Fans out gpu.list + gpu.metrics to every node, returns a flat
 * inventory with per-GPU utilization and availability.
 */

import { invokeAll, listNodes, type InvokeOptions } from "../invoke.js";

interface GpuEntry {
  nodeId: string;
  gpuIndex: number;
  model: string;
  vramMb: number;
  utilization: number;
  temperature: number;
  available: boolean;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function registerGpuInventory(api: any, invokeTimeoutMs: number): void {
  const opts: InvokeOptions = {
    gatewayUrl: api.pluginConfig?.gatewayUrl ?? "http://127.0.0.1:18789",
    timeoutMs: invokeTimeoutMs,
    logger: api.logger,
  };

  api.registerTool({
    name: "claw_gpu_inventory",
    label: "GPU Inventory",
    description:
      "List all GPUs across all connected clawnodes with model, VRAM, " +
      "utilization, temperature, and availability. " +
      "Returns a flat array for easy filtering.",
    parameters: {
      type: "object",
      properties: {
        model: {
          type: "string",
          description: "Filter by GPU model (e.g., H100, A100, RTX4090)",
        },
        availableOnly: {
          type: "boolean",
          description: "Only show GPUs not currently allocated to a workload",
        },
      },
    },
    async execute(_id: string, args: Record<string, unknown>) {
      try {
        const nodes = await listNodes(opts);
        if (nodes.length === 0) {
          return toolResult([]);
        }

        const nodeIds = nodes.map((n) => n.nodeId);

        const [gpuListResults, gpuMetricsResults] = await Promise.all([
          invokeAll(opts, nodeIds, "gpu.list"),
          invokeAll(opts, nodeIds, "gpu.metrics"),
        ]);

        const inventory: GpuEntry[] = [];

        for (const nodeId of nodeIds) {
          const listRes = gpuListResults.get(nodeId);
          const metricsRes = gpuMetricsResults.get(nodeId);

          if (!listRes?.ok || !Array.isArray(listRes.payload)) continue;

          const gpus = listRes.payload as Array<Record<string, unknown>>;
          const metricsMap = buildMetricsMap(metricsRes);

          for (let i = 0; i < gpus.length; i++) {
            const gpu = gpus[i];
            const metrics = metricsMap.get(i);

            inventory.push({
              nodeId,
              gpuIndex: typeof gpu.index === "number" ? gpu.index : i,
              model: String(gpu.model ?? gpu.name ?? "unknown"),
              vramMb: typeof gpu.vram_mb === "number" ? gpu.vram_mb : (typeof gpu.memory_mb === "number" ? gpu.memory_mb : 0),
              utilization: metrics?.utilization ?? (typeof gpu.utilization === "number" ? gpu.utilization : 0),
              temperature: metrics?.temperature ?? (typeof gpu.temperature === "number" ? gpu.temperature : 0),
              available: gpu.available !== false && gpu.allocated !== true,
            });
          }
        }

        // Apply filters
        let filtered = inventory;
        const modelFilter = typeof args.model === "string" ? args.model.toLowerCase() : undefined;
        if (modelFilter) {
          filtered = filtered.filter((g) => g.model.toLowerCase().includes(modelFilter));
        }
        if (args.availableOnly === true) {
          filtered = filtered.filter((g) => g.available);
        }

        return toolResult(filtered);
      } catch (err) {
        return toolError(err);
      }
    },
  });
}

function buildMetricsMap(metricsRes: { ok: boolean; payload?: unknown } | undefined): Map<number, { utilization: number; temperature: number }> {
  const map = new Map<number, { utilization: number; temperature: number }>();
  if (!metricsRes?.ok) return map;

  const payload = metricsRes.payload;
  if (Array.isArray(payload)) {
    for (const m of payload) {
      if (typeof m === "object" && m !== null) {
        const rec = m as Record<string, unknown>;
        const idx = typeof rec.index === "number" ? rec.index : (typeof rec.gpu_index === "number" ? rec.gpu_index : -1);
        if (idx >= 0) {
          map.set(idx, {
            utilization: typeof rec.utilization === "number" ? rec.utilization : 0,
            temperature: typeof rec.temperature === "number" ? rec.temperature : (typeof rec.temperature_c === "number" ? rec.temperature_c : 0),
          });
        }
      }
    }
  }
  return map;
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
