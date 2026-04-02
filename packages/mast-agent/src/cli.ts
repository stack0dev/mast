#!/usr/bin/env node

import { resolve } from "path";
import { mkdirSync } from "fs";
import { loadConfig } from "./config.js";
import { createProvider } from "./providers/index.js";
import { createMemoryManager } from "./memory.js";
import { Agent } from "./agent.js";
import { createChannels } from "mast-channels";
import { DefaultToolRegistry, createBuiltinTools, loadMcpServers, createStack0Tools } from "mast-tools";
import { createMemoryMcpServer, createStack0McpServer } from "./mcp-server.js";

async function main() {
  const configPath = process.argv[2] ?? "mast-agent.toml";
  const config = loadConfig(configPath);

  console.log(`Starting agent "${config.agent.name}"...`);

  // Ensure data directory exists
  const dbDir = resolve(config.memory.dbPath, "..");
  mkdirSync(dbDir, { recursive: true });

  const isAnthropicSdk = config.agent.provider === "anthropic";

  // Create memory manager (shared by both paths)
  // For SDK path: always use local embedder (SDK doesn't expose embedding)
  let memory;
  if (isAnthropicSdk) {
    memory = createMemoryManager({
      dbPath: config.memory.dbPath,
      durability: config.memory.durability,
      useLocalEmbedder: true,
    });
  } else {
    const provider = createProvider({
      provider: config.agent.provider,
      model: config.agent.model,
      apiKey: config.agent.apiKey ?? "",
      embeddingModel: config.agent.embeddingModel,
    });
    const useLocal = !provider.supportsEmbedding;
    memory = createMemoryManager({
      dbPath: config.memory.dbPath,
      durability: config.memory.durability,
      embedFn: provider.supportsEmbedding ? (text) => provider.embed(text) : undefined,
      dimensions: provider.supportsEmbedding ? provider.dimensions : undefined,
      useLocalEmbedder: useLocal,
    });
  }

  let agent: Agent;
  const mcpClosers: Array<() => Promise<void>> = [];

  const systemPrompt = config.agent.systemPrompt ?? `You are ${config.agent.name}, a helpful AI assistant.`;

  if (isAnthropicSdk) {
    // --- SDK path (anthropic provider) ---
    let currentCollection = "default";

    // Build in-process MCP servers
    const mcpServers: Record<string, unknown> = {};

    // Memory MCP server
    const memoryMcp = createMemoryMcpServer(
      memory,
      () => currentCollection,
    );
    mcpServers.memory = memoryMcp;

    // Stack0 MCP server
    if (config.tools?.stack0?.apiKey) {
      const stack0Mcp = createStack0McpServer(config.tools.stack0);
      mcpServers.stack0 = stack0Mcp;
      console.log("[stack0] MCP server registered for SDK");
    }

    // Add external MCP servers from config (stdio — SDK handles them natively)
    if (config.tools?.mcpServers?.length) {
      for (const srv of config.tools.mcpServers) {
        mcpServers[srv.name] = {
          command: srv.command,
          args: srv.args,
        };
      }
    }

    agent = new Agent({
      mode: "sdk",
      memory,
      mcpServers,
      config: {
        name: config.agent.name,
        systemPrompt,
        maxTurns: config.agent.maxTurns,
      },
      model: config.agent.model,
      permissionMode: config.agent.permissionMode,
      effort: config.agent.effort,
    });

    // Wire collection tracking into handler
    const channels = createChannels(
      buildChannelConfig(config),
      config.agent.name,
    );

    const handler = {
      async handleMessage(userId: string, channel: string, message: string): Promise<string> {
        currentCollection = memory.getCollection(channel, userId);
        return agent.handleMessage(userId, channel, message);
      },
    };

    for (const ch of channels) {
      await ch.start(handler);
      console.log(`[${ch.name}] channel started`);
    }

    setupShutdown(channels, mcpClosers, memory);
  } else {
    // --- Custom loop path (openai/openrouter) ---
    const provider = createProvider({
      provider: config.agent.provider,
      model: config.agent.model,
      apiKey: config.agent.apiKey ?? "",
      embeddingModel: config.agent.embeddingModel,
    });

    // Create tool registry with builtins
    const tools = new DefaultToolRegistry();
    let currentCollection = "default";

    const builtins = createBuiltinTools({
      store: (col, content, meta) => memory.store(col, content, meta),
      recall: (col, query, limit) => memory.recall(col, query, limit),
      forget: (col, id) => memory.forget(col, id),
      getCollection: () => currentCollection,
    });
    for (const tool of builtins) {
      tools.register(tool);
    }

    // Register Stack0 tools
    if (config.tools?.stack0?.apiKey) {
      const stack0Tools = createStack0Tools(config.tools.stack0);
      for (const tool of stack0Tools) {
        tools.register(tool);
      }
      console.log(`[stack0] ${stack0Tools.length} tools registered`);
    }

    // Connect MCP servers
    if (config.tools?.mcpServers?.length) {
      const closers = await loadMcpServers(config.tools.mcpServers, tools);
      mcpClosers.push(...closers);
    }

    agent = new Agent({
      mode: "custom",
      provider,
      memory,
      tools,
      config: {
        name: config.agent.name,
        systemPrompt,
        maxTurns: config.agent.maxTurns,
      },
    });

    const channels = createChannels(
      buildChannelConfig(config),
      config.agent.name,
    );

    const handler = {
      async handleMessage(userId: string, channel: string, message: string): Promise<string> {
        currentCollection = memory.getCollection(channel, userId);
        return agent.handleMessage(userId, channel, message);
      },
    };

    for (const ch of channels) {
      await ch.start(handler);
      console.log(`[${ch.name}] channel started`);
    }

    setupShutdown(channels, mcpClosers, memory);
  }
}

function buildChannelConfig(config: ReturnType<typeof loadConfig>) {
  const channelConfig = config.channels ?? { cli: true };
  if (!channelConfig.cli && !channelConfig.http) {
    channelConfig.cli = true;
  }
  return channelConfig;
}

function setupShutdown(
  channels: Array<{ stop: () => Promise<void> }>,
  mcpClosers: Array<() => Promise<void>>,
  memory: { close: () => void },
) {
  const shutdown = async () => {
    console.log("\nShutting down...");
    for (const ch of channels) {
      await ch.stop();
    }
    for (const close of mcpClosers) {
      await close();
    }
    memory.close();
    process.exit(0);
  };

  process.on("SIGINT", shutdown);
  process.on("SIGTERM", shutdown);
}

main().catch((err) => {
  console.error("Fatal:", err);
  process.exit(1);
});
