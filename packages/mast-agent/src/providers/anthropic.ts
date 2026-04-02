import Anthropic from "@anthropic-ai/sdk";
import type {
  ChatMessage,
  ChatResponse,
  LLMProvider,
  ProviderConfig,
  StreamChunk,
  ToolCall,
  ToolDefinition,
} from "./types.js";

const DEFAULT_MODEL = "claude-sonnet-4-5-20250929";
const DEFAULT_MAX_TOKENS = 8192;

export class AnthropicProvider implements LLMProvider {
  private client: Anthropic;
  private model: string;
  private maxTokens: number;

  readonly supportsEmbedding = false;
  readonly dimensions = 0;
  readonly modelId: string;

  constructor(config: ProviderConfig) {
    this.client = new Anthropic({ apiKey: config.apiKey });
    this.model = config.model || DEFAULT_MODEL;
    this.maxTokens = config.maxTokens ?? DEFAULT_MAX_TOKENS;
    this.modelId = this.model;
  }

  async chat(
    messages: ChatMessage[],
    tools?: ToolDefinition[],
  ): Promise<ChatResponse> {
    const { system, mapped } = mapMessages(messages);

    const params: Anthropic.MessageCreateParams = {
      model: this.model,
      max_tokens: this.maxTokens,
      messages: mapped,
    };
    if (system) params.system = system;
    if (tools?.length) params.tools = mapTools(tools);

    const response = await this.client.messages.create(params);
    return mapResponse(response);
  }

  async *stream(
    messages: ChatMessage[],
    tools?: ToolDefinition[],
  ): AsyncIterable<StreamChunk> {
    const { system, mapped } = mapMessages(messages);

    const params: Anthropic.MessageCreateParams = {
      model: this.model,
      max_tokens: this.maxTokens,
      messages: mapped,
    };
    if (system) params.system = system;
    if (tools?.length) params.tools = mapTools(tools);

    const stream = this.client.messages.stream(params);

    let currentToolCall: Partial<ToolCall> | null = null;

    for await (const event of stream) {
      if (event.type === "content_block_start") {
        const block = event.content_block;
        if (block.type === "text") {
          // text block starting — content comes in deltas
        } else if (block.type === "tool_use") {
          currentToolCall = {
            id: block.id,
            name: block.name,
            arguments: "",
          };
          yield { type: "tool_call_start", tool_call: currentToolCall };
        }
      } else if (event.type === "content_block_delta") {
        const delta = event.delta;
        if (delta.type === "text_delta") {
          yield { type: "text", content: delta.text };
        } else if (
          delta.type === "input_json_delta" &&
          currentToolCall
        ) {
          currentToolCall.arguments =
            (currentToolCall.arguments ?? "") + delta.partial_json;
          yield {
            type: "tool_call_delta",
            tool_call: currentToolCall,
            content: delta.partial_json,
          };
        }
      } else if (event.type === "content_block_stop") {
        if (currentToolCall) {
          yield { type: "tool_call_end", tool_call: currentToolCall };
          currentToolCall = null;
        }
      } else if (event.type === "message_stop") {
        yield { type: "done" };
      }
    }
  }

  async embed(_text: string): Promise<number[]> {
    throw new Error(
      "Anthropic does not support embeddings. Use a local embedder or configure an OpenAI embedding model.",
    );
  }

  async embedBatch(_texts: string[]): Promise<number[][]> {
    throw new Error(
      "Anthropic does not support embeddings. Use a local embedder or configure an OpenAI embedding model.",
    );
  }
}

// --- helpers ---

function mapMessages(messages: ChatMessage[]): {
  system: string | undefined;
  mapped: Anthropic.MessageParam[];
} {
  let system: string | undefined;
  const mapped: Anthropic.MessageParam[] = [];

  for (const msg of messages) {
    if (msg.role === "system") {
      system = msg.content;
      continue;
    }

    if (msg.role === "tool") {
      // Tool result — map to Anthropic's tool_result content block
      mapped.push({
        role: "user",
        content: [
          {
            type: "tool_result",
            tool_use_id: msg.tool_call_id!,
            content: msg.content,
          },
        ],
      });
      continue;
    }

    if (msg.role === "assistant" && msg.tool_calls?.length) {
      // Assistant message with tool calls — map to content blocks
      const content: Anthropic.ContentBlockParam[] = [];
      if (msg.content) {
        content.push({ type: "text", text: msg.content });
      }
      for (const tc of msg.tool_calls) {
        content.push({
          type: "tool_use",
          id: tc.id,
          name: tc.name,
          input: JSON.parse(tc.arguments),
        });
      }
      mapped.push({ role: "assistant", content });
      continue;
    }

    mapped.push({
      role: msg.role as "user" | "assistant",
      content: msg.content,
    });
  }

  return { system, mapped };
}

function mapTools(
  tools: ToolDefinition[],
): Anthropic.Messages.Tool[] {
  return tools.map((t) => ({
    name: t.name,
    description: t.description,
    input_schema: t.parameters as Anthropic.Messages.Tool.InputSchema,
  }));
}

function mapResponse(
  response: Anthropic.Message,
): ChatResponse {
  let content = "";
  const toolCalls: ToolCall[] = [];

  for (const block of response.content) {
    if (block.type === "text") {
      content += block.text;
    } else if (block.type === "tool_use") {
      toolCalls.push({
        id: block.id,
        name: block.name,
        arguments: JSON.stringify(block.input),
      });
    }
  }

  return {
    content,
    tool_calls: toolCalls.length > 0 ? toolCalls : undefined,
    usage: {
      input_tokens: response.usage.input_tokens,
      output_tokens: response.usage.output_tokens,
    },
  };
}
