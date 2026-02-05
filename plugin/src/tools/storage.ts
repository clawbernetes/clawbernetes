/**
 * Storage Tools
 */

import type { ClawbernnetesTool } from "../types.js";

export function createStorageTools(): ClawbernnetesTool[] {
  return [
    {
      name: "storage_class_create",
      description: "Create a storage class",
      parameters: {
        type: "object",
        properties: {
          name: { type: "string", description: "Storage class name" },
          provisioner: { type: "string", description: "Provisioner name" },
          is_default: { type: "boolean", description: "Set as default storage class" },
          reclaim_policy: { type: "string", description: "Reclaim policy: retain, delete, recycle" },
          parameters: {
            type: "object",
            description: "Provisioner-specific parameters",
          },
        },
        required: ["name", "provisioner"],
      },
    },
    {
      name: "storage_class_list",
      description: "List storage classes",
      parameters: { type: "object", properties: {} },
    },
    {
      name: "volume_provision",
      description: "Provision a new volume",
      parameters: {
        type: "object",
        properties: {
          id: { type: "string", description: "Volume ID" },
          capacity_gb: { type: "number", description: "Capacity in GB" },
          storage_class: { type: "string", description: "Storage class name" },
          access_mode: {
            type: "string",
            description: "Access mode: ReadWriteOnce, ReadOnlyMany, ReadWriteMany",
          },
        },
        required: ["id", "capacity_gb"],
      },
    },
    {
      name: "volume_get",
      description: "Get volume details",
      parameters: {
        type: "object",
        properties: {
          volume_id: { type: "string", description: "Volume ID" },
        },
        required: ["volume_id"],
      },
    },
    {
      name: "volume_list",
      description: "List volumes",
      parameters: {
        type: "object",
        properties: {
          available_only: { type: "boolean", description: "Only show available volumes" },
        },
      },
    },
    {
      name: "claim_create",
      description: "Create a volume claim",
      parameters: {
        type: "object",
        properties: {
          id: { type: "string", description: "Claim ID" },
          requested_gb: { type: "number", description: "Requested capacity in GB" },
          storage_class: { type: "string", description: "Storage class name" },
          access_mode: { type: "string", description: "Access mode" },
        },
        required: ["id", "requested_gb"],
      },
    },
    {
      name: "claim_list",
      description: "List volume claims",
      parameters: {
        type: "object",
        properties: {
          pending_only: { type: "boolean", description: "Only show pending claims" },
        },
      },
    },
    {
      name: "claim_bind",
      description: "Bind a volume to a claim",
      parameters: {
        type: "object",
        properties: {
          volume_id: { type: "string", description: "Volume ID" },
          claim_id: { type: "string", description: "Claim ID" },
        },
        required: ["volume_id", "claim_id"],
      },
    },
    {
      name: "reconcile_claims",
      description: "Reconcile pending claims with available volumes",
      parameters: { type: "object", properties: {} },
    },
  ];
}
