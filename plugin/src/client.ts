/**
 * Clawbernetes Client
 *
 * Communicates with the claw-bridge Rust binary via JSON-RPC over stdio.
 * The bridge provides access to all claw-* Rust crates.
 */

import { spawn, ChildProcess } from "node:child_process";
import { createInterface, Interface } from "node:readline";
import { EventEmitter } from "node:events";
import path from "node:path";

import type {
  ClawbernetesConfig,
  ClusterNode,
  ClusterStatus,
  Workload,
  WorkloadSpec,
  MetricSeries,
  LogEntry,
  Alert,
  AlertSeverity,
  MoltOffer,
  MoltBid,
} from "./types.js";

// ─────────────────────────────────────────────────────────────
// JSON-RPC Types
// ─────────────────────────────────────────────────────────────

interface RpcRequest {
  id: number;
  method: string;
  params: unknown;
}

interface RpcResponse {
  id: number;
  result?: unknown;
  error?: {
    code: number;
    message: string;
  };
}

type PendingRequest = {
  resolve: (result: unknown) => void;
  reject: (error: Error) => void;
};

// ─────────────────────────────────────────────────────────────
// Bridge Process Manager
// ─────────────────────────────────────────────────────────────

class BridgeProcess extends EventEmitter {
  private process: ChildProcess | null = null;
  private readline: Interface | null = null;
  private requestId = 0;
  private pending = new Map<number, PendingRequest>();
  private bridgePath: string;

  constructor(bridgePath?: string) {
    super();
    // Default to looking for claw-bridge in PATH or relative to this module
    this.bridgePath = bridgePath ?? this.findBridge();
  }

  private findBridge(): string {
    // Try common locations
    const candidates = [
      "claw-bridge", // In PATH
      path.join(process.cwd(), "target", "release", "claw-bridge"),
      path.join(process.cwd(), "target", "debug", "claw-bridge"),
      path.join(__dirname, "..", "..", "target", "release", "claw-bridge"),
      path.join(__dirname, "..", "..", "target", "debug", "claw-bridge"),
    ];
    // For now, just return the first one (will error if not found)
    return candidates[0];
  }

  async start(): Promise<void> {
    if (this.process) {
      return;
    }

    return new Promise((resolve, reject) => {
      this.process = spawn(this.bridgePath, [], {
        stdio: ["pipe", "pipe", "pipe"],
        env: { ...process.env, RUST_LOG: "claw_bridge=info" },
      });

      this.process.on("error", (err) => {
        this.emit("error", err);
        reject(err);
      });

      this.process.on("exit", (code) => {
        this.emit("exit", code);
        this.cleanup();
      });

      if (this.process.stdout) {
        this.readline = createInterface({
          input: this.process.stdout,
          crlfDelay: Infinity,
        });

        this.readline.on("line", (line) => {
          this.handleResponse(line);
        });
      }

      if (this.process.stderr) {
        this.process.stderr.on("data", (data) => {
          // Log bridge stderr for debugging
          console.error(`[claw-bridge] ${data.toString().trim()}`);
        });
      }

      // Give the process a moment to start
      setTimeout(() => resolve(), 100);
    });
  }

  stop(): void {
    if (this.process) {
      this.process.kill();
      this.cleanup();
    }
  }

  private cleanup(): void {
    this.process = null;
    this.readline = null;
    // Reject all pending requests
    for (const [id, pending] of this.pending) {
      pending.reject(new Error("Bridge process terminated"));
      this.pending.delete(id);
    }
  }

  private handleResponse(line: string): void {
    try {
      const response = JSON.parse(line) as RpcResponse;
      const pending = this.pending.get(response.id);
      if (pending) {
        this.pending.delete(response.id);
        if (response.error) {
          pending.reject(new Error(`${response.error.message} (code: ${response.error.code})`));
        } else {
          pending.resolve(response.result);
        }
      }
    } catch (err) {
      console.error("[claw-bridge] Failed to parse response:", line);
    }
  }

