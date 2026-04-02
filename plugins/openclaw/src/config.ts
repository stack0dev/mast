import * as os from "os";
import * as path from "path";
import * as fs from "fs";
import type { PluginConfig } from "./types";

const PROVIDER_DEFAULTS: Record<string, { model: string; dimensions: number }> =
  {
    voyage: { model: "voyage-3.5", dimensions: 1024 },
    openai: { model: "text-embedding-3-small", dimensions: 1536 },
  };

export function parseConfig(raw: Partial<PluginConfig> = {}): PluginConfig {
  const provider = raw.embeddingProvider ?? "voyage";
  if (provider !== "voyage" && provider !== "openai") {
    throw new Error(
      `Invalid embeddingProvider "${provider}". Must be "voyage" or "openai".`
    );
  }

  const defaults = PROVIDER_DEFAULTS[provider];
  const apiKey =
    raw.apiKey || process.env.MAST_EMBEDDING_API_KEY || "";
  if (!apiKey) {
    throw new Error(
      "Missing embedding API key. Set apiKey in plugin config or MAST_EMBEDDING_API_KEY env var."
    );
  }

  const dbPath = resolveDbPath(raw.dbPath ?? "~/.mast/memory.db");
  const maxResults = raw.maxResults ?? 10;
  if (maxResults < 1 || maxResults > 50) {
    throw new Error("maxResults must be between 1 and 50.");
  }

  const searchMode = raw.searchMode ?? "hybrid";
  if (!["vector", "fulltext", "hybrid"].includes(searchMode)) {
    throw new Error(
      `Invalid searchMode "${searchMode}". Must be "vector", "fulltext", or "hybrid".`
    );
  }

  return {
    dbPath,
    collection: raw.collection ?? "openclaw",
    embeddingProvider: provider,
    apiKey,
    embeddingModel: raw.embeddingModel ?? defaults.model,
    dimensions: raw.dimensions ?? defaults.dimensions,
    autoRecall: raw.autoRecall ?? true,
    autoCapture: raw.autoCapture ?? true,
    maxResults,
    searchMode,
  };
}

function resolveDbPath(input: string): string {
  const expanded = input.startsWith("~")
    ? path.join(os.homedir(), input.slice(1))
    : input;
  const resolved = path.resolve(expanded);
  fs.mkdirSync(path.dirname(resolved), { recursive: true });
  return resolved;
}
