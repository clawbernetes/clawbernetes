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
  provider: Type.String({ description: "Provider identifier" }),
  gpu_count: Type.Number({ description: "Number of GPUs" }),
  gpu_model: Type.String({ description: "GPU model (H100, A100, etc.)" }),
  memory_gb: Type.Number({ description: "GPU memory in GB" }),
  price_per_hour: Type.Number({ description: "Price per hour (integer, e.g. cents)" }),
  reputation: Type.Optional(Type.Number({ description: "Provider reputation score (default 100)" })),
});

const MoltBidSchema = Type.Object({
  offer_id: Type.String({ description: "Offer ID to bid on" }),
  price_per_hour: Type.Number({ description: "Bid price per hour" }),
  duration_hours: Type.Number({ description: "Requested duration" }),
});

const MoltSpotPricesSchema = Type.Object({
  gpu_model: Type.Optional(Type.String({ description: "GPU model filter" })),
});

// ─── Auth Schemas ───

const UserCreateSchema = Type.Object({
  name: Type.String({ description: "Username" }),
  email: Type.Optional(Type.String({ description: "User email" })),
  roles: Type.Optional(Type.Array(Type.String(), { description: "Initial roles" })),
});

const UserGetSchema = Type.Object({
  user_id: Type.Optional(Type.String({ description: "User ID" })),
  name: Type.Optional(Type.String({ description: "Username" })),
});

const RoleAssignSchema = Type.Object({
  user_id: Type.String({ description: "User ID" }),
  role_name: Type.String({ description: "Role name to assign" }),
});

const PermissionCheckSchema = Type.Object({
  user_id: Type.String({ description: "User ID" }),
  action: Type.String({ description: "Action (read, write, admin)" }),
  resource: Type.String({ description: "Resource type" }),
});

const ApiKeyGenerateSchema = Type.Object({
  user_id: Type.String({ description: "User ID" }),
  name: Type.String({ description: "Key name" }),
  expires_in_days: Type.Optional(Type.Number({ description: "Expiration days" })),
});

// ─── Deploy Schemas ───

const DeployIntentSchema = Type.Object({
  intent: Type.String({ description: "Natural language deployment intent" }),
  namespace: Type.Optional(Type.String({ description: "Target namespace" })),
  dry_run: Type.Optional(Type.Boolean({ description: "Preview without executing" })),
});

const DeployStatusSchema = Type.Object({
  deployment_id: Type.String({ description: "Deployment ID" }),
});

// ─── Tenancy Schemas ───

const TenantCreateSchema = Type.Object({
  name: Type.String({ description: "Tenant name" }),
});

const NamespaceCreateSchema = Type.Object({
  tenant_id: Type.String({ description: "Parent tenant ID" }),
  name: Type.String({ description: "Namespace name" }),
});

const QuotaSetSchema = Type.Object({
  namespace_id: Type.String({ description: "Namespace ID" }),
  quota: Type.Object({
    max_gpus: Type.Optional(Type.Number()),
    gpu_hours: Type.Optional(Type.Number()),
    memory_mib: Type.Optional(Type.Number()),
  }),
});

// ─── Secrets Schemas ───

const SecretPutSchema = Type.Object({
  id: Type.String({ description: "Secret identifier" }),
  value: Type.String({ description: "Secret value" }),
  allowed_workloads: Type.Optional(Type.Array(Type.String(), { description: "Allowed workload IDs" })),
});

const SecretGetSchema = Type.Object({
  id: Type.String({ description: "Secret identifier" }),
  workload_id: Type.Optional(Type.String({ description: "Requesting workload" })),
  reason: Type.Optional(Type.String({ description: "Access reason (audit)" })),
});

// ─── PKI Schemas ───