  async call<T>(method: string, params: unknown = {}): Promise<T> {
    if (!this.process || !this.process.stdin) {
      await this.start();
    }

    return new Promise((resolve, reject) => {
      const id = ++this.requestId;
      const request: RpcRequest = { id, method, params };

      this.pending.set(id, {
        resolve: resolve as (result: unknown) => void,
        reject,
      });

      const json = JSON.stringify(request) + "\n";
      this.process!.stdin!.write(json, (err) => {
        if (err) {
          this.pending.delete(id);
          reject(err);
        }
      });

      // Timeout after 30 seconds
      setTimeout(() => {
        if (this.pending.has(id)) {
          this.pending.delete(id);
          reject(new Error(`Request timeout: ${method}`));
        }
      }, 30000);
    });
  }
}

// ─────────────────────────────────────────────────────────────
// Clawbernetes Client
// ─────────────────────────────────────────────────────────────

export class ClawbernetesClient {
  private config: ClawbernetesConfig;
  private bridge: BridgeProcess;

  constructor(config: ClawbernetesConfig = {}) {
    this.config = config;
    this.bridge = new BridgeProcess();
  }

  async initialize(): Promise<void> {
    await this.bridge.start();
  }

  shutdown(): void {
    this.bridge.stop();
  }

  // ─────────────────────────────────────────────────────────────
  // Cluster Operations
  // ─────────────────────────────────────────────────────────────

  async getClusterStatus(): Promise<ClusterStatus> {
    return this.bridge.call("cluster_status", {
      cluster: this.config.defaultCluster,
    });
  }

  async listNodes(filters?: {
    status?: string;
    labels?: Record<string, string>;
  }): Promise<ClusterNode[]> {
    return this.bridge.call("node_list", filters ?? {});
  }

  async getNode(nodeId: string): Promise<ClusterNode | null> {
    try {
      return await this.bridge.call("node_get", { node_id: nodeId });
    } catch (err) {
      if (String(err).includes("not found")) {
        return null;
      }
      throw err;
    }
  }

  async drainNode(
    nodeId: string,
    options?: { gracePeriodSeconds?: number; force?: boolean }
  ): Promise<{ success: boolean; migratedWorkloads: number }> {
    return this.bridge.call("node_drain", {
      node_id: nodeId,
      grace_period_seconds: options?.gracePeriodSeconds,
      force: options?.force,
    });
  }

  async cordonNode(nodeId: string): Promise<{ success: boolean }> {
    return this.bridge.call("node_cordon", { node_id: nodeId });
  }

  async uncordonNode(nodeId: string): Promise<{ success: boolean }> {
    return this.bridge.call("node_uncordon", { node_id: nodeId });
  }

  // ─────────────────────────────────────────────────────────────
  // Workload Operations
  // ─────────────────────────────────────────────────────────────

  async submitWorkload(spec: WorkloadSpec): Promise<Workload> {
    return this.bridge.call("workload_submit", spec);
  }

  async getWorkload(workloadId: string): Promise<Workload | null> {
    try {
      return await this.bridge.call("workload_get", { workload_id: workloadId });
    } catch (err) {
      if (String(err).includes("not found")) {
        return null;
      }
      throw err;
    }
  }

  async listWorkloads(filters?: {
    state?: string;
    labels?: Record<string, string>;
    nodeId?: string;
  }): Promise<Workload[]> {
    return this.bridge.call("workload_list", {
      state: filters?.state,
      node_id: filters?.nodeId,
      labels: filters?.labels,
    });
  }

  async stopWorkload(
    workloadId: string,
    options?: { gracePeriodSeconds?: number; force?: boolean }
  ): Promise<{ success: boolean }> {
    return this.bridge.call("workload_stop", {
      workload_id: workloadId,
      grace_period_seconds: options?.gracePeriodSeconds,
      force: options?.force,
    });
  }

  async scaleWorkload(
    workloadId: string,
    replicas: number
  ): Promise<{ success: boolean; previousReplicas: number }> {
    return this.bridge.call("workload_scale", {
      workload_id: workloadId,
      replicas,
    });
  }

  // ─────────────────────────────────────────────────────────────
  // Observability
  // ─────────────────────────────────────────────────────────────

