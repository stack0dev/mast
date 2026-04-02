import OpenAI from "openai";

export interface ChatMessage {
  role: "system" | "user" | "assistant";
  content: string;
}

export interface LLMClient {
  complete(messages: ChatMessage[]): Promise<string>;
}

export function createLLMClient(config: {
  model: string;
  apiKey: string;
  baseUrl?: string;
  maxTokens?: number;
}): LLMClient {
  const client = new OpenAI({
    apiKey: config.apiKey,
    ...(config.baseUrl ? { baseURL: config.baseUrl } : {}),
  });
  const maxTokens = config.maxTokens ?? 256;

  return {
    async complete(messages: ChatMessage[]): Promise<string> {
      const maxRetries = 3;
      for (let attempt = 0; attempt < maxRetries; attempt++) {
        try {
          const response = await client.chat.completions.create({
            model: config.model,
            messages,
            temperature: 0,
            max_tokens: maxTokens,
          });
          return response.choices[0]?.message?.content?.trim() ?? "";
        } catch (err: unknown) {
          const isRateLimit =
            err instanceof OpenAI.RateLimitError ||
            (err instanceof OpenAI.APIError && err.status === 429);
          if (isRateLimit && attempt < maxRetries - 1) {
            const delay = Math.pow(2, attempt + 1) * 1000;
            await new Promise((r) => setTimeout(r, delay));
            continue;
          }
          throw err;
        }
      }
      throw new Error("Unreachable");
    },
  };
}
