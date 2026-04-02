export interface PluginConfig {
  dbPath: string;
  collection: string;
  embeddingProvider: "voyage" | "openai";
  apiKey: string;
  embeddingModel: string;
  dimensions: number;
  autoRecall: boolean;
  autoCapture: boolean;
  maxResults: number;
  searchMode: "vector" | "fulltext" | "hybrid";
}

export interface StoreParams {
  content: string;
  metadata?: Record<string, string>;
  tier?: "core" | "active" | "background" | "archive";
  ttl?: number;
}

export interface SearchParams {
  query: string;
  limit?: number;
  mode?: "vector" | "fulltext" | "hybrid";
}

export interface ForgetParams {
  id: string;
}

export interface RelateParams {
  source: string;
  target: string;
  relation: string;
  weight?: number;
}

export interface TraverseParams {
  start: string;
  maxDepth?: number;
  relation?: string;
  minWeight?: number;
}

export type EmbedFn = (text: string) => Promise<number[]>;

export interface ToolResult {
  content: string;
  isError?: boolean;
}

export interface HookContext {
  messages: Array<{ role: string; content: string }>;
}

export interface HookResult {
  prependContext?: string;
}
