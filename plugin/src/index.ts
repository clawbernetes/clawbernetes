/**
 * Clawbernetes OpenClaw Plugin
 *
 * AI-native GPU orchestration tools for OpenClaw.
 */

import { Type } from "@sinclair/typebox";
import { ClawbernetesClient } from "./client.js";

// Re-export types
export * from "./types.js";
export * from "./client.js";

/**
 * Plugin version
 */
export const VERSION = "0.1.0";

/**
 * Plugin ID
 */
export const PLUGIN_ID = "clawbernetes";

// ─────────────────────────────────────────────────────────────
// Config Schema
// ─────────────────────────────────────────────────────────────

interface ClawbernetesConfig {
  bridgePath: string;
  defaultCluster?: string;
}

const clawbernetesConfigSchema = {
  parse(value: unknown): ClawbernetesConfig {
    const raw = value && typeof value === "object" ? (value as Record<string, unknown>) : {};
    return {
      bridgePath: typeof raw.bridgePath === "string" ? raw.bridgePath : "",
      defaultCluster: typeof raw.defaultCluster === "string" ? raw.defaultCluster : "default",
    };
  },
  uiHints: {
    bridgePath: {
      label: "Bridge Binary Path",
      placeholder: "/path/to/claw-bridge",
    },
    defaultCluster: {
      label: "Default Cluster",
      placeholder: "default",
    },
  },
};

// ─────────────────────────────────────────────────────────────
// Tool Schemas
// ─────────────────────────────────────────────────────────────

const ClusterStatusSchema = Type.Object({});

const NodeListSchema = Type.Object({
  status: Type.Optional(Type.String({ description: "Filter by status: healthy, unhealthy, draining, offline" })),
  gpu_type: Type.Optional(Type.String({ description: "Filter by GPU type: H100, A100, etc." })),
});

const NodeGetSchema = Type.Object({
  node_id: Type.String({ description: "Node ID" }),
});

const NodeDrainSchema = Type.Object({
  node_id: Type.String({ description: "Node ID to drain" }),
  force: Type.Optional(Type.Boolean({ description: "Force drain even with running workloads" })),
});

const NodeCordonSchema = Type.Object({
  node_id: Type.String({ description: "Node ID to cordon/uncordon" }),
});

const WorkloadSubmitSchema = Type.Object({
  name: Type.Optional(Type.String({ description: "Workload name" })),
  image: Type.String({ description: "Container image" }),
  gpus: Type.Optional(Type.Number({ description: "Number of GPUs required" })),
  cpu_cores: Type.Optional(Type.Number({ description: "CPU cores" })),
  memory_mb: Type.Optional(Type.Number({ description: "Memory in MB" })),
  env: Type.Optional(Type.Record(Type.String(), Type.String(), { description: "Environment variables" })),
  command: Type.Optional(Type.Array(Type.String(), { description: "Command to run" })),
});

const WorkloadGetSchema = Type.Object({
  workload_id: Type.String({ description: "Workload ID" }),
});

const WorkloadListSchema = Type.Object({
  state: Type.Optional(Type.String({ description: "Filter by state: pending, running, completed, failed" })),
  node_id: Type.Optional(Type.String({ description: "Filter by node ID" })),
  limit: Type.Optional(Type.Number({ description: "Max results" })),
});

const WorkloadStopSchema = Type.Object({
  workload_id: Type.String({ description: "Workload ID to stop" }),
  force: Type.Optional(Type.Boolean({ description: "Force stop" })),
});

const WorkloadScaleSchema = Type.Object({
  workload_id: Type.String({ description: "Workload ID" }),
  replicas: Type.Number({ description: "Number of replicas" }),
});

const WorkloadLogsSchema = Type.Object({
  workload_id: Type.String({ description: "Workload ID" }),
  tail: Type.Optional(Type.Number({ description: "Number of lines to tail" })),
});

const MetricsQuerySchema = Type.Object({
  name: Type.String({ description: "Metric name (e.g., gpu_utilization, memory_usage)" }),
  start_time: Type.Optional(Type.Number({ description: "Start timestamp (ms)" })),
  end_time: Type.Optional(Type.Number({ description: "End timestamp (ms)" })),
  labels: Type.Optional(Type.Record(Type.String(), Type.String(), { description: "Label filters" })),
});

