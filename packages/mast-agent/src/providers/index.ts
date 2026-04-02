export type {
  ChatMessage,
  ChatResponse,
  LLMProvider,
  ProviderConfig,
  StreamChunk,
  ToolCall,
  ToolDefinition,
} from "./types.js";

import type { LLMProvider, ProviderConfig } from "./types.js";
import { AnthropicProvider } from "./anthropic.js";
import { OpenAIProvider } from "./openai.js";
import { OpenRouterProvider } from "./openrouter.js";

export { AnthropicProvider } from "./anthropic.js";
export { OpenAIProvider } from "./openai.js";
export { OpenRouterProvider } from "./openrouter.js";

export function createProvider(config: ProviderConfig): LLMProvider {
  switch (config.provider) {
    case "anthropic":
      return new AnthropicProvider(config);
    case "openai":
      return new OpenAIProvider(config);
    case "openrouter":
      return new OpenRouterProvider(config);
    default: {
      const exhaustive: never = config.provider;
      throw new Error(`Unknown provider: ${exhaustive}`);
    }
  }
}