const CertIssueSchema = Type.Object({
  common_name: Type.String({ description: "Certificate CN" }),
  dns_names: Type.Optional(Type.Array(Type.String(), { description: "DNS SANs" })),
  ip_addresses: Type.Optional(Type.Array(Type.String(), { description: "IP SANs" })),
  validity_days: Type.Optional(Type.Number({ description: "Validity period" })),
  server_auth: Type.Optional(Type.Boolean({ description: "Server auth (default true)" })),
  client_auth: Type.Optional(Type.Boolean({ description: "Client auth" })),
});

const CertGetSchema = Type.Object({
  cert_id: Type.String({ description: "Certificate ID (UUID)" }),
});

// ─── Service Discovery Schemas ───

const ServiceRegisterSchema = Type.Object({
  name: Type.String({ description: "Service name" }),
  namespace: Type.Optional(Type.String({ description: "Namespace" })),
  ports: Type.Array(Type.Object({
    port: Type.Number(),
    name: Type.Optional(Type.String()),
    protocol: Type.Optional(Type.String({ description: "http, tcp, grpc" })),
  })),
});

const EndpointAddSchema = Type.Object({
  namespace: Type.String({ description: "Namespace" }),
  service_name: Type.String({ description: "Service name" }),
  address: Type.String({ description: "IP address" }),
  port: Type.Number({ description: "Port number" }),
});

// ─── Storage Schemas ───

const VolumeProvisionSchema = Type.Object({
  id: Type.String({ description: "Volume ID" }),
  capacity_gb: Type.Number({ description: "Capacity in GB" }),
  storage_class: Type.Optional(Type.String({ description: "Storage class" })),
});

const ClaimCreateSchema = Type.Object({
  id: Type.String({ description: "Claim ID" }),
  requested_gb: Type.Number({ description: "Requested capacity" }),
});

// ─── Operations Schemas ───

const AutoscalePoolCreateSchema = Type.Object({
  id: Type.String({ description: "Pool ID" }),
  name: Type.String({ description: "Pool name" }),
  min_nodes: Type.Number({ description: "Minimum nodes" }),
  max_nodes: Type.Number({ description: "Maximum nodes" }),
  target_utilization: Type.Optional(Type.Number({ description: "Target GPU %" })),
});

const PreemptionRegisterSchema = Type.Object({
  workload_id: Type.String({ description: "Workload ID" }),
  priority_class: Type.Optional(Type.String({ description: "Priority class" })),
  gpus: Type.Optional(Type.Number({ description: "GPU count" })),
});

const RollbackRecordSchema = Type.Object({
  deployment_id: Type.String({ description: "Deployment ID" }),
  name: Type.String({ description: "Deployment name" }),
  image: Type.String({ description: "Container image" }),
});

// ─── Network Discovery Schemas ───

const NetworkScanSchema = Type.Object({
  subnet: Type.String({ description: "CIDR notation (e.g., 192.168.1.0/24)" }),
  ports: Type.Optional(Type.Array(Type.Number(), { description: "Ports to scan (default: 22, 80, 443, 8080)" })),
  timeout_ms: Type.Optional(Type.Number({ description: "Timeout per host in ms (default: 500)" })),
  detect_gpus: Type.Optional(Type.Boolean({ description: "Attempt GPU detection via SSH" })),
  credential_profile: Type.Optional(Type.String({ description: "Credential profile for SSH access" })),
});

const CredentialProfileCreateSchema = Type.Object({
  name: Type.String({ description: "Profile name" }),
  credential_type: Type.Optional(Type.String({ description: "Type: ssh, winrm, api" })),
  username: Type.Optional(Type.String({ description: "Username for SSH/WinRM" })),
  secret_ref: Type.Optional(Type.String({ description: "Secret ID containing key/password" })),
  auth_method: Type.Optional(Type.String({ description: "Auth method: key, password, agent" })),
  scope: Type.Optional(Type.Array(Type.String(), { description: "Subnets where profile applies" })),
  sudo: Type.Optional(Type.Boolean({ description: "Use sudo (default: true)" })),
});

