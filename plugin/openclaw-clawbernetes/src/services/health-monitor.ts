/**
 * Fleet health monitor — background service that periodically checks
 * node health and caches fleet status for the dashboard.
 *
 * Tracks state transitions (healthy → unhealthy, connected/disconnected)
 * and logs warnings when problems are detected.
 */

import { invokeAll, listNodes, type InvokeOptions, type InvokeResult } from "../invoke.js";

interface NodeHealthState {
  nodeId: string;
  healthy: boolean;
  lastSeen: number;
  consecutiveFailures: number;
}

interface CachedFleetStatus {
  timestamp: number;
  nodes: number;
  healthy: number;
  unhealthy: number;
  disconnected: string[];
  recentTransitions: Array<{
    nodeId: string;
    from: string;
    to: string;
    at: number;
  }>;
}

// Module-level cache so the gateway method handler can read it
let cachedStatus: CachedFleetStatus = {
  timestamp: 0,
  nodes: 0,
  healthy: 0,
  unhealthy: 0,
  disconnected: [],
  recentTransitions: [],
};

export function getCachedFleetStatus(): CachedFleetStatus {
  return cachedStatus;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function registerHealthMonitor(api: any, healthIntervalMs: number, invokeTimeoutMs: number): void {
  const opts: InvokeOptions = {
    gatewayUrl: api.pluginConfig?.gatewayUrl ?? "http://127.0.0.1:18789",
    timeoutMs: invokeTimeoutMs,
    logger: api.logger,
  };

  const knownNodes = new Map<string, NodeHealthState>();
  let intervalHandle: ReturnType<typeof setInterval> | null = null;

  async function checkFleetHealth(): Promise<void> {
    try {
      const nodes = await listNodes(opts);
      const now = Date.now();
      const currentNodeIds = new Set(nodes.map((n) => n.nodeId));

      // Detect disconnected nodes
      const disconnected: string[] = [];
      for (const [nodeId, state] of knownNodes) {
        if (!currentNodeIds.has(nodeId)) {
          disconnected.push(nodeId);
          if (state.healthy) {
            api.logger.warn(`[clawbernetes] Node disconnected: ${nodeId}`);
            addTransition(nodeId, "healthy", "disconnected", now);
            state.healthy = false;
          }
        }
      }

      if (nodes.length === 0) {
        cachedStatus = {
          timestamp: now,
          nodes: 0,
          healthy: 0,
          unhealthy: 0,
          disconnected,
          recentTransitions: cachedStatus.recentTransitions.slice(-20),
        };
        return;
      }

      // Health check all connected nodes
      const nodeIds = nodes.map((n) => n.nodeId);
      const healthResults = await invokeAll(opts, nodeIds, "node.health");

      let healthyCount = 0;
      let unhealthyCount = 0;

      for (const nodeId of nodeIds) {
        const result = healthResults.get(nodeId);
        const isHealthy = isHealthyResult(result);

        const prev = knownNodes.get(nodeId);
        const wasHealthy = prev?.healthy ?? true;

        if (isHealthy) {
          healthyCount++;
        } else {
          unhealthyCount++;
        }

        // Track state transitions
        if (prev && wasHealthy && !isHealthy) {
          api.logger.warn(`[clawbernetes] Node unhealthy: ${nodeId}`);
          addTransition(nodeId, "healthy", "unhealthy", now);
        } else if (prev && !wasHealthy && isHealthy) {
          api.logger.info(`[clawbernetes] Node recovered: ${nodeId}`);
          addTransition(nodeId, "unhealthy", "healthy", now);
        } else if (!prev) {
          api.logger.info(`[clawbernetes] Node discovered: ${nodeId}`);
          addTransition(nodeId, "unknown", isHealthy ? "healthy" : "unhealthy", now);
        }

        knownNodes.set(nodeId, {
          nodeId,
          healthy: isHealthy,
          lastSeen: now,
          consecutiveFailures: isHealthy ? 0 : (prev?.consecutiveFailures ?? 0) + 1,
        });
      }

      cachedStatus = {
        timestamp: now,
        nodes: nodeIds.length,
        healthy: healthyCount,
        unhealthy: unhealthyCount,
        disconnected,
        recentTransitions: cachedStatus.recentTransitions.slice(-20),
      };
    } catch (err) {
      api.logger.error(`[clawbernetes] Health check failed: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  function addTransition(nodeId: string, from: string, to: string, at: number): void {
    cachedStatus.recentTransitions.push({ nodeId, from, to, at });
    // Keep only the last 50 transitions
    if (cachedStatus.recentTransitions.length > 50) {
      cachedStatus.recentTransitions = cachedStatus.recentTransitions.slice(-50);
    }
  }

  // Register as a plugin service with start/stop lifecycle
  if (typeof api.registerService === "function") {
    api.registerService({
      id: "clawbernetes-health-monitor",
      name: "Clawbernetes Health Monitor",
      start() {
        api.logger.info(`[clawbernetes] Health monitor starting (interval: ${healthIntervalMs}ms)`);
        // Run immediately, then on interval
        void checkFleetHealth();
        intervalHandle = setInterval(() => void checkFleetHealth(), healthIntervalMs);
      },
      stop() {
        api.logger.info("[clawbernetes] Health monitor stopping");
        if (intervalHandle) {
          clearInterval(intervalHandle);
          intervalHandle = null;
        }
      },
    });
  } else {
    // Fallback: start immediately if no service registry
    api.logger.info(`[clawbernetes] Health monitor starting (interval: ${healthIntervalMs}ms)`);
    void checkFleetHealth();
    intervalHandle = setInterval(() => void checkFleetHealth(), healthIntervalMs);
  }
}

function isHealthyResult(result: InvokeResult | undefined): boolean {
  if (!result?.ok) return false;
  const payload = result.payload;
  if (typeof payload === "object" && payload !== null) {
    const p = payload as Record<string, unknown>;
    return p.healthy === true || p.status === "healthy" || p.status === "ok";
  }
  return false;
}