  async queryMetrics(query: {
    name: string;
    startTime?: number;
    endTime?: number;
    step?: number;
    labels?: Record<string, string>;
  }): Promise<MetricSeries[]> {
    return this.bridge.call("metrics_query", {
      name: query.name,
      start_time: query.startTime,
      end_time: query.endTime,
      step: query.step,
      labels: query.labels,
    });
  }

  async searchLogs(query: {
    text?: string;
    level?: string;
    workloadId?: string;
    nodeId?: string;
    startTime?: number;
    endTime?: number;
    limit?: number;
  }): Promise<LogEntry[]> {
    return this.bridge.call("logs_search", {
      text: query.text,
      level: query.level,
      workload_id: query.workloadId,
      node_id: query.nodeId,
      start_time: query.startTime,
      end_time: query.endTime,
      limit: query.limit,
    });
  }

  async createAlert(alert: {
    name: string;
    severity: AlertSeverity;
    condition: string;
    message: string;
    labels?: Record<string, string>;
  }): Promise<Alert> {
    return this.bridge.call("alert_create", alert);
  }

  async listAlerts(filters?: {
    severity?: AlertSeverity;
    resolved?: boolean;
  }): Promise<Alert[]> {
    return this.bridge.call("alert_list", filters ?? {});
  }

  async silenceAlert(
    alertId: string,
    durationSeconds: number
  ): Promise<{ success: boolean }> {
    return this.bridge.call("alert_silence", {
      alert_id: alertId,
      duration_seconds: durationSeconds,
    });
  }

  // ─────────────────────────────────────────────────────────────
  // MOLT Marketplace
  // ─────────────────────────────────────────────────────────────

  async listMoltOffers(filters?: {
    minGpus?: number;
    maxPricePerHour?: number;
    region?: string;
    gpuModel?: string;
  }): Promise<MoltOffer[]> {
    return this.bridge.call("molt_offers", {
      min_gpus: filters?.minGpus,
      max_price_per_hour: filters?.maxPricePerHour,
      region: filters?.region,
      gpu_model: filters?.gpuModel,
    });
  }

  async createMoltOffer(offer: {
    gpus: number;
    gpuModel: string;
    pricePerHour: number;
    minDurationHours?: number;
    maxDurationHours?: number;
  }): Promise<MoltOffer> {
    return this.bridge.call("molt_offer_create", {
      gpus: offer.gpus,
      gpu_model: offer.gpuModel,
      price_per_hour: offer.pricePerHour,
      min_duration_hours: offer.minDurationHours,
      max_duration_hours: offer.maxDurationHours,
    });
  }

  async placeMoltBid(bid: {
    offerId: string;
    pricePerHour: number;
    durationHours: number;
  }): Promise<MoltBid> {
    return this.bridge.call("molt_bid", {
      offer_id: bid.offerId,
      price_per_hour: bid.pricePerHour,
      duration_hours: bid.durationHours,
    });
  }

  // ─────────────────────────────────────────────────────────────
  // Cost
  // ─────────────────────────────────────────────────────────────

  async getCostReport(query: {
    startTime?: number;
    endTime?: number;
    groupBy?: "node" | "workload" | "label";
  }): Promise<{
    totalCost: number;
    breakdown: Array<{ key: string; cost: number; gpuHours: number }>;
  }> {
    // TODO: Implement when billing is wired up
    void query;
    return { totalCost: 0, breakdown: [] };
  }

  async getSpotPrices(filters?: {
    region?: string;
    gpuModel?: string;
  }): Promise<Array<{ region: string; gpuModel: string; pricePerHour: number }>> {
    return this.bridge.call("molt_spot_prices", {
      region: filters?.region,
      gpu_model: filters?.gpuModel,
    });
  }
}

// Singleton for convenience
let defaultClient: ClawbernetesClient | null = null;

export function getClient(config?: ClawbernetesConfig): ClawbernetesClient {
  if (!defaultClient || config) {
    defaultClient = new ClawbernetesClient(config);
  }
  return defaultClient;
}
