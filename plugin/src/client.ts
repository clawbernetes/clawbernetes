/**
 * Clawbernetes Client
 *
 * Interfaces with the claw-* Rust crates. This is the bridge layer that will
 * eventually use FFI, gRPC, or embedded Rust via napi-rs.
 *
 * For now, this provides the interface contract with mock implementations
 * that will be replaced with real crate bindings.
 */

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

export class ClawbernetesClient {
  private config: ClawbernetesConfig;

  constructor(config: ClawbernetesConfig = {}) {
    this.config = config;
  }

  // ─────────────────────────────────────────────────────────────
  // Cluster Operations (claw-gateway-server, claw-discovery)
  // ─────────────────────────────────────────────────────────────

  async getClusterStatus(): Promise<ClusterStatus> {
    // TODO: Call claw-gateway-server via FFI/gRPC
    return {
      name: this.config.defaultCluster ?? "default",
      healthy: true,
      nodes: { total: 0, ready: 0, notReady: 0 },
      gpus: { total: 0, available: 0, allocated: 0 },
      workloads: { running: 0, pending: 0, failed: 0 },
    };
  }

  async listNodes(filters?: {
    status?: string;
    labels?: Record<string, string>;
  }): Promise<ClusterNode[]> {
    // TODO: Call claw-discovery via FFI/gRPC
    void filters;
    return [];
  }

  async getNode(nodeId: string): Promise<ClusterNode | null> {
    // TODO: Call claw-discovery via FFI/gRPC
    void nodeId;
    return null;
  }

  async drainNode(
    nodeId: string,
    options?: { gracePeriodSeconds?: number; force?: boolean }
  ): Promise<{ success: boolean; migratedWorkloads: number }> {
    // TODO: Call claw-scheduler via FFI/gRPC
    void nodeId;
    void options;
    return { success: true, migratedWorkloads: 0 };
  }

  async cordonNode(nodeId: string): Promise<{ success: boolean }> {
    // TODO: Call claw-discovery via FFI/gRPC
    void nodeId;
    return { success: true };
  }

  async uncordonNode(nodeId: string): Promise<{ success: boolean }> {
    // TODO: Call claw-discovery via FFI/gRPC
    void nodeId;
    return { success: true };
  }

  // ─────────────────────────────────────────────────────────────
  // Workload Operations (claw-scheduler, claw-runtime)
  // ─────────────────────────────────────────────────────────────

  async submitWorkload(spec: WorkloadSpec): Promise<Workload> {
    // TODO: Call claw-scheduler via FFI/gRPC
    return {
      id: `wl-${Date.now()}`,
      spec,
      state: "pending",
      createdAt: Date.now(),
    };
  }

  async getWorkload(workloadId: string): Promise<Workload | null> {
    // TODO: Call claw-scheduler via FFI/gRPC
    void workloadId;
    return null;
  }

  async listWorkloads(filters?: {
    state?: string;
    labels?: Record<string, string>;
    nodeId?: string;
  }): Promise<Workload[]> {
    // TODO: Call claw-scheduler via FFI/gRPC
    void filters;
    return [];
  }

  async stopWorkload(
    workloadId: string,
    options?: { gracePeriodSeconds?: number; force?: boolean }
  ): Promise<{ success: boolean }> {
    // TODO: Call claw-scheduler via FFI/gRPC
    void workloadId;
    void options;
    return { success: true };
  }

  async scaleWorkload(
    workloadId: string,
    replicas: number
  ): Promise<{ success: boolean; previousReplicas: number }> {
    // TODO: Call claw-scheduler via FFI/gRPC
    void workloadId;
    void replicas;
    return { success: true, previousReplicas: 1 };
  }

  // ─────────────────────────────────────────────────────────────
  // Observability (claw-metrics, claw-logs, claw-alerts)
  // ─────────────────────────────────────────────────────────────

  async queryMetrics(query: {
    name: string;
    startTime?: number;
    endTime?: number;
    step?: number;
    labels?: Record<string, string>;
  }): Promise<MetricSeries[]> {
    // TODO: Call claw-metrics via FFI/gRPC
    void query;
    return [];
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
    // TODO: Call claw-logs via FFI/gRPC
    void query;
    return [];
  }

