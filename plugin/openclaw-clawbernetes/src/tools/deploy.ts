/**
 * claw_deploy — deploy a workload to the best available node.
 *
 * Scores nodes by available GPUs, memory, health, and current load,
 * then runs workload.run on the selected node.
 */

import { invokeNode, invokeAll, listNodes, type InvokeOptions } from "../invoke.js";

interface NodeScore {
  nodeId: string;
  score: number;
  gpuAvailable: number;
  memoryAvailable: number;
  healthy: boolean;
  workloadCount: number;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function registerDeploy(api: any, invokeTimeoutMs: number): void {
  const opts: InvokeOptions = {
    gatewayUrl: api.pluginConfig?.gatewayUrl ?? "http://127.0.0.1:18789",
    timeoutMs: invokeTimeoutMs,
    logger: api.logger,
  };

  api.registerTool({
    name: "claw_deploy",
    label: "Deploy Workload",
    description:
      "Deploy a workload to the best available node in the fleet. " +
      "Automatically selects a node based on GPU availability, memory, " +
      "health status, and current load. Returns the selected node and workload ID.",
    parameters: {
      type: "object",
      properties: {
        image: {
          type: "string",
          description: "Container image to deploy (e.g., pytorch/pytorch:2.0-cuda12)",
        },
        gpus: {
          type: "number",
          description: "Number of GPUs required (default: 1)",
        },
        memory: {
          type: "string",
          description: "Memory requirement (e.g., '8g', '512m')",
        },
        env: {
          type: "object",
          description: "Environment variables as key-value pairs",
        },
        command: {
          type: "array",
          items: { type: "string" },
          description: "Command to run in the container",
        },
        preferNode: {
          type: "string",
          description: "Preferred node ID (will be used if it meets requirements)",
        },
      },
      required: ["image"],
    },
    async execute(_id: string, args: Record<string, unknown>) {
      try {
        const image = args.image as string;
        const gpusNeeded = typeof args.gpus === "number" ? args.gpus : 1;
        const memory = typeof args.memory === "string" ? args.memory : undefined;
        const env = typeof args.env === "object" && args.env !== null ? args.env as Record<string, string> : undefined;
        const command = Array.isArray(args.command) ? args.command as string[] : undefined;
        const preferNode = typeof args.preferNode === "string" ? args.preferNode : undefined;

        // Discover nodes
        const nodes = await listNodes(opts);
        if (nodes.length === 0) {
          return toolError(new Error("No nodes connected to the fleet"));
        }

        const nodeIds = nodes.map((n) => n.nodeId);

        // Get capabilities and health for all nodes
        const [capsResults, healthResults] = await Promise.all([
          invokeAll(opts, nodeIds, "node.capabilities"),
          invokeAll(opts, nodeIds, "node.health"),
        ]);

        // Score each node
        const scores: NodeScore[] = [];
        for (const nodeId of nodeIds) {
          const caps = capsResults.get(nodeId);
          const health = healthResults.get(nodeId);

          if (!caps?.ok || !health?.ok) continue;

          const cp = (caps.payload ?? {}) as Record<string, unknown>;
          const hp = (health.payload ?? {}) as Record<string, unknown>;

          const gpuAvailable = typeof cp.gpu_available === "number" ? cp.gpu_available : 0;
          const memoryAvailable = typeof cp.memory_available === "number" ? cp.memory_available : 0;
          const workloadCount = typeof cp.workload_count === "number" ? cp.workload_count : 0;
          const healthy = hp.healthy === true || hp.status === "healthy" || hp.status === "ok";

          // Skip nodes that can't fit the workload
          if (gpuAvailable < gpusNeeded) continue;
          if (!healthy) continue;

          // Score: more available GPUs, more memory, fewer workloads = higher score
          let score = gpuAvailable * 100 + memoryAvailable / (1024 * 1024) - workloadCount * 50;

          // Bonus for preferred node
          if (preferNode && nodeId === preferNode) {
            score += 10000;
          }

          scores.push({ nodeId, score, gpuAvailable, memoryAvailable, healthy, workloadCount });
        }

        if (scores.length === 0) {
          return toolError(new Error(`No node has ${gpusNeeded} available GPU(s). Check fleet status with claw_fleet_status.`));
        }

        // Pick best node
        scores.sort((a, b) => b.score - a.score);
        const best = scores[0];

        // Deploy to selected node
        const deployParams: Record<string, unknown> = {
          image,
          gpus: gpusNeeded,
        };
        if (memory) deployParams.memory = memory;
        if (env) deployParams.env = env;
        if (command) deployParams.command = command;

        const deployResult = await invokeNode(opts, best.nodeId, "workload.run", deployParams);

        if (!deployResult.ok) {
          return toolError(new Error(`Deploy to ${best.nodeId} failed: ${deployResult.error}`));
        }

        const payload = (deployResult.payload ?? {}) as Record<string, unknown>;

        return toolResult({
          nodeId: best.nodeId,
          workloadId: payload.workload_id ?? payload.id ?? "unknown",
          gpusAllocated: gpusNeeded,
          image,
          candidatesEvaluated: scores.length,
        });
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
