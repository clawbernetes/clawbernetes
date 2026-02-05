/**
 * Deployment Tools
 */

import type { ClawbernnetesTool } from "../types.js";

export function createDeployTools(): ClawbernnetesTool[] {
  return [
    {
      name: "deploy_intent",
      description: "Deploy using natural language intent (AI-native deployment)",
      parameters: {
        type: "object",
        properties: {
          intent: {
            type: "string",
            description: "Natural language deployment intent (e.g., 'deploy pytorch:2.0 with 4 GPUs using canary')",
          },
          namespace: { type: "string", description: "Target namespace (optional)" },
          dry_run: { type: "boolean", description: "Preview without executing" },
        },
        required: ["intent"],
      },
    },
    {
      name: "deploy_status",
      description: "Get deployment status",
      parameters: {
        type: "object",
        properties: {
          deployment_id: { type: "string", description: "Deployment ID" },
        },
        required: ["deployment_id"],
      },
    },
    {
      name: "deploy_list",
      description: "List deployments",
      parameters: {
        type: "object",
        properties: {
          namespace: { type: "string", description: "Filter by namespace" },
          state: { type: "string", description: "Filter by state (pending, running, complete, failed)" },
          limit: { type: "number", description: "Max results" },
        },
      },
    },
    {
      name: "deploy_promote",
      description: "Promote a canary deployment to full rollout",
      parameters: {
        type: "object",
        properties: {
          deployment_id: { type: "string", description: "Deployment ID to promote" },
        },
        required: ["deployment_id"],
      },
    },
    {
      name: "deploy_rollback",
      description: "Rollback a deployment to previous version",
      parameters: {
        type: "object",
        properties: {
          deployment_id: { type: "string", description: "Deployment ID to rollback" },
          target_version: { type: "string", description: "Specific version to rollback to (optional)" },
        },
        required: ["deployment_id"],
      },
    },
    {
      name: "deploy_abort",
      description: "Abort an in-progress deployment",
      parameters: {
        type: "object",
        properties: {
          deployment_id: { type: "string", description: "Deployment ID to abort" },
        },
        required: ["deployment_id"],
      },
    },
  ];
}
