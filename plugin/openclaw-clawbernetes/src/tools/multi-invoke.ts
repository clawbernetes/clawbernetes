/**
 * claw_multi_invoke — run any command on multiple nodes in parallel.
 *
 * This is the escape hatch: any of the 91 clawnode commands can be
 * fanned out across the fleet in a single tool call.
 */

import { invokeAll, listNodes, type InvokeOptions } from "../invoke.js";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function registerMultiInvoke(api: any, invokeTimeoutMs: number): void {
  const opts: InvokeOptions = {
    gatewayUrl: api.config?.gatewayUrl ?? "http://127.0.0.1:18789",
    timeoutMs: invokeTimeoutMs,
    logger: api.logger,
  };

  api.registerTool({
    name: "claw_multi_invoke",
    label: "Multi-Node Invoke",
    description:
      "Run any clawnode command on multiple nodes in parallel. " +
      "If nodes are not specified, runs on ALL connected nodes. " +
      "Returns per-node results. Use this for any of the 91 commands " +
      "when you need fleet-wide execution (e.g., gpu.metrics on all nodes).",
    parameters: {
      type: "object",
      properties: {
        command: {
          type: "string",
          description: "The clawnode command to invoke (e.g., 'gpu.metrics', 'system.info', 'workload.list')",
        },
        params: {
          type: "object",
          description: "Parameters to pass to the command (same on every node)",
        },
        nodes: {
          type: "array",
          items: { type: "string" },
          description: "Specific node IDs to target. Omit to run on ALL nodes.",
        },
      },
      required: ["command"],
    },
    async execute(_id: string, args: Record<string, unknown>) {
      try {
        const command = args.command as string;
        const params = typeof args.params === "object" && args.params !== null
          ? args.params as Record<string, unknown>
          : undefined;

        let nodeIds: string[];
        if (Array.isArray(args.nodes) && args.nodes.length > 0) {
          nodeIds = args.nodes.filter((n): n is string => typeof n === "string");
        } else {
          const nodes = await listNodes(opts);
          nodeIds = nodes.map((n) => n.nodeId);
        }

        if (nodeIds.length === 0) {
          return toolResult({ results: [], message: "No nodes available" });
        }

        const results = await invokeAll(opts, nodeIds, command, params);

        const output = nodeIds.map((nodeId) => {
          const r = results.get(nodeId);
          if (!r) return { nodeId, ok: false, error: "No response" };
          if (r.ok) return { nodeId, ok: true, payload: r.payload };
          return { nodeId, ok: false, error: r.error };
        });

        return toolResult({
          command,
          nodeCount: nodeIds.length,
          succeeded: output.filter((r) => r.ok).length,
          failed: output.filter((r) => !r.ok).length,
          results: output,
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
