import type { PluginConfig, EmbedFn } from "./types";

interface EmbeddingResponse {
  data: Array<{ embedding: number[] }>;
}

const ENDPOINTS: Record<string, string> = {
  voyage: "https://api.voyageai.com/v1/embeddings",
  openai: "https://api.openai.com/v1/embeddings",
};

export function createEmbedFn(config: PluginConfig): EmbedFn {
  const endpoint = ENDPOINTS[config.embeddingProvider];

  return async (text: string): Promise<number[]> => {
    const body: Record<string, unknown> = {
      model: config.embeddingModel,
      input: [text],
    };

    if (config.embeddingProvider === "voyage") {
      body.output_dimension = config.dimensions;
    } else {
      body.dimensions = config.dimensions;
    }

    const resp = await fetch(endpoint, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${config.apiKey}`,
      },
      body: JSON.stringify(body),
    });

    if (!resp.ok) {
      const text = await resp.text().catch(() => "unknown");
      throw new Error(
        `Embedding API error ${resp.status}: ${text}`
      );
    }

    const json = (await resp.json()) as EmbeddingResponse;
    const first = json.data?.[0];
    if (!first) {
      throw new Error("Embedding API returned empty data array");
    }
    return first.embedding;
  };
}
