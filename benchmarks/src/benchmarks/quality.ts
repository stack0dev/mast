import type { BenchmarkSystem } from "../systems/interface.js";
import type { Query } from "../dataset.js";
import { TOP_K } from "../config.js";

export interface QualityResult {
  precisionAtK: string;
}

export async function qualityBenchmark(
  system: BenchmarkSystem,
  queries: Query[],
  queryEmbeddings: Map<string, number[]>,
  topK: number = TOP_K
): Promise<QualityResult> {
  let totalPrecision = 0;

  for (const query of queries) {
    const emb = queryEmbeddings.get(query.text)!;
    const results = await system.query(emb, topK, query.text);
    const resultIds = new Set(results.map((r) => r.id));
    const relevantSet = new Set(query.relevantIds);

    let hits = 0;
    for (const id of resultIds) {
      if (relevantSet.has(id)) hits++;
    }

    totalPrecision += hits / topK;
  }

  return {
    precisionAtK: (totalPrecision / queries.length).toFixed(4),
  };
}
