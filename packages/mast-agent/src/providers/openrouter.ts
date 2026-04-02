import OpenAI from "openai";
import type {
  ChatMessage,
  ChatResponse,
  LLMProvider,
  ProviderConfig,
  StreamChunk,
  ToolDefinition,
} from "./types.js";
import { OpenAIProvider } from "./openai.js";

const OPENROUTER_BASE_URL = "https://openrouter.ai/api/v1";
const DEFAULT_MODEL = "anthropic/claude-sonnet-4-5-20250929";

/**
 * OpenRouter is OpenAI-compatible. We reuse OpenAIProvider by constructing
 * it with the OpenRouter base URL and default headers.
 *
 * Embedding is not supported through OpenRouter — delegates to a separate
 * OpenAI embedding client if an API key is available, otherwise throws.
 */
export class OpenRouterProvider implements LLMProvider {
  private inner: OpenAIProvider;

  readonly supportsEmbedding: boolean;
  readonly dimensions: number;
  readonly modelId: string;

  constructor(config: ProviderConfig) {
    // OpenRouter uses the OpenAI-compatible API
    const orConfig: ProviderConfig = {
      ...config,
      model: config.model || DEFAULT_MODEL,
      baseUrl: config.baseUrl || OPENROUTER_BASE_URL,
    };

    this.inner = new OpenAIProvider(orConfig);
    this.modelId = orConfig.model;

    // OpenRouter doesn't serve embeddings
    this.supportsEmbedding = false;
    this.dimensions = 0;
  }

  chat(
    messages: ChatMessage[],
    tools?: ToolDefinition[],
  ): Promise<ChatResponse> {
    return this.inner.chat(messages, tools);
  }

  stream(
    messages: ChatMessage[],
    tools?: ToolDefinition[],
  ): AsyncIterable<StreamChunk> {
    return this.inner.stream(messages, tools);
  }

  async embed(_text: string): Promise<number[]> {
    throw new Error(
      "OpenRouter does not support embeddings. Use a local embedder or configure an OpenAI embedding model.",
    );
  }

  async embedBatch(_texts: string[]): Promise<number[][]> {
    throw new Error(
      "OpenRouter does not support embeddings. Use a local embedder or configure an OpenAI embedding model.",
    );
  }
}
