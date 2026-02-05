/**
 * Observability Tools
 *
 * Tools for metrics, logs, and alerts.
 */

import { Type } from "@sinclair/typebox";
import type { ClawbernnetesTool, ToolResult, AlertSeverity } from "../types.js";
import { getClient } from "../client.js";

function jsonResult(data: unknown): ToolResult {
  return { type: "json", content: JSON.stringify(data, null, 2) };
}

function errorResult(message: string): ToolResult {
  return { type: "error", content: message };
}

// ─────────────────────────────────────────────────────────────
// metrics_query
// ─────────────────────────────────────────────────────────────

const MetricsQuerySchema = Type.Object({
  metric: Type.String({
    description:
      "Metric name (e.g., gpu_utilization, gpu_memory_used, gpu_temperature, workload_duration)",
  }),
  nodeId: Type.Optional(Type.String({ description: "Filter by node ID" })),
  workloadId: Type.Optional(Type.String({ description: "Filter by workload ID" })),
  labels: Type.Optional(
    Type.Record(Type.String(), Type.String(), { description: "Additional label filters" })
  ),
  startTime: Type.Optional(Type.String({ description: "Start time (ISO 8601 or relative: -1h, -24h)" })),
  endTime: Type.Optional(Type.String({ description: "End time (ISO 8601 or 'now')" })),
  step: Type.Optional(Type.String({ description: "Resolution step (e.g., 1m, 5m, 1h)" })),
});

function parseTimeArg(value: string | undefined, defaultMs: number): number {
  if (!value) return defaultMs;
  if (value === "now") return Date.now();
  if (value.startsWith("-")) {
    const match = value.match(/^-(\d+)([smhd])$/);
    if (match) {
      const num = parseInt(match[1], 10);
      const unit = match[2];
      const ms = { s: 1000, m: 60000, h: 3600000, d: 86400000 }[unit] ?? 1000;
      return Date.now() - num * ms;
    }
  }
  return new Date(value).getTime() || defaultMs;
}

function parseStepMs(step: string | undefined): number {
  if (!step) return 60000; // default 1m
  const match = step.match(/^(\d+)([smh])$/);
  if (match) {
    const num = parseInt(match[1], 10);
    const unit = match[2];
    return num * ({ s: 1000, m: 60000, h: 3600000 }[unit] ?? 60000);
  }
  return 60000;
}

