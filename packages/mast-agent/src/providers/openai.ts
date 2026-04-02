import OpenAI from "openai";
import type {
  ChatMessage,
  ChatResponse,
  LLMProvider,
  ProviderConfig,
  StreamChunk,
  ToolCall,
  ToolDefinition,
} from "./types.js";

const DEFAULT_MODEL = "gpt-4o";
const DEFAULT_EMBEDDING_MODEL = "text-embedding-3-small";
const DEFAULT_MAX_TOKENS = 8192;

const EMBEDDING_DIMENSIONS: Record<string, number> = {
  "text-embedding-3-small": 1536,
  "text-embedding-3-large": 3072,
  "text-embedding-ada-002": 1536,
};

export class OpenAIProvider implements LLMProvider {
  private client: OpenAI;
  private model: string;
  private embeddingModel: string;
  private maxTokens: number;

  readonly supportsEmbedding: boolean;
  readonly dimensions: number;
  readonly modelId: string;

  constructor(config: ProviderConfig) {
    this.client = new OpenAI({
      apiKey: config.apiKey,
      ...(config.baseUrl && { baseURL: config.baseUrl }),
    });
    this.model = config.model || DEFAULT_MODEL;
    this.embeddingModel = config.embeddingModel || DEFAULT_EMBEDDING_MODEL;
    this.maxTokens = config.maxTokens ?? DEFAULT_MAX_TOKENS;
    this.modelId = this.model;
    this.supportsEmbedding = true;
    this.dimensions =
      EMBEDDING_DIMENSIONS[this.embeddingModel] ?? 1536;
  }

  async chat(
    messages: ChatMessage[],
    tools?: ToolDefinition[],
  ): Promise<ChatResponse> {
    const params: OpenAI.ChatCompletionCreateParams = {
      model: this.model,
      max_tokens: this.maxTokens,
      messages: mapMessages(messages),
    };
    if (tools?.length) {
      params.tools = mapTools(tools);
    }

    const response = await this.client.chat.completions.create(params);
    const choice = response.choices[0];
    if (!choice) throw new Error("OpenAI returned no choices");

    return mapChoice(choice, response.usage);
  }

  async *stream(
    messages: ChatMessage[],
    tools?: ToolDefinition[],
  ): AsyncIterable<StreamChunk> {
    const params: OpenAI.ChatCompletionCreateParams = {
      model: this.model,
      max_tokens: this.maxTokens,
      messages: mapMessages(messages),
      stream: true,
    };
    if (tools?.length) {
      params.tools = mapTools(tools);
    }

    const stream = await this.client.chat.completions.create(params);

    // Track in-progress tool calls by index
    const activeCalls = new Map<number, Partial<ToolCall>>();

    for await (const chunk of stream as AsyncIterable<OpenAI.ChatCompletionChunk>) {
      const delta = chunk.choices[0]?.delta;
      if (!delta) continue;

      if (delta.content) {
        yield { type: "text", content: delta.content };
      }

      if (delta.tool_calls) {
        for (const tc of delta.tool_calls) {
          if (tc.function?.name) {
            // New tool call starting
            const call: Partial<ToolCall> = {
              id: tc.id,
              name: tc.function.name,
              arguments: tc.function.arguments ?? "",
            };
            activeCalls.set(tc.index, call);
            yield { type: "tool_call_start", tool_call: call };
          } else if (tc.function?.arguments) {
            // Delta for existing tool call
            const existing = activeCalls.get(tc.index);
            if (existing) {
              existing.arguments =
                (existing.arguments ?? "") + tc.function.arguments;
              yield {
                type: "tool_call_delta",
                tool_call: existing,
                content: tc.function.arguments,
              };
            }
          }
        }
      }

      if (chunk.choices[0]?.finish_reason) {
        // Emit tool_call_end for all active calls
        for (const [, call] of activeCalls) {
          yield { type: "tool_call_end", tool_call: call };
        }
        activeCalls.clear();
        yield { type: "done" };
      }
    }
  }

  async embed(text: string): Promise<number[]> {
    const response = await this.client.embeddings.create({
      model: this.embeddingModel,
      input: text,
    });
    return response.data[0].embedding;
  }

  async embedBatch(texts: string[]): Promise<number[][]> {
    if (texts.length === 0) return [];
    const response = await this.client.embeddings.create({
      model: this.embeddingModel,
      input: texts,
    });
    // Ensure results are in the same order as input
    return response.data
      .sort((a, b) => a.index - b.index)
      .map((d) => d.embedding);
  }
}

// --- helpers ---

function mapMessages(
  messages: ChatMessage[],
): OpenAI.ChatCompletionMessageParam[] {
  return messages.map((msg) => {
    if (msg.role === "tool") {
      return {
        role: "tool" as const,
        content: msg.content,
        tool_call_id: msg.tool_call_id!,
      };
    }

    if (msg.role === "assistant" && msg.tool_calls?.length) {
      return {
        role: "assistant" as const,
        content: msg.content || null,
        tool_calls: msg.tool_calls.map((tc) => ({
          id: tc.id,
          type: "function" as const,
          function: {
            name: tc.name,
            arguments: tc.arguments,
          },
        })),
      };
    }

    return {
      role: msg.role as "system" | "user" | "assistant",
      content: msg.content,
    };
  });
}

function mapTools(
  tools: ToolDefinition[],
): OpenAI.ChatCompletionTool[] {
  return tools.map((t) => ({
    type: "function" as const,
    function: {
      name: t.name,
      description: t.description,
      parameters: t.parameters,
    },
  }));
}

function mapChoice(
  choice: OpenAI.ChatCompletion.Choice,
  usage?: OpenAI.CompletionUsage | null,
): ChatResponse {
  const toolCalls: ToolCall[] | undefined =
    choice.message.tool_calls?.map((tc) => ({
      id: tc.id,
      name: tc.function.name,
      arguments: tc.function.arguments,
    }));

  return {
    content: choice.message.content ?? "",
    tool_calls: toolCalls?.length ? toolCalls : undefined,
    usage: usage
      ? {
          input_tokens: usage.prompt_tokens,
          output_tokens: usage.completion_tokens,
        }
      : undefined,
  };
}
