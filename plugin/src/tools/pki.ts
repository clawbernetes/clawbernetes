/**
 * PKI / Certificate Management Tools
 */

import type { ClawbernnetesTool } from "../types.js";

export function createPkiTools(): ClawbernnetesTool[] {
  return [
    {
      name: "cert_issue",
      description: "Issue a new certificate",
      parameters: {
        type: "object",
        properties: {
          common_name: { type: "string", description: "Certificate common name (CN)" },
          dns_names: {
            type: "array",
            items: { type: "string" },
            description: "DNS SANs",
          },
          ip_addresses: {
            type: "array",
            items: { type: "string" },
            description: "IP SANs",
          },
          validity_days: { type: "number", description: "Validity period in days" },
          server_auth: { type: "boolean", description: "Enable server authentication (default true)" },
          client_auth: { type: "boolean", description: "Enable client authentication" },
        },
        required: ["common_name"],
      },
    },
    {
      name: "cert_get",
      description: "Get certificate details",
      parameters: {
        type: "object",
        properties: {
          cert_id: { type: "string", description: "Certificate ID (UUID)" },
        },
        required: ["cert_id"],
      },
    },
    {
      name: "cert_list",
      description: "List certificates",
      parameters: {
        type: "object",
        properties: {
          expiring_within_days: { type: "number", description: "Filter to certs expiring within N days" },
        },
      },
    },
    {
      name: "cert_revoke",
      description: "Revoke a certificate",
      parameters: {
        type: "object",
        properties: {
          cert_id: { type: "string", description: "Certificate ID to revoke" },
        },
        required: ["cert_id"],
      },
    },
    {
      name: "cert_rotate",
      description: "Rotate (renew) a certificate",
      parameters: {
        type: "object",
        properties: {
          cert_id: { type: "string", description: "Certificate ID to rotate" },
        },
        required: ["cert_id"],
      },
    },
    {
      name: "ca_status",
      description: "Get Certificate Authority status",
      parameters: { type: "object", properties: {} },
    },
  ];
}
