export interface ChatMessage {
  role: "system" | "user" | "assistant" | "tool";
  content: string;
  tool_call_id?: string;
  tool_calls?: ToolCall[];
}

export interface ToolCall {
  id: string;
  name: string;
  arguments: string; // JSON string
}

export interface ToolDefinition {
  name: string;
  description: string;
  parameters: Record<string, unknown>; // JSON Schema
}

export interface ChatResponse {
  content: string;
  tool_calls?: ToolCall[];
  usage?: { input_tokens: number; output_tokens: number };
}

export interface StreamChunk {
  type:
    | "text"
    | "tool_call_start"
    | "tool_call_delta"
    | "tool_call_end"
    | "done";
  content?: string;
  tool_call?: Partial<ToolCall>;
}

export interface LLMProvider {
  chat(
    messages: ChatMessage[],
    tools?: ToolDefinition[],
  ): Promise<ChatResponse>;
  stream(
    messages: ChatMessage[],
    tools?: ToolDefinition[],
  ): AsyncIterable<StreamChunk>;
  embed(text: string): Promise<number[]>;
  embedBatch(texts: string[]): Promise<number[][]>;
  readonly supportsEmbedding: boolean;
  readonly dimensions: number;
  readonly modelId: string;
}

export interface ProviderConfig {
  provider: "anthropic" | "openai" | "openrouter";
  model: string;
  apiKey: string;
  baseUrl?: string;
  embeddingModel?: string;
  maxTokens?: number;
}
