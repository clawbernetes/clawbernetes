/**
 * Node invoke helper — wraps gateway HTTP API for node.invoke calls.
 *
 * All fleet-level tools use this to fan out commands to clawnodes.
 * Works against both claw-gateway-server and OpenClaw gateway.
 */

// Minimal logger interface matching OpenClaw plugin API
interface Logger {
  info(msg: string): void;
  warn(msg: string): void;
  error(msg: string): void;
}

export interface InvokeOptions {
  gatewayUrl: string;
  timeoutMs: number;
  logger: Logger;
}

export interface InvokeResult {
  ok: boolean;
  payload?: unknown;
  error?: string;
}

export interface NodeInfo {
  nodeId: string;
  name?: string;
  status?: string;
  connectedAt?: number;
}

/**
 * Invoke a command on a specific clawnode via the gateway.
 */
export async function invokeNode(
  opts: InvokeOptions,
  nodeId: string,
  command: string,
  params?: Record<string, unknown>,
): Promise<InvokeResult> {
  const token = process.env.OPENCLAW_GATEWAY_TOKEN ?? process.env.CLAW_GATEWAY_TOKEN;
  const url = `${opts.gatewayUrl}/tools/invoke`;

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), opts.timeoutMs);

  try {
    const res = await fetch(url, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
      },
      body: JSON.stringify({
        tool: "nodes",
        action: "invoke",
        args: {
          node: nodeId,
          command,
          params: JSON.stringify(params ?? {}),
        },
      }),
      signal: controller.signal,
    });

    if (!res.ok) {
      const text = await res.text();
      return { ok: false, error: `HTTP ${res.status}: ${text}` };
    }

    const data = await res.json();
    return { ok: true, payload: data };
  } catch (err) {
    if (err instanceof Error && err.name === "AbortError") {
      return { ok: false, error: `Timeout after ${opts.timeoutMs}ms` };
    }
    return { ok: false, error: err instanceof Error ? err.message : String(err) };
  } finally {
    clearTimeout(timer);
  }
}

/**
 * List all connected clawnodes from the gateway presence API.
 */
export async function listNodes(opts: InvokeOptions): Promise<NodeInfo[]> {
  const token = process.env.OPENCLAW_GATEWAY_TOKEN ?? process.env.CLAW_GATEWAY_TOKEN;
  const url = `${opts.gatewayUrl}/nodes`;

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), opts.timeoutMs);

  try {
    const res = await fetch(url, {
      method: "GET",
      headers: {
        "Content-Type": "application/json",
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
      },
      signal: controller.signal,
    });

    if (!res.ok) {
      opts.logger.warn(`Failed to list nodes: HTTP ${res.status}`);
      return [];
    }

    const data = (await res.json()) as { nodes?: NodeInfo[] };
    return data.nodes ?? [];
  } catch (err) {
    opts.logger.warn(`Failed to list nodes: ${err instanceof Error ? err.message : String(err)}`);
    return [];
  } finally {
    clearTimeout(timer);
  }
}

/**
 * Fan out a command to multiple nodes in parallel.
 * Returns results keyed by nodeId.
 */
export async function invokeAll(
  opts: InvokeOptions,
  nodeIds: string[],
  command: string,
  params?: Record<string, unknown>,
): Promise<Map<string, InvokeResult>> {
  const results = new Map<string, InvokeResult>();
  const promises = nodeIds.map(async (nodeId) => {
    const result = await invokeNode(opts, nodeId, command, params);
    results.set(nodeId, result);
  });
  await Promise.all(promises);
  return results;
}
