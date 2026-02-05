/**
 * Multi-Tenancy Tools
 */

import type { ClawbernnetesTool } from "../types.js";

export function createTenancyTools(): ClawbernnetesTool[] {
  return [
    {
      name: "tenant_create",
      description: "Create a new tenant",
      parameters: {
        type: "object",
        properties: {
          name: { type: "string", description: "Tenant name" },
          default_quota: {
            type: "object",
            description: "Default quota for namespaces",
            properties: {
              max_gpus: { type: "number" },
              gpu_hours: { type: "number" },
              memory_mib: { type: "number" },
            },
          },
        },
        required: ["name"],
      },
    },
    {
      name: "tenant_get",
      description: "Get tenant details",
      parameters: {
        type: "object",
        properties: {
          tenant_id: { type: "string", description: "Tenant ID" },
          name: { type: "string", description: "Tenant name (alternative)" },
        },
      },
    },
    {
      name: "tenant_list",
      description: "List all tenants",
      parameters: { type: "object", properties: {} },
    },
    {
      name: "tenant_delete",
      description: "Delete a tenant",
      parameters: {
        type: "object",
        properties: {
          tenant_id: { type: "string", description: "Tenant ID to delete" },
        },
        required: ["tenant_id"],
      },
    },
    {
      name: "namespace_create",
      description: "Create a namespace within a tenant",
      parameters: {
        type: "object",
        properties: {
          tenant_id: { type: "string", description: "Parent tenant ID" },
          name: { type: "string", description: "Namespace name" },
          quota: {
            type: "object",
            description: "Resource quota",
            properties: {
              max_gpus: { type: "number" },
              gpu_hours: { type: "number" },
              memory_mib: { type: "number" },
            },
          },
        },
        required: ["tenant_id", "name"],
      },
    },
    {
      name: "namespace_get",
      description: "Get namespace details",
      parameters: {
        type: "object",
        properties: {
          namespace_id: { type: "string", description: "Namespace ID" },
          tenant_id: { type: "string", description: "Tenant ID (with name)" },
          name: { type: "string", description: "Namespace name (with tenant_id)" },
        },
      },
    },
    {
      name: "namespace_list",
      description: "List namespaces",
      parameters: {
        type: "object",
        properties: {
          tenant_id: { type: "string", description: "Filter by tenant" },
        },
      },
    },
    {
      name: "namespace_delete",
      description: "Delete a namespace",
      parameters: {
        type: "object",
        properties: {
          namespace_id: { type: "string", description: "Namespace ID to delete" },
        },
        required: ["namespace_id"],
      },
    },
    {
      name: "quota_set",
      description: "Set quota for a namespace",
      parameters: {
        type: "object",
        properties: {
          namespace_id: { type: "string", description: "Namespace ID" },
          quota: {
            type: "object",
            description: "New quota values",
            properties: {
              max_gpus: { type: "number" },
              gpu_hours: { type: "number" },
              memory_mib: { type: "number" },
              max_workloads: { type: "number" },
            },
          },
        },
        required: ["namespace_id", "quota"],
      },
    },
    {
      name: "usage_report",
      description: "Get resource usage report",
      parameters: {
        type: "object",
        properties: {
          tenant_id: { type: "string", description: "Filter by tenant" },
          namespace_id: { type: "string", description: "Filter by namespace" },
          threshold_percent: { type: "number", description: "Alert threshold (default 80%)" },
        },
      },
    },
  ];
}
