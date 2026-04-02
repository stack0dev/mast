import type { Tool, ToolResult } from "./types.js";

export interface BuiltinToolsConfig {
  store: (
    collection: string,
    content: string,
    metadata?: Record<string, string>,
  ) => Promise<{ id: string }>;
  recall: (
    collection: string,
    query: string,
    limit?: number,
  ) => Promise<Array<{ content: string; score: number }>>;
  forget: (collection: string, id: string) => Promise<void>;
  getCollection: () => string;
}

export function createBuiltinTools(config: BuiltinToolsConfig): Tool[] {
  const remember: Tool = {
    definition: {
      name: "remember",
      description:
        "Store something in long-term memory. Use this to save important facts, preferences, or context for later retrieval.",
      parameters: {
        type: "object",
        properties: {
          content: {
            type: "string",
            description: "The content to remember",
          },
          metadata: {
            type: "object",
            description: "Optional key-value metadata to attach",
            additionalProperties: { type: "string" },
          },
        },
        required: ["content"],
      },
    },
    async execute(args: Record<string, unknown>): Promise<ToolResult> {
      try {
        const content = args.content as string;
        const metadata = args.metadata as Record<string, string> | undefined;
        const collection = config.getCollection();
        const { id } = await config.store(collection, content, metadata);
        return { content: `Stored memory ${id}` };
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        return { content: `Failed to store memory: ${message}`, isError: true };
      }
    },
  };

  const searchMemory: Tool = {
    definition: {
      name: "search_memory",
      description:
        "Search long-term memory for relevant information. Returns memories ranked by relevance.",
      parameters: {
        type: "object",
        properties: {
          query: {
            type: "string",
            description: "The search query",
          },
          limit: {
            type: "number",
            description: "Maximum number of results (default: 5)",
          },
        },
        required: ["query"],
      },
    },
    async execute(args: Record<string, unknown>): Promise<ToolResult> {
      try {
        const query = args.query as string;
        const limit = (args.limit as number) ?? 5;
        const collection = config.getCollection();
        const results = await config.recall(collection, query, limit);
        if (results.length === 0) {
          return { content: "No matching memories found." };
        }
        const formatted = results
          .map(
            (r, i) =>
              `[${i + 1}] (score: ${r.score.toFixed(3)}) ${r.content}`,
          )
          .join("\n\n");
        return { content: formatted };
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        return {
          content: `Failed to search memory: ${message}`,
          isError: true,
        };
      }
    },
  };

  const forget: Tool = {
    definition: {
      name: "forget",
      description: "Remove a specific memory by its ID.",
      parameters: {
        type: "object",
        properties: {
          memory_id: {
            type: "string",
            description: "The ID of the memory to remove",
          },
        },
        required: ["memory_id"],
      },
    },
    async execute(args: Record<string, unknown>): Promise<ToolResult> {
      try {
        const memoryId = args.memory_id as string;
        const collection = config.getCollection();
        await config.forget(collection, memoryId);
        return { content: `Removed memory ${memoryId}` };
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        return {
          content: `Failed to remove memory: ${message}`,
          isError: true,
        };
      }
    },
  };

  return [remember, searchMemory, forget];
}
