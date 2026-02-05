/**
 * Service Discovery Tools
 */

import type { ClawbernnetesTool } from "../types.js";

export function createServiceTools(): ClawbernnetesTool[] {
  return [
    {
      name: "service_register",
      description: "Register a service for discovery",
      parameters: {
        type: "object",
        properties: {
          name: { type: "string", description: "Service name" },
          namespace: { type: "string", description: "Namespace (default: default)" },
          ports: {
            type: "array",
            items: {
              type: "object",
              properties: {
                port: { type: "number" },
                name: { type: "string" },
                target_port: { type: "number" },
                protocol: { type: "string", description: "http, tcp, grpc, https" },
              },
              required: ["port"],
            },
            description: "Service ports",
          },
          labels: {
            type: "object",
            description: "Service labels for selection",
          },
        },
        required: ["name", "ports"],
      },
    },
    {
      name: "service_get",
      description: "Get service details",
      parameters: {
        type: "object",
        properties: {
          namespace: { type: "string", description: "Namespace" },
          name: { type: "string", description: "Service name" },
        },
        required: ["namespace", "name"],
      },
    },
    {
      name: "service_list",
      description: "List services",
      parameters: {
        type: "object",
        properties: {
          namespace: { type: "string", description: "Filter by namespace" },
          labels: {
            type: "object",
            description: "Filter by labels",
          },
        },
      },
    },
    {
      name: "service_deregister",
      description: "Deregister a service",
      parameters: {
        type: "object",
        properties: {
          namespace: { type: "string", description: "Namespace" },
          name: { type: "string", description: "Service name" },
        },
        required: ["namespace", "name"],
      },
    },
    {
      name: "endpoint_add",
      description: "Add an endpoint to a service",
      parameters: {
        type: "object",
        properties: {
          namespace: { type: "string", description: "Namespace" },
          service_name: { type: "string", description: "Service name" },
          address: { type: "string", description: "IP address" },
          port: { type: "number", description: "Port number" },
          weight: { type: "number", description: "Load balancing weight (default 100)" },
        },
        required: ["namespace", "service_name", "address", "port"],
      },
    },
    {
      name: "endpoint_list",
      description: "List endpoints for a service",
      parameters: {
        type: "object",
        properties: {
          namespace: { type: "string", description: "Namespace" },
          service_name: { type: "string", description: "Service name" },
          healthy_only: { type: "boolean", description: "Only return healthy endpoints" },
        },
        required: ["namespace", "service_name"],
      },
    },
    {
      name: "endpoint_select",
      description: "Select an endpoint using load balancing",
      parameters: {
        type: "object",
        properties: {
          namespace: { type: "string", description: "Namespace" },
          service_name: { type: "string", description: "Service name" },
        },
        required: ["namespace", "service_name"],
      },
    },
  ];
}
