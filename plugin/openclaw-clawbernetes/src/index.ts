/**
 * Clawbernetes OpenClaw Plugin — Fleet-level GPU cluster management.
 *
 * Provides 5 fleet-level tools that aggregate across all connected clawnodes,
 * a background health monitor, and gateway RPC methods for the Control UI.
 *
 * Tools:
 *   claw_fleet_status  — aggregate cluster state in one call
 *   claw_gpu_inventory — all GPUs across all nodes
 *   claw_deploy        — auto-place workload on best node
 *   claw_workloads     — cross-node workload list
 *   claw_multi_invoke  — fan-out any command to multiple nodes
 */

import { registerFleetStatus } from "./tools/fleet-status.js";
import { registerGpuInventory } from "./tools/gpu-inventory.js";
import { registerDeploy } from "./tools/deploy.js";
import { registerWorkloads } from "./tools/workloads.js";
import { registerMultiInvoke } from "./tools/multi-invoke.js";
import { registerHealthMonitor, getCachedFleetStatus } from "./services/health-monitor.js";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type PluginApi = any;

export default {
  id: "clawbernetes",
  name: "Clawbernetes GPU Cluster",
  description: "Fleet-level GPU cluster management — aggregate tools, monitoring, and 20 bundled skills",

  register(api: PluginApi) {
    const pluginConfig = api.pluginConfig ?? {};
    const invokeTimeoutMs: number = typeof pluginConfig.invokeTimeoutMs === "number" ? pluginConfig.invokeTimeoutMs : 30000;
    const healthIntervalMs: number = typeof pluginConfig.healthIntervalMs === "number" ? pluginConfig.healthIntervalMs : 60000;

    // Fleet-level agent tools
    registerFleetStatus(api, invokeTimeoutMs);
    registerGpuInventory(api, invokeTimeoutMs);
    registerDeploy(api, invokeTimeoutMs);
    registerWorkloads(api, invokeTimeoutMs);
    registerMultiInvoke(api, invokeTimeoutMs);

    // Background fleet health monitor
    registerHealthMonitor(api, healthIntervalMs, invokeTimeoutMs);

    // Gateway RPC for Control UI dashboard
    if (typeof api.registerGatewayMethod === "function") {
      api.registerGatewayMethod("clawbernetes.fleet-status", async ({ respond }: { respond: (ok: boolean, data: unknown) => void }) => {
        const status = getCachedFleetStatus();
        respond(true, status);
      });
    }

    api.logger.info("[clawbernetes] Plugin loaded — 5 fleet tools, health monitor");
  },
};
