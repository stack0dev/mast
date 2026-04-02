import { query as sdkQuery } from "@anthropic-ai/claude-agent-sdk";
import type { LLMProvider, ChatMessage, ToolDefinition } from "./providers/types.js";
import type { MemoryManager, RecalledMemory } from "./memory.js";
import type { ToolRegistry } from "mast-tools";
import { assembleContext } from "./context.js";

export interface AgentConfig {
  name: string;
  systemPrompt: string;
  maxTurns?: number;
}

interface UserSession {
  messages: ChatMessage[];
}

type SdkMcpServers = Record<string, unknown>;

interface SdkAgentOpts {
  mode: "sdk";
  memory: MemoryManager;
  mcpServers: SdkMcpServers;
  config: AgentConfig;
  model: string;
  permissionMode?: "default" | "bypassPermissions" | "acceptEdits";
  effort?: "low" | "medium" | "high";
}

interface CustomAgentOpts {
  mode: "custom";
  provider: LLMProvider;
  memory: MemoryManager;
  tools: ToolRegistry;
  config: AgentConfig;
}

export type AgentOpts = SdkAgentOpts | CustomAgentOpts;

export class Agent {
  private memory: MemoryManager;
  private config: AgentConfig;
  private sessions = new Map<string, UserSession>();

  // Custom loop (openai/openrouter)
  private provider?: LLMProvider;
  private tools?: ToolRegistry;

  // SDK path (anthropic)
  private sdkOpts?: {
    mcpServers: SdkMcpServers;
    model: string;
    permissionMode: "default" | "bypassPermissions" | "acceptEdits";
    effort?: "low" | "medium" | "high";
  };
  private sdkSessions = new Map<string, string>(); // userId → SDK session ID

  constructor(opts: AgentOpts) {
    this.memory = opts.memory;
    this.config = opts.config;

    if (opts.mode === "sdk") {
      this.sdkOpts = {
        mcpServers: opts.mcpServers,
        model: opts.model,
        permissionMode: opts.permissionMode ?? "acceptEdits",
        effort: opts.effort,
      };
    } else {
      this.provider = opts.provider;
      this.tools = opts.tools;
    }
  }

  async handleMessage(
    userId: string,
    channel: string,
    message: string,
  ): Promise<string> {
    if (this.sdkOpts) {
      return this.handleSdk(userId, channel, message);
    }
    return this.handleCustomLoop(userId, channel, message);
  }

  /**
   * SDK path: Claude Agent SDK handles the agentic loop.
   * Memory tools + Stack0 exposed as in-process MCP servers.
   */
  private async handleSdk(
    userId: string,
    channel: string,
    message: string,
  ): Promise<string> {
    const collection = this.memory.getCollection(channel, userId);

    // 1. Recall relevant memories
    let memories: RecalledMemory[] = [];
    try {
      memories = await this.memory.recall(collection, message, 10);
    } catch (err) {
      console.error(`[agent:sdk] recall failed: ${err}`);
    }

    // 2. Build system prompt with recalled memories
    let systemPrompt = this.config.systemPrompt;
    if (memories.length > 0) {
      const formatted = memories
        .map(
          (m) =>
            `[${m.tier}|${m.score.toFixed(2)}] ${m.content}`,
        )
        .join("\n");
      systemPrompt +=
        "\n\n<recalled-memories>\n" + formatted + "\n</recalled-memories>";
    }

    // 3. Build SDK options
    const opts = this.sdkOpts!;
    const existingSessionId = this.sdkSessions.get(userId);

    const queryOpts: Record<string, unknown> = {
      model: opts.model,
      systemPrompt,
      mcpServers: opts.mcpServers,
      maxTurns: this.config.maxTurns ?? 10,
      permissionMode: opts.permissionMode,
      allowedTools: ["mcp__memory__*", "mcp__stack0__*"],
    };
    if (opts.effort) {
      queryOpts.effort = opts.effort;
    }
    if (opts.permissionMode === "bypassPermissions") {
      queryOpts.allowDangerouslySkipPermissions = true;
    }
    if (existingSessionId) {
      queryOpts.resume = existingSessionId;
    }

    // 4. Run the SDK query
    let resultText = "";
    let sessionId: string | undefined;

    const q = sdkQuery({
      prompt: message,
      options: queryOpts as any,
    });

    for await (const msg of q) {
      // Capture session ID from init message
      if (
        msg.type === "system" &&
        (msg as any).subtype === "init"
      ) {
        sessionId = (msg as any).session_id;
      }

      // Extract final result text
      if (
        msg.type === "result" &&
        (msg as any).subtype === "success"
      ) {
        resultText = (msg as any).result ?? "";
      }

      // Handle error results
      if (
        msg.type === "result" &&
        (msg as any).subtype !== "success"
      ) {
        const errors = (msg as any).errors as string[] | undefined;
        resultText =
          errors?.join("\n") ?? "Agent encountered an error.";
      }
    }

    // 5. Store session ID for resume on next message
    if (sessionId) {
      this.sdkSessions.set(userId, sessionId);
    }

    // 6. Auto-store exchange to memory
    if (resultText) {
      try {
        await this.memory.store(
          collection,
          `User: ${message}\nAssistant: ${resultText}`,
          { type: "conversation", channel },
          "active",
        );
      } catch (err) {
        console.error(`[agent:sdk] store failed: ${err}`);
      }
    }

    return resultText;
  }

