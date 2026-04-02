import type { BenchmarkSystem } from "../systems/interface.js";
import type { Query } from "../dataset.js";
import { TOP_K } from "../config.js";
import { p50, p99, formatMs } from "../stats.js";

export interface RecallResult {
  queryP50Ms: string;
  queryP99Ms: string;
  qps: string;
}

export async function recallBenchmark(
  system: BenchmarkSystem,
  queries: Query[],
  queryEmbeddings: Map<string, number[]>
): Promise<RecallResult> {
  const latencies: number[] = [];

  const totalStart = process.hrtime.bigint();

  for (const query of queries) {
    const emb = queryEmbeddings.get(query.text)!;
    const start = process.hrtime.bigint();
    await system.query(emb, TOP_K, query.text);
    const elapsed = Number(process.hrtime.bigint() - start);
    latencies.push(elapsed);
  }

  const totalElapsed = Number(process.hrtime.bigint() - totalStart);
  const qps = (queries.length / totalElapsed) * 1_000_000_000;

  return {
    queryP50Ms: formatMs(p50(latencies)),
    queryP99Ms: formatMs(p99(latencies)),
    qps: qps.toFixed(1),
  };
}
