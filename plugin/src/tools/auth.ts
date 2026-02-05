/**
 * Auth & RBAC Tools
 */

import type { ClawbernnetesTool } from "../types.js";

export function createAuthTools(): ClawbernnetesTool[] {
  return [
    {
      name: "user_create",
      description: "Create a new user with optional roles",
      parameters: {
        type: "object",
        properties: {
          name: { type: "string", description: "Username" },
          email: { type: "string", description: "User email (optional)" },
          roles: { type: "array", items: { type: "string" }, description: "Initial roles to assign" },
        },
        required: ["name"],
      },
    },
    {
      name: "user_get",
      description: "Get user details by ID or name",
      parameters: {
        type: "object",
        properties: {
          user_id: { type: "string", description: "User ID" },
          name: { type: "string", description: "Username (alternative to user_id)" },
        },
      },
    },
    {
      name: "user_list",
      description: "List all users",
      parameters: { type: "object", properties: {} },
    },
    {
      name: "user_delete",
      description: "Delete a user",
      parameters: {
        type: "object",
        properties: {
          user_id: { type: "string", description: "User ID to delete" },
        },
        required: ["user_id"],
      },
    },
    {
      name: "role_assign",
      description: "Assign a role to a user",
      parameters: {
        type: "object",
        properties: {
          user_id: { type: "string", description: "User ID" },
          role_name: { type: "string", description: "Role name to assign" },
        },
        required: ["user_id", "role_name"],
      },
    },
    {
      name: "role_revoke",
      description: "Revoke a role from a user",
      parameters: {
        type: "object",
        properties: {
          user_id: { type: "string", description: "User ID" },
          role_name: { type: "string", description: "Role name to revoke" },
        },
        required: ["user_id", "role_name"],
      },
    },
    {
      name: "role_list",
      description: "List all available roles",
      parameters: { type: "object", properties: {} },
    },
    {
      name: "permission_check",
      description: "Check if a user has permission to perform an action",
      parameters: {
        type: "object",
        properties: {
          user_id: { type: "string", description: "User ID" },
          action: { type: "string", description: "Action to check (read, write, admin, etc.)" },
          resource: { type: "string", description: "Resource type (workloads, nodes, etc.)" },
        },
        required: ["user_id", "action", "resource"],
      },
    },
    {
      name: "api_key_generate",
      description: "Generate a new API key for a user",
      parameters: {
        type: "object",
        properties: {
          user_id: { type: "string", description: "User ID" },
          name: { type: "string", description: "Key name/description" },
          expires_in_days: { type: "number", description: "Expiration in days (optional)" },
        },
        required: ["user_id", "name"],
      },
    },
    {
      name: "api_key_list",
      description: "List API keys for a user",
      parameters: {
        type: "object",
        properties: {
          user_id: { type: "string", description: "User ID" },
        },
        required: ["user_id"],
      },
    },
    {
      name: "api_key_revoke",
      description: "Revoke an API key",
      parameters: {
        type: "object",
        properties: {
          key_id: { type: "string", description: "API key ID to revoke" },
        },
        required: ["key_id"],
      },
    },
  ];
}