  /**
   * Custom loop path: existing OpenAI/OpenRouter provider with tool registry.
   * Recall → assemble context → LLM → tool calls → store.
   */
  private async handleCustomLoop(
    userId: string,
    channel: string,
    message: string,
  ): Promise<string> {
    const collection = this.memory.getCollection(channel, userId);
    const session = this.getSession(userId);

    // 1. Recall relevant memories
    let memories: RecalledMemory[] = [];
    try {
      memories = await this.memory.recall(collection, message, 10);
    } catch (err) {
      console.error(`[agent] recall failed: ${err}`);
    }

    // 2. Add user message to session
    session.messages.push({ role: "user", content: message });

    // 3. Assemble context within budget
    const ctx = assembleContext({
      systemPrompt: this.config.systemPrompt,
      memories,
      recentMessages: session.messages,
    });

    // 4. Build messages for LLM
    const llmMessages: ChatMessage[] = [
      { role: "system", content: ctx.systemPrompt },
      ...ctx.messages,
    ];

    // 5. Get tool definitions for LLM
    const toolDefs = this.tools!.list().map(toLLMToolDef);

    // 6. LLM call loop (handles tool calls)
    const maxTurns = this.config.maxTurns ?? 10;
    let response = await this.provider!.chat(
      llmMessages,
      toolDefs.length > 0 ? toolDefs : undefined,
    );

    let turns = 0;
    while (response.tool_calls?.length && turns < maxTurns) {
      turns++;

      // Add assistant message with tool calls
      llmMessages.push({
        role: "assistant",
        content: response.content,
        tool_calls: response.tool_calls,
      });

      // Execute each tool call
      for (const tc of response.tool_calls) {
        let args: Record<string, unknown>;
        try {
          args = JSON.parse(tc.arguments);
        } catch {
          args = {};
        }

        const result = await this.tools!.execute(tc.name, args);
        llmMessages.push({
          role: "tool",
          content: result.content,
          tool_call_id: tc.id,
        });
      }

      // Call LLM again with tool results
      response = await this.provider!.chat(
        llmMessages,
        toolDefs.length > 0 ? toolDefs : undefined,
      );
    }

    const assistantContent = response.content;

    // 7. Add assistant response to session history
    session.messages.push({ role: "assistant", content: assistantContent });

    // 8. Auto-store: store the exchange in memory
    try {
      await this.memory.store(
        collection,
        `User: ${message}\nAssistant: ${assistantContent}`,
        { type: "conversation", channel },
        "active",
      );
    } catch (err) {
      console.error(`[agent] store failed: ${err}`);
    }

    return assistantContent;
  }

  private getSession(userId: string): UserSession {
    let session = this.sessions.get(userId);
    if (!session) {
      session = { messages: [] };
      this.sessions.set(userId, session);
    }
    return session;
  }
}

function toLLMToolDef(def: {
  name: string;
  description: string;
  parameters: Record<string, unknown>;
}): ToolDefinition {
  return {
    name: def.name,
    description: def.description,
    parameters: def.parameters,
  };
}