const LogsSearchSchema = Type.Object({
  text: Type.Optional(Type.String({ description: "Search text" })),
  level: Type.Optional(Type.String({ description: "Log level: trace, debug, info, warn, error" })),
  workload_id: Type.Optional(Type.String({ description: "Filter by workload ID" })),
  node_id: Type.Optional(Type.String({ description: "Filter by node ID" })),
  limit: Type.Optional(Type.Number({ description: "Max results" })),
});

const AlertCreateSchema = Type.Object({
  name: Type.String({ description: "Alert rule name" }),
  severity: Type.String({ description: "Severity: info, warning, critical" }),
  condition: Type.String({ description: "Condition: 'metric_name operator value' (e.g., 'gpu_temp > 80')" }),
  message: Type.String({ description: "Alert message" }),
  labels: Type.Optional(Type.Record(Type.String(), Type.String(), { description: "Labels" })),
});

const AlertListSchema = Type.Object({
  severity: Type.Optional(Type.String({ description: "Filter by severity" })),
  resolved: Type.Optional(Type.Boolean({ description: "Include resolved alerts" })),
});

const AlertSilenceSchema = Type.Object({
  alert_id: Type.String({ description: "Alert ID to silence" }),
  duration_seconds: Type.Number({ description: "Silence duration in seconds" }),
});

const MoltOffersSchema = Type.Object({
  gpu_type: Type.Optional(Type.String({ description: "GPU type filter" })),
  min_gpus: Type.Optional(Type.Number({ description: "Minimum GPU count" })),
  max_price_per_hour: Type.Optional(Type.Number({ description: "Maximum price per hour" })),
});

const MoltOfferCreateSchema = Type.Object({
  gpu_type: Type.String({ description: "GPU type being offered" }),
  gpu_count: Type.Number({ description: "Number of GPUs" }),
  price_per_hour: Type.Number({ description: "Price per hour" }),
  min_duration_hours: Type.Optional(Type.Number({ description: "Minimum rental duration" })),
  max_duration_hours: Type.Optional(Type.Number({ description: "Maximum rental duration" })),
});

const MoltBidSchema = Type.Object({
  offer_id: Type.String({ description: "Offer ID to bid on" }),
  price_per_hour: Type.Number({ description: "Bid price per hour" }),
  duration_hours: Type.Number({ description: "Requested duration" }),
});

const MoltSpotPricesSchema = Type.Object({
  gpu_type: Type.Optional(Type.String({ description: "GPU type filter" })),
});

// ─────────────────────────────────────────────────────────────
// Plugin Registration
// ─────────────────────────────────────────────────────────────