export function createMetricsQueryTool(): ClawbernnetesTool {
  return {
    name: "metrics_query",
    label: "Clawbernetes",
    description:
      "Query metrics from the cluster. Supports GPU utilization, memory, temperature, and workload metrics.",
    parameters: MetricsQuerySchema,
    execute: async (_id, args, context) => {
      try {
        const client = getClient(context?.config);
        const params = args as {
          metric: string;
          nodeId?: string;
          workloadId?: string;
          labels?: Record<string, string>;
          startTime?: string;
          endTime?: string;
          step?: string;
        };

        const labels: Record<string, string> = { ...params.labels };
        if (params.nodeId) labels.node_id = params.nodeId;
        if (params.workloadId) labels.workload_id = params.workloadId;

        const series = await client.queryMetrics({
          name: params.metric,
          startTime: parseTimeArg(params.startTime, Date.now() - 3600000),
          endTime: parseTimeArg(params.endTime, Date.now()),
          step: parseStepMs(params.step),
          labels,
        });

        return jsonResult({
          metric: params.metric,
          series: series.map((s) => ({
            name: s.name,
            points: s.points.slice(-20).map((p) => ({
              time: new Date(p.timestamp).toISOString(),
              value: p.value,
            })),
            summary: s.points.length > 0 ? {
              min: Math.min(...s.points.map((p) => p.value)),
              max: Math.max(...s.points.map((p) => p.value)),
              avg: s.points.reduce((a, b) => a + b.value, 0) / s.points.length,
              latest: s.points[s.points.length - 1]?.value,
            } : null,
          })),
        });
      } catch (err) {
        return errorResult(`Failed to query metrics: ${err}`);
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────
// logs_search
// ─────────────────────────────────────────────────────────────

const LogsSearchSchema = Type.Object({
  query: Type.Optional(Type.String({ description: "Full-text search query" })),
  level: Type.Optional(Type.String({ description: "Log level: debug, info, warn, error" })),
  nodeId: Type.Optional(Type.String({ description: "Filter by node ID" })),
  workloadId: Type.Optional(Type.String({ description: "Filter by workload ID" })),
  startTime: Type.Optional(Type.String({ description: "Start time (ISO 8601 or relative)" })),
  endTime: Type.Optional(Type.String({ description: "End time (ISO 8601 or 'now')" })),
  limit: Type.Optional(Type.Number({ description: "Max results (default: 100)", default: 100 })),
});

export function createLogsSearchTool(): ClawbernnetesTool {
  return {
    name: "logs_search",
    label: "Clawbernetes",
    description: "Search logs across the cluster with filtering by level, node, workload, or text.",
    parameters: LogsSearchSchema,
    execute: async (_id, args, context) => {
      try {
        const client = getClient(context?.config);
        const params = args as {
          query?: string;
          level?: string;
          nodeId?: string;
          workloadId?: string;
          startTime?: string;
          endTime?: string;
          limit?: number;
        };

        const logs = await client.searchLogs({
          text: params.query,
          level: params.level,
          nodeId: params.nodeId,
          workloadId: params.workloadId,
          startTime: parseTimeArg(params.startTime, Date.now() - 3600000),
          endTime: parseTimeArg(params.endTime, Date.now()),
          limit: params.limit ?? 100,
        });

        return jsonResult({
          count: logs.length,
          logs: logs.map((l) => ({
            timestamp: new Date(l.timestamp).toISOString(),
            level: l.level,
            source: l.source,
            workloadId: l.workloadId,
            nodeId: l.nodeId,
            message: l.message,
          })),
        });
      } catch (err) {
        return errorResult(`Failed to search logs: ${err}`);
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────
// alert_create
// ─────────────────────────────────────────────────────────────

const AlertCreateSchema = Type.Object({
  name: Type.String({ description: "Alert name" }),
  severity: Type.String({
    description: "Severity: info, warning, critical",
  }),
  condition: Type.String({
    description: "Alert condition (e.g., 'gpu_utilization > 90 for 5m')",
  }),
  message: Type.String({ description: "Alert message template" }),
  labels: Type.Optional(
    Type.Record(Type.String(), Type.String(), { description: "Labels for routing/grouping" })
  ),
});

export function createAlertCreateTool(): ClawbernnetesTool {
  return {
    name: "alert_create",
    label: "Clawbernetes",
    description:
      "Create an alert rule that fires when a condition is met. Alerts are sent to configured channels.",
    parameters: AlertCreateSchema,
    execute: async (_id, args, context) => {
      try {
        const client = getClient(context?.config);
        const params = args as {
          name: string;
          severity: string;
          condition: string;
          message: string;
          labels?: Record<string, string>;
        };

        const alert = await client.createAlert({
          name: params.name,
          severity: params.severity as AlertSeverity,
          condition: params.condition,
          message: params.message,
          labels: params.labels,
        });

        return jsonResult({
          success: true,
          alert: {
            id: alert.id,
            name: alert.name,
            severity: alert.severity,
            message: alert.message,
          },
          message: `Alert "${alert.name}" created (id: ${alert.id}).`,
        });
      } catch (err) {
        return errorResult(`Failed to create alert: ${err}`);
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────
// alert_list
// ─────────────────────────────────────────────────────────────

const AlertListSchema = Type.Object({
  severity: Type.Optional(Type.String({ description: "Filter by severity" })),
  active: Type.Optional(Type.Boolean({ description: "Only show active (unresolved) alerts" })),
});

export function createAlertListTool(): ClawbernnetesTool {
  return {
    name: "alert_list",
    label: "Clawbernetes",
    description: "List alerts with optional filtering by severity or active status.",
    parameters: AlertListSchema,
    execute: async (_id, args, context) => {
      try {
        const client = getClient(context?.config);
        const params = args as { severity?: string; active?: boolean };

        const alerts = await client.listAlerts({
          severity: params.severity as AlertSeverity | undefined,
          resolved: params.active === true ? false : undefined,
        });

        return jsonResult({
          count: alerts.length,
          alerts: alerts.map((a) => ({
            id: a.id,
            name: a.name,
            severity: a.severity,
            message: a.message,
            source: a.source,
            firedAt: new Date(a.firedAt).toISOString(),
            resolvedAt: a.resolvedAt ? new Date(a.resolvedAt).toISOString() : null,
            active: !a.resolvedAt,
          })),
        });
      } catch (err) {
        return errorResult(`Failed to list alerts: ${err}`);
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────
// alert_silence
// ─────────────────────────────────────────────────────────────

const AlertSilenceSchema = Type.Object({
  alertId: Type.String({ description: "Alert ID to silence" }),
  duration: Type.String({ description: "Silence duration (e.g., 1h, 24h, 7d)" }),
});

export function createAlertSilenceTool(): ClawbernnetesTool {
  return {
    name: "alert_silence",
    label: "Clawbernetes",
    description: "Silence an alert for a specified duration.",
    parameters: AlertSilenceSchema,
    execute: async (_id, args, context) => {
      try {
        const client = getClient(context?.config);
        const { alertId, duration } = args as { alertId: string; duration: string };

        const durationMs = parseStepMs(duration) * (duration.endsWith("d") ? 24 : 1);
        const result = await client.silenceAlert(alertId, durationMs / 1000);

        if (result.success) {
          return jsonResult({
            success: true,
            alertId,
            silencedFor: duration,
            message: `Alert ${alertId} silenced for ${duration}.`,
          });
        } else {
          return errorResult(`Failed to silence alert ${alertId}`);
        }
      } catch (err) {
        return errorResult(`Failed to silence alert: ${err}`);
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────
// Export all observability tools
// ─────────────────────────────────────────────────────────────

export function createObservabilityTools(): ClawbernnetesTool[] {
  return [
    createMetricsQueryTool(),
    createLogsSearchTool(),
    createAlertCreateTool(),
    createAlertListTool(),
    createAlertSilenceTool(),
  ];
}
