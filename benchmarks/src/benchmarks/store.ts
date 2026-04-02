import type { BenchmarkSystem } from "../systems/interface.js";
import type { Document } from "../dataset.js";
import { p50, p99, formatMs } from "../stats.js";

export interface StoreResult {
  singleP50Ms: string;
  singleP99Ms: string;
  batchTotalMs: string;
  batchThroughputOps: string;
}

export async function storeBenchmark(
  system: BenchmarkSystem,
  documents: Document[],
  embeddings: Map<string, number[]>
): Promise<StoreResult> {
  // Single-doc store latency: measure first 100 docs individually
  const singleCount = Math.min(100, documents.length);
  const singleLatencies: number[] = [];

  for (let i = 0; i < singleCount; i++) {
    const doc = documents[i];
    const emb = embeddings.get(doc.id)!;
    const start = process.hrtime.bigint();
    await system.store(doc.id, doc.text, emb, { topic: String(doc.topic) });
    const elapsed = Number(process.hrtime.bigint() - start);
    singleLatencies.push(elapsed);
  }

  // Teardown and re-setup for batch test
  await system.teardown();
  await system.setup();

  // Batch store: all 1000 docs at once
  const batchItems = documents.map((doc) => ({
    id: doc.id,
    text: doc.text,
    embedding: embeddings.get(doc.id)!,
    metadata: { topic: String(doc.topic) },
  }));

  const batchStart = process.hrtime.bigint();
  await system.storeBatch(batchItems);
  const batchElapsed = Number(process.hrtime.bigint() - batchStart);

  const count = await system.count();
  if (count !== documents.length) {
    console.warn(
      `  WARNING: expected ${documents.length} docs, got ${count}`
    );
  }

  return {
    singleP50Ms: formatMs(p50(singleLatencies)),
    singleP99Ms: formatMs(p99(singleLatencies)),
    batchTotalMs: formatMs(batchElapsed),
    batchThroughputOps: (
      (documents.length / Number(batchElapsed)) *
      1_000_000_000
    ).toFixed(0),
  };
}