const NodeTokenCreateSchema = Type.Object({
  hostname: Type.String({ description: "Hostname for the joining node" }),
  ttl_minutes: Type.Optional(Type.Number({ description: "Token TTL in minutes (default: 15)" })),
  labels: Type.Optional(Type.Record(Type.String(), Type.String(), { description: "Labels for the node" })),
  auto_approve: Type.Optional(Type.Boolean({ description: "Auto-approve on trusted subnet (default: true)" })),
});

const TrustedSubnetAddSchema = Type.Object({
  subnet: Type.String({ description: "CIDR notation for trusted subnet" }),
});

const CheckTrustedSchema = Type.Object({
  address: Type.String({ description: "IP address to check" }),
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

    // ─── Auth Tools ───

    api.registerTool({
      name: "user_create",
      label: "Create User",
      description: "Create a new user with optional roles",
      parameters: UserCreateSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("user_create", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "user_get",
      label: "Get User",
      description: "Get user details by ID or name",
      parameters: UserGetSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("user_get", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "user_list",
      label: "List Users",
      description: "List all users",
      parameters: Type.Object({}),
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("user_list", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "role_assign",
      label: "Assign Role",
      description: "Assign a role to a user",
      parameters: RoleAssignSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("role_assign", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "role_list",
      label: "List Roles",
      description: "List all available roles",
      parameters: Type.Object({}),
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("role_list", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "permission_check",
      label: "Check Permission",
      description: "Check if a user has permission for an action",
      parameters: PermissionCheckSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("permission_check", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "api_key_generate",
      label: "Generate API Key",
      description: "Generate a new API key for a user",
      parameters: ApiKeyGenerateSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("api_key_generate", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    // ─── Deploy Tools ───

    api.registerTool({
      name: "deploy_intent",
      label: "Deploy (Intent)",
      description: "Deploy using natural language (e.g., 'deploy pytorch:2.0 with 4 GPUs using canary')",
      parameters: DeployIntentSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("deploy_intent", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "deploy_status",
      label: "Deployment Status",
      description: "Get deployment status",
      parameters: DeployStatusSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("deploy_status", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "deploy_list",
      label: "List Deployments",
      description: "List all deployments",
      parameters: Type.Object({}),
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("deploy_list", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    // ─── Tenancy Tools ───

    api.registerTool({
      name: "tenant_create",
      label: "Create Tenant",
      description: "Create a new tenant",
      parameters: TenantCreateSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("tenant_create", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "tenant_list",
      label: "List Tenants",
      description: "List all tenants",
      parameters: Type.Object({}),
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("tenant_list", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "namespace_create",
      label: "Create Namespace",
      description: "Create a namespace within a tenant",
      parameters: NamespaceCreateSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("namespace_create", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "namespace_list",
      label: "List Namespaces",
      description: "List namespaces",
      parameters: Type.Object({}),
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("namespace_list", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "quota_set",
      label: "Set Quota",
      description: "Set resource quota for a namespace",
      parameters: QuotaSetSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("quota_set", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    // ─── Secrets Tools ───

    api.registerTool({
      name: "secret_put",
      label: "Store Secret",
      description: "Store an encrypted secret",
      parameters: SecretPutSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("secret_put", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "secret_get",
      label: "Get Secret",
      description: "Retrieve a secret value",
      parameters: SecretGetSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("secret_get", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "secret_list",
      label: "List Secrets",
      description: "List all secret IDs",
      parameters: Type.Object({}),
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("secret_list", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    // ─── PKI Tools ───

    api.registerTool({
      name: "cert_issue",
      label: "Issue Certificate",
      description: "Issue a new TLS certificate",
      parameters: CertIssueSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("cert_issue", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "cert_get",
      label: "Get Certificate",
      description: "Get certificate details",
      parameters: CertGetSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("cert_get", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "cert_list",
      label: "List Certificates",
      description: "List all certificates",
      parameters: Type.Object({}),
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("cert_list", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "ca_status",
      label: "CA Status",
      description: "Get Certificate Authority status",
      parameters: Type.Object({}),
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("ca_status", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    // ─── Service Discovery Tools ───

    api.registerTool({
      name: "service_register",
      label: "Register Service",
      description: "Register a service for discovery",
      parameters: ServiceRegisterSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("service_register", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "service_list",
      label: "List Services",
      description: "List registered services",
      parameters: Type.Object({}),
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("service_list", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "endpoint_add",
      label: "Add Endpoint",
      description: "Add an endpoint to a service",
      parameters: EndpointAddSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("endpoint_add", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    // ─── Storage Tools ───

    api.registerTool({
      name: "volume_provision",
      label: "Provision Volume",
      description: "Provision a new storage volume",
      parameters: VolumeProvisionSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("volume_provision", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "volume_list",
      label: "List Volumes",
      description: "List storage volumes",
      parameters: Type.Object({}),
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("volume_list", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "claim_create",
      label: "Create Claim",
      description: "Create a volume claim",
      parameters: ClaimCreateSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("claim_create", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    // ─── Autoscaling Tools ───

    api.registerTool({
      name: "autoscale_pool_create",
      label: "Create Autoscale Pool",
      description: "Create an autoscaling node pool",
      parameters: AutoscalePoolCreateSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("autoscale_pool_create", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "autoscale_status",
      label: "Autoscaler Status",
      description: "Get autoscaler status",
      parameters: Type.Object({}),
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("autoscale_status", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "autoscale_evaluate",
      label: "Evaluate Autoscaling",
      description: "Evaluate and get scaling recommendations",
      parameters: Type.Object({}),
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("autoscale_evaluate", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    // ─── Preemption Tools ───

    api.registerTool({
      name: "preemption_register",
      label: "Register for Preemption",
      description: "Register a workload for preemption tracking",
      parameters: PreemptionRegisterSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("preemption_register", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "preemption_list",
      label: "List Preemptible",
      description: "List preemptible workloads",
      parameters: Type.Object({}),
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("preemption_list", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    // ─── Rollback Tools ───

    api.registerTool({
      name: "rollback_record",
      label: "Record Deployment",
      description: "Record a deployment snapshot for rollback",
      parameters: RollbackRecordSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("rollback_record", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "rollback_history",
      label: "Rollback History",
      description: "Get rollback history",
      parameters: Type.Object({}),
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("rollback_history", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    // ─── Network Discovery Tools ───

    api.registerTool({
      name: "network_scan",
      label: "Network Scan",
      description: "Scan a subnet to discover hosts, open ports, and potential GPU nodes",
      parameters: NetworkScanSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("network_scan", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "credential_profile_create",
      label: "Create Credential Profile",
      description: "Create a reusable credential profile for SSH/WinRM access to nodes",
      parameters: CredentialProfileCreateSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("credential_profile_create", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "credential_profile_list",
      label: "List Credential Profiles",
      description: "List all credential profiles",
      parameters: Type.Object({}),
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("credential_profile_list", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "node_token_create",
      label: "Create Node Token",
      description: "Generate a one-time bootstrap token for a node to join the cluster",
      parameters: NodeTokenCreateSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("node_token_create", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "trusted_subnet_add",
      label: "Add Trusted Subnet",
      description: "Add a subnet to the trusted list for auto-approval of joining nodes",
      parameters: TrustedSubnetAddSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("trusted_subnet_add", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "trusted_subnet_list",
      label: "List Trusted Subnets",
      description: "List all trusted subnets for auto-approval",
      parameters: Type.Object({}),
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("trusted_subnet_list", params || {});
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    api.registerTool({
      name: "check_trusted",
      label: "Check Trusted",
      description: "Check if an IP address is in a trusted subnet",
      parameters: CheckTrustedSchema,
      async execute(_id: string, params: unknown) {
        try {
          const result = await callBridge("check_trusted", params);
          return toolResult(result);
        } catch (err) {
          return toolError(err);
        }
      },
    });

    // Note: Client cleanup happens automatically when process exits
    api.logger.info(`[clawbernetes] Registered 62 tools`);
  },
};

export default clawbernetesPlugin;
