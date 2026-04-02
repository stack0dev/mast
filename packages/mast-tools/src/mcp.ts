import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import type { Tool, ToolRegistry, ToolResult } from "./types.js";

export interface McpServerConfig {
  name: string;
  command: string;
  args?: string[];
  env?: Record<string, string>;
}

export async function connectMcpServer(
  config: McpServerConfig,
): Promise<{ tools: Tool[]; close: () => Promise<void> }> {
  const transport = new StdioClientTransport({
    command: config.command,
    args: config.args,
    env: config.env,
  });

  const client = new Client(
    { name: "mast-agent", version: "0.1.0" },
    { capabilities: {} },
  );

  await client.connect(transport);

  const { tools: serverTools } = await client.listTools();

  const tools: Tool[] = serverTools.map((serverTool) => {
    const prefixedName = `${config.name}_${serverTool.name}`;
    return {
      definition: {
        name: prefixedName,
        description: serverTool.description ?? "",
        parameters: {
          type: "object" as const,
          properties: (serverTool.inputSchema.properties ?? {}) as Record<
            string,
            unknown
          >,
          required: serverTool.inputSchema.required,
        },
      },
      async execute(args: Record<string, unknown>): Promise<ToolResult> {
        try {
          const result = await client.callTool({
            name: serverTool.name,
            arguments: args,
          });
          if ("toolResult" in result) {
            return { content: String(result.toolResult) };
          }
          const text = result.content
            .map((c) => ("text" in c ? c.text : JSON.stringify(c)))
            .join("\n");
          return { content: text, isError: result.isError ?? false };
        } catch (err) {
          const message = err instanceof Error ? err.message : String(err);
          return { content: `MCP call failed: ${message}`, isError: true };
        }
      },
    };
  });

  return {
    tools,
    close: () => client.close(),
  };
}

export async function loadMcpServers(
  configs: McpServerConfig[],
  registry: ToolRegistry,
): Promise<Array<() => Promise<void>>> {
  const closers: Array<() => Promise<void>> = [];

  for (const config of configs) {
    try {
      const { tools, close } = await connectMcpServer(config);
      closers.push(close);
      for (const tool of tools) {
        registry.register(tool);
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      console.error(`Failed to connect MCP server "${config.name}": ${message}`);
    }
  }

  return closers;
}