  async createAlert(alert: {
    name: string;
    severity: AlertSeverity;
    condition: string;
    message: string;
    labels?: Record<string, string>;
  }): Promise<Alert> {
    // TODO: Call claw-alerts via FFI/gRPC
    return {
      id: `alert-${Date.now()}`,
      name: alert.name,
      severity: alert.severity,
      message: alert.message,
      source: "clawbernetes-plugin",
      firedAt: Date.now(),
      labels: alert.labels,
    };
  }

  async listAlerts(filters?: {
    severity?: AlertSeverity;
    resolved?: boolean;
  }): Promise<Alert[]> {
    // TODO: Call claw-alerts via FFI/gRPC
    void filters;
    return [];
  }

  async silenceAlert(
    alertId: string,
    durationSeconds: number
  ): Promise<{ success: boolean }> {
    // TODO: Call claw-alerts via FFI/gRPC
    void alertId;
    void durationSeconds;
    return { success: true };
  }

  // ─────────────────────────────────────────────────────────────
  // Security (claw-secrets, claw-pki, claw-auth)
  // ─────────────────────────────────────────────────────────────

  async getSecret(
    secretId: string,
    accessor: string,
    reason: string
  ): Promise<{ value: string } | null> {
    // TODO: Call claw-secrets via FFI/gRPC
    void secretId;
    void accessor;
    void reason;
    return null;
  }

  async putSecret(
    secretId: string,
    value: string,
    policy?: { allowedAccessors?: string[]; expiresAt?: number }
  ): Promise<{ success: boolean }> {
    // TODO: Call claw-secrets via FFI/gRPC
    void secretId;
    void value;
    void policy;
    return { success: true };
  }

  async issueCertificate(request: {
    commonName: string;
    dnsNames?: string[];
    validityDays?: number;
  }): Promise<{ cert: string; key: string; ca: string }> {
    // TODO: Call claw-pki via FFI/gRPC
    void request;
    return { cert: "", key: "", ca: "" };
  }

  // ─────────────────────────────────────────────────────────────
  // MOLT Marketplace (claw-molt)
  // ─────────────────────────────────────────────────────────────

  async listMoltOffers(filters?: {
    minGpus?: number;
    maxPricePerHour?: number;
    region?: string;
    gpuModel?: string;
  }): Promise<MoltOffer[]> {
    // TODO: Call claw-molt via FFI/gRPC
    void filters;
    return [];
  }

  async createMoltOffer(offer: {
    gpus: number;
    gpuModel: string;
    pricePerHour: number;
    minDurationHours?: number;
    maxDurationHours?: number;
  }): Promise<MoltOffer> {
    // TODO: Call claw-molt via FFI/gRPC
    return {
      id: `offer-${Date.now()}`,
      nodeId: this.config.molt?.nodeId ?? "unknown",
      gpus: offer.gpus,
      gpuModel: offer.gpuModel,
      pricePerHour: offer.pricePerHour,
      minDurationHours: offer.minDurationHours,
      maxDurationHours: offer.maxDurationHours,
      region: this.config.molt?.region ?? "unknown",
      availableAt: Date.now(),
    };
  }

  async placeMoltBid(bid: {
    offerId: string;
    pricePerHour: number;
    durationHours: number;
  }): Promise<MoltBid> {
    // TODO: Call claw-molt via FFI/gRPC
    return {
      id: `bid-${Date.now()}`,
      offerId: bid.offerId,
      bidderId: "self",
      pricePerHour: bid.pricePerHour,
      durationHours: bid.durationHours,
      status: "pending",
      createdAt: Date.now(),
    };
  }

  // ─────────────────────────────────────────────────────────────
  // Cost (claw-billing)
  // ─────────────────────────────────────────────────────────────

  async getCostReport(query: {
    startTime?: number;
    endTime?: number;
    groupBy?: "node" | "workload" | "label";
  }): Promise<{
    totalCost: number;
    breakdown: Array<{ key: string; cost: number; gpuHours: number }>;
  }> {
    // TODO: Call claw-billing via FFI/gRPC
    void query;
    return { totalCost: 0, breakdown: [] };
  }

  async getSpotPrices(filters?: {
    region?: string;
    gpuModel?: string;
  }): Promise<Array<{ region: string; gpuModel: string; pricePerHour: number }>> {
    // TODO: Call claw-billing + claw-molt via FFI/gRPC
    void filters;
    return [];
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
