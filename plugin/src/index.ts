/**
 * Clawbernetes OpenClaw Plugin
 *
 * AI-native GPU orchestration tools for OpenClaw.
 *
 * @example
 * ```typescript
 * // Install as OpenClaw plugin
 * // In openclaw.json:
 * {
 *   "plugins": {
 *     "clawbernetes": {
 *       "enabled": true
 *     }
 *   }
 * }
 * ```
 */

import type { OpenClawPlugin, ToolContext, ClawbernnetesTool } from "./types.js";
import { createAllTools, TOOL_CATEGORIES } from "./tools/index.js";

// Re-export types
export * from "./types.js";
export * from "./client.js";
export * from "./tools/index.js";

/**
 * Plugin version
 */
export const VERSION = "0.1.0";

/**
 * Plugin ID
 */
export const PLUGIN_ID = "clawbernetes";

/**
 * Create the Clawbernetes plugin for OpenClaw.
 */
export function createPlugin(): OpenClawPlugin {
  return {
    id: PLUGIN_ID,
    name: "Clawbernetes",
    version: VERSION,
    description: "AI-native GPU orchestration tools for cluster management, workloads, and the MOLT marketplace",
    tools: (_context: ToolContext): ClawbernnetesTool[] => {
      return createAllTools();
    },
  };
}

/**
 * Default export for OpenClaw plugin auto-discovery.
 */
export default createPlugin();

/**
 * List all available tools with their descriptions.
 */
export function listTools(): Array<{ name: string; description: string; category: string }> {
  const tools = createAllTools();
  const result: Array<{ name: string; description: string; category: string }> = [];

  for (const [categoryKey, category] of Object.entries(TOOL_CATEGORIES)) {
    for (const toolName of category.tools) {
      const tool = tools.find((t) => t.name === toolName);
      if (tool) {
        result.push({
          name: tool.name,
          description: tool.description,
          category: categoryKey,
        });
      }
    }
  }

  return result;
}

/**
 * Print a summary of the plugin for documentation.
 */
export function printPluginSummary(): string {
  const lines: string[] = [
    `# Clawbernetes OpenClaw Plugin v${VERSION}`,
    "",
    "AI-native GPU orchestration tools.",
    "",
  ];

  for (const [_key, category] of Object.entries(TOOL_CATEGORIES)) {
    lines.push(`## ${category.name}`);
    lines.push(category.description);
    lines.push("");
    for (const toolName of category.tools) {
      lines.push(`- \`${toolName}\``);
    }
    lines.push("");
  }

  return lines.join("\n");
}
