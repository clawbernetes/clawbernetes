/**
 * Operations Tools (Autoscaling, Preemption, Rollback)
 */

import type { ClawbernnetesTool } from "../types.js";

export function createOperationsTools(): ClawbernnetesTool[] {
  return [
    // Autoscaling
    {
      name: "autoscale_pool_create",
      description: "Create an autoscaling pool",
      parameters: {
        type: "object",
        properties: {
          id: { type: "string", description: "Pool identifier" },
          name: { type: "string", description: "Pool name" },
          min_nodes: { type: "number", description: "Minimum node count" },
          max_nodes: { type: "number", description: "Maximum node count" },
          target_utilization: { type: "number", description: "Target GPU utilization % (default 70)" },
        },
        required: ["id", "name", "min_nodes", "max_nodes"],
      },
    },
    {
      name: "autoscale_pool_list",
      description: "List autoscaling pools",
      parameters: { type: "object", properties: {} },
    },
    {
      name: "autoscale_evaluate",
      description: "Evaluate autoscaling and get scaling recommendations",
      parameters: { type: "object", properties: {} },
    },
    {
      name: "autoscale_status",
      description: "Get autoscaler status",
      parameters: { type: "object", properties: {} },
    },

    // Preemption
    {
      name: "preemption_register",
      description: "Register a workload for preemption tracking",
      parameters: {
        type: "object",
        properties: {
          workload_id: { type: "string", description: "Workload ID" },
          priority_class: {
            type: "string",
            description: "Priority class (system-critical, high-priority, default, spot, preemptible)",
          },
          gpus: { type: "number", description: "GPU count" },
          memory_gb: { type: "number", description: "Memory in GB" },
        },
        required: ["workload_id"],
      },
    },
    {
      name: "preemption_request",
      description: "Request preemption to free resources",
      parameters: {
        type: "object",
        properties: {
          gpus_needed: { type: "number", description: "GPUs needed" },
          memory_gb_needed: { type: "number", description: "Memory needed in GB" },
          requester_priority: {
            type: "string",
            description: "Priority of the requester",
          },
        },
        required: ["gpus_needed"],
      },
    },
    {
      name: "preemption_list",
      description: "List preemptible workloads",
      parameters: { type: "object", properties: {} },
    },

    // Rollback
    {
      name: "rollback_record",
      description: "Record a deployment snapshot for rollback",
      parameters: {
        type: "object",
        properties: {
          deployment_id: { type: "string", description: "Deployment ID" },
          name: { type: "string", description: "Deployment name" },
          image: { type: "string", description: "Container image" },
        },
        required: ["deployment_id", "name", "image"],
      },
    },
    {
      name: "rollback_plan",
      description: "Plan a rollback",
      parameters: {
        type: "object",
        properties: {
          deployment_id: { type: "string", description: "Deployment to rollback" },
          target_version: { type: "string", description: "Target version (optional)" },
        },
        required: ["deployment_id"],
      },
    },
    {
      name: "rollback_history",
      description: "Get rollback history",
      parameters: {
        type: "object",
        properties: {
          limit: { type: "number", description: "Max results (default 20)" },
        },
      },
    },
    {
      name: "rollback_trigger_check",
      description: "Check if rollback should be triggered based on metrics",
      parameters: {
        type: "object",
        properties: {
          error_rate: { type: "number", description: "Current error rate %" },
          p99_latency_ms: { type: "number", description: "P99 latency in ms" },
        },
      },
    },
  ];
}
