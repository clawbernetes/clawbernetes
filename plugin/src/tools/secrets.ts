/**
 * Secrets Management Tools
 */

import type { ClawbernnetesTool } from "../types.js";

export function createSecretsTools(): ClawbernnetesTool[] {
  return [
    {
      name: "secret_put",
      description: "Store a secret",
      parameters: {
        type: "object",
        properties: {
          id: { type: "string", description: "Secret identifier" },
          value: { type: "string", description: "Secret value" },
          allowed_workloads: {
            type: "array",
            items: { type: "string" },
            description: "Workload IDs allowed to access this secret",
          },
        },
        required: ["id", "value"],
      },
    },
    {
      name: "secret_get",
      description: "Retrieve a secret value",
      parameters: {
        type: "object",
        properties: {
          id: { type: "string", description: "Secret identifier" },
          workload_id: { type: "string", description: "Workload requesting access (for access control)" },
          reason: { type: "string", description: "Reason for access (audit)" },
        },
        required: ["id"],
      },
    },
    {
      name: "secret_delete",
      description: "Delete a secret",
      parameters: {
        type: "object",
        properties: {
          id: { type: "string", description: "Secret identifier to delete" },
        },
        required: ["id"],
      },
    },
    {
      name: "secret_list",
      description: "List all secret IDs (values not included)",
      parameters: { type: "object", properties: {} },
    },
    {
      name: "secret_rotate",
      description: "Rotate a secret to a new value",
      parameters: {
        type: "object",
        properties: {
          id: { type: "string", description: "Secret identifier" },
          new_value: { type: "string", description: "New secret value" },
        },
        required: ["id", "new_value"],
      },
    },
    {
      name: "secret_metadata",
      description: "Get secret metadata (without the value)",
      parameters: {
        type: "object",
        properties: {
          id: { type: "string", description: "Secret identifier" },
        },
        required: ["id"],
      },
    },
  ];
}