const clawbernetesPlugin = {
  id: PLUGIN_ID,
  name: "Clawbernetes",
  description: "AI-native GPU orchestration tools",
  configSchema: clawbernetesConfigSchema,

  register(api: any) {
    const config = clawbernetesConfigSchema.parse(api.pluginConfig);

    if (!config.bridgePath) {
      api.logger.warn("[clawbernetes] bridgePath not configured; tools will fail");
    }

    let client: ClawbernetesClient | null = null;

    const ensureClient = async (): Promise<ClawbernetesClient> => {
      if (!config.bridgePath) {
        throw new Error("clawbernetes.bridgePath not configured");
      }
      if (!client) {
        client = new ClawbernetesClient({ bridgePath: config.bridgePath });
        await client.initialize();
      }
      return client;
    };

    const callBridge = async (method: string, params: unknown) => {
      const c = await ensureClient();
      return c.rpc(method, params);
    };

    const toolResult = (data: unknown) => ({
      content: [{ type: "text", text: JSON.stringify(data, null, 2) }],
      details: data,
    });

    const toolError = (err: unknown) => ({
      content: [{ type: "text", text: `Error: ${err instanceof Error ? err.message : String(err)}` }],
      isError: true,
    });

    // ─── Cluster Tools ───

    api.registerTool({
      name: "cluster_status",
      label: "Cluster Status",
      description: "Get overall cluster health and resource summary including nodes, GPUs, and workload counts.",
      parameters: ClusterStatusSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("cluster_status", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "node_list",
      label: "List Nodes",
      description: "List all nodes in the cluster with optional filtering by status or GPU type.",
      parameters: NodeListSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("node_list", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "node_get",
      label: "Get Node",
      description: "Get detailed information about a specific node including GPUs and workloads.",
      parameters: NodeGetSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("node_get", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "node_drain",
      label: "Drain Node",
      description: "Drain a node by migrating all workloads off it. Use before maintenance.",
      parameters: NodeDrainSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("node_drain", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "node_cordon",
      label: "Cordon Node",
      description: "Prevent new workloads from being scheduled on a node.",
      parameters: NodeCordonSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("node_cordon", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "node_uncordon",
      label: "Uncordon Node",
      description: "Allow workloads to be scheduled on a previously cordoned node.",
      parameters: NodeCordonSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("node_uncordon", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    // ─── Workload Tools ───

    api.registerTool({
      name: "workload_submit",
      label: "Submit Workload",
      description: "Submit a new GPU workload to the cluster. Specify image, GPU count, and resources.",
      parameters: WorkloadSubmitSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("workload_submit", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "workload_get",
      label: "Get Workload",
      description: "Get detailed information about a specific workload.",
      parameters: WorkloadGetSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("workload_get", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "workload_list",
      label: "List Workloads",
      description: "List workloads with optional filtering by state or node.",
      parameters: WorkloadListSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("workload_list", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "workload_stop",
      label: "Stop Workload",
      description: "Stop a running workload. Use force=true to terminate immediately.",
      parameters: WorkloadStopSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("workload_stop", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "workload_scale",
      label: "Scale Workload",
      description: "Scale a workload to a specified number of replicas.",
      parameters: WorkloadScaleSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("workload_scale", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "workload_logs",
      label: "Workload Logs",
      description: "Get stdout/stderr logs from a workload.",
      parameters: WorkloadLogsSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("workload_logs", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    // ─── Observability Tools ───

    api.registerTool({
      name: "metrics_query",
      label: "Query Metrics",
      description: "Query GPU and workload metrics (utilization, memory, temperature).",
      parameters: MetricsQuerySchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("metrics_query", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "logs_search",
      label: "Search Logs",
      description: "Search logs across the cluster with text, level, and source filters.",
      parameters: LogsSearchSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("logs_search", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "alert_create",
      label: "Create Alert",
      description: "Create an alert rule that fires when a condition is met.",
      parameters: AlertCreateSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("alert_create", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "alert_list",
      label: "List Alerts",
      description: "List active alerts with optional severity filter.",
      parameters: AlertListSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("alert_list", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "alert_silence",
      label: "Silence Alert",
      description: "Silence an alert for a specified duration.",
      parameters: AlertSilenceSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("alert_silence", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    // ─── MOLT Marketplace Tools ───

    api.registerTool({
      name: "molt_offers",
      label: "MOLT Offers",
      description: "List available GPU capacity offers in the MOLT P2P marketplace.",
      parameters: MoltOffersSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("molt_offers", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "molt_offer_create",
      label: "Create MOLT Offer",
      description: "Offer your GPU capacity for rent on the MOLT marketplace.",
      parameters: MoltOfferCreateSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("molt_offer_create", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "molt_bid",
      label: "MOLT Bid",
      description: "Place a bid on a GPU capacity offer.",
      parameters: MoltBidSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("molt_bid", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "molt_spot_prices",
      label: "MOLT Spot Prices",
      description: "Get current spot prices for GPU types in the MOLT marketplace.",
      parameters: MoltSpotPricesSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("molt_spot_prices", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    // ─── Shutdown ───

    api.onShutdown(async () => {
      if (client) {
        client.shutdown();
        client = null;
      }
    });

    api.logger.info(`[clawbernetes] Registered ${21} tools`);
  },
};

export default clawbernetesPlugin;
