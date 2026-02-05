/**
 * Clawbernetes OpenClaw Plugin Types
 *
 * These types mirror OpenClaw's tool interface for plugin compatibility.
 */

import type { TSchema } from "@sinclair/typebox";

/** Result from a tool execution */
export type ToolResult = {
  type: "text" | "json" | "error" | "image";
  content: string;
  mimeType?: string;
};

/** OpenClaw-compatible tool interface */
export type ClawbernnetesTool = {
  name: string;
  label?: string;
  description: string;
  parameters: TSchema;
  execute: (
    toolCallId: string,
    args: Record<string, unknown>,
    context?: ToolContext
  ) => Promise<ToolResult>;
};

/** Context passed to tool execution */
export type ToolContext = {
  sessionKey?: string;
  agentId?: string;
  workspaceDir?: string;
  config?: ClawbernetesConfig;
};

/** Plugin configuration */
export type ClawbernetesConfig = {
  /** Path to the claw-bridge binary */
  bridgePath?: string;
  /** Gateway connection mode */
  mode?: "embedded" | "remote";
  /** Remote gateway URL (when mode=remote) */
  gatewayUrl?: string;
  /** Authentication token */
  token?: string;
  /** Default cluster name */
  defaultCluster?: string;
  /** MOLT marketplace settings */
  molt?: {
    enabled?: boolean;
    nodeId?: string;
    region?: string;
  };
};

/** OpenClaw plugin manifest */
export type OpenClawPlugin = {
  id: string;
  name: string;
  version: string;
  description: string;
  tools: (context: ToolContext) => ClawbernnetesTool[];
};

// ─────────────────────────────────────────────────────────────
// Domain Types (from claw-* crates)
// ─────────────────────────────────────────────────────────────

export type NodeStatus = "ready" | "not_ready" | "cordoned" | "draining" | "offline";

export type GpuInfo = {
  id: string;
  model: string;
  vendor: "nvidia" | "amd" | "apple" | "intel";
  memoryMb: number;
  computeCapability?: string;
  utilizationPercent?: number;
  temperatureCelsius?: number;
};

export type ClusterNode = {
  id: string;
  name: string;
  status: NodeStatus;
  labels: Record<string, string>;
  gpus: GpuInfo[];
  cpuCores: number;
  memoryMb: number;
  platform: string;
  region?: string;
  zone?: string;
  connectedAt: number;
  lastHeartbeat: number;
};

export type ClusterStatus = {
  name: string;
  healthy: boolean;
  nodes: {
    total: number;
    ready: number;
    notReady: number;
  };
  gpus: {
    total: number;
    available: number;
    allocated: number;
  };
  workloads: {
    running: number;
    pending: number;
    failed: number;
  };
};

export type WorkloadState =
  | "pending"
  | "scheduled"
  | "running"
  | "succeeded"
  | "failed"
  | "cancelled";

export type WorkloadSpec = {
  name: string;
  image: string;
  command?: string[];
  args?: string[];
  env?: Record<string, string>;
  gpus: number;
  gpuMemoryMb?: number;
  cpuCores?: number;
  memoryMb?: number;
  priority?: number;
  preemptible?: boolean;
  maxRuntimeSeconds?: number;
  labels?: Record<string, string>;
};

export type Workload = {
  id: string;
  spec: WorkloadSpec;
  state: WorkloadState;
  nodeId?: string;
  createdAt: number;
  startedAt?: number;
  finishedAt?: number;
  exitCode?: number;
  error?: string;
};

export type MetricPoint = {
  timestamp: number;
  value: number;
  labels?: Record<string, string>;
};

export type MetricSeries = {
  name: string;
  points: MetricPoint[];
};

export type LogEntry = {
  timestamp: number;
  level: "debug" | "info" | "warn" | "error";
  message: string;
  source?: string;
  workloadId?: string;
  nodeId?: string;
  fields?: Record<string, unknown>;
};

export type AlertSeverity = "info" | "warning" | "critical";

export type Alert = {
  id: string;
  name: string;
  severity: AlertSeverity;
  message: string;
  source: string;
  firedAt: number;
  resolvedAt?: number;
  labels?: Record<string, string>;
};

export type MoltOffer = {
  id: string;
  nodeId: string;
  gpus: number;
  gpuModel: string;
  pricePerHour: number;
  minDurationHours?: number;
  maxDurationHours?: number;
  region: string;
  availableAt: number;
  expiresAt?: number;
};

export type MoltBid = {
  id: string;
  offerId: string;
  bidderId: string;
  pricePerHour: number;
  durationHours: number;
  status: "pending" | "accepted" | "rejected" | "expired";
  createdAt: number;
};
