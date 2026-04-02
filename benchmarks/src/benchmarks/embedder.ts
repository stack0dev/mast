import { p50, p99, formatMs } from "../stats.js";
import { embedAll } from "../embed.js";

export interface EmbedderResult {
  name: string;
  singleP50Ms: string;
  singleP99Ms: string;
  batchTotalMs: string;
  batchThroughputTextsPerSec: string;
}

/**
 * Benchmark OpenAI API embedding performance.
 * Measures single-text latency and batch throughput.
 */
export async function benchmarkOpenAIEmbedder(
  texts: string[]
): Promise<EmbedderResult> {
  const singleCount = Math.min(100, texts.length);
  const singleLatencies: number[] = [];

  for (let i = 0; i < singleCount; i++) {
    const start = process.hrtime.bigint();
    await embedAll([texts[i]]);
    const elapsed = Number(process.hrtime.bigint() - start);
    singleLatencies.push(elapsed);
  }

  const batchSize = Math.min(1000, texts.length);
  const batchTexts = texts.slice(0, batchSize);
  const batchStart = process.hrtime.bigint();
  await embedAll(batchTexts);
  const batchElapsed = Number(process.hrtime.bigint() - batchStart);

  return {
    name: "OpenAI API",
    singleP50Ms: formatMs(p50(singleLatencies)),
    singleP99Ms: formatMs(p99(singleLatencies)),
    batchTotalMs: formatMs(batchElapsed),
    batchThroughputTextsPerSec: (
      (batchSize / Number(batchElapsed)) *
      1_000_000_000
    ).toFixed(0),
  };
}

/**
 * Benchmark local embedder (fastembed) via MAST's store_local.
 * Measures end-to-end embedding + store latency.
 */
export async function benchmarkLocalEmbedder(
  texts: string[]
): Promise<EmbedderResult | null> {
  try {
    const { MastDb } = await import("mast-memory");
    const { mkdirSync, rmSync, existsSync } = await import("fs");
    const { join } = await import("path");

    const dbPath = ".bench-data/embedder-test";
    if (existsSync(dbPath)) rmSync(dbPath, { recursive: true });
    mkdirSync(dbPath, { recursive: true });
    const db = new MastDb(join(dbPath, "test.db")) as any;

    if (typeof db.storeLocal !== "function") {
      console.log("  Local embedder not available (build with local-embedder feature)");
      db.close();
      return null;
    }

    const singleCount = Math.min(100, texts.length);
    const singleLatencies: number[] = [];

    for (let i = 0; i < singleCount; i++) {
      const start = process.hrtime.bigint();
      db.storeLocal("bench", texts[i], null, null, null);
      const elapsed = Number(process.hrtime.bigint() - start);
      singleLatencies.push(elapsed);
    }

    // Recreate DB for batch test
    db.close();
    if (existsSync(dbPath)) rmSync(dbPath, { recursive: true });
    mkdirSync(dbPath, { recursive: true });
    const db2 = new MastDb(join(dbPath, "test.db")) as any;

    const batchSize = Math.min(1000, texts.length);
    const batchStart = process.hrtime.bigint();
    for (let i = 0; i < batchSize; i++) {
      db2.storeLocal("bench", texts[i], null, null, null);
    }
    const batchElapsed = Number(process.hrtime.bigint() - batchStart);

    db2.close();

    return {
      name: "Local (fastembed)",
      singleP50Ms: formatMs(p50(singleLatencies)),
      singleP99Ms: formatMs(p99(singleLatencies)),
      batchTotalMs: formatMs(batchElapsed),
      batchThroughputTextsPerSec: (
        (batchSize / Number(batchElapsed)) *
        1_000_000_000
      ).toFixed(0),
    };
  } catch (err) {
    console.log(`  Local embedder benchmark skipped: ${err}`);
    return null;
  }
}

export function printEmbedderTable(results: EmbedderResult[]): void {
  console.log("\n### Embedder Performance\n");
  console.log(
    "| Embedder | Single p50 (ms) | Single p99 (ms) | Batch 1K (ms) | Throughput (texts/s) |"
  );
  console.log(
    "|----------|----------------|----------------|--------------|---------------------|"
  );
  for (const r of results) {
    console.log(
      `| ${r.name} | ${r.singleP50Ms} | ${r.singleP99Ms} | ${r.batchTotalMs} | ${r.batchThroughputTextsPerSec} |`
    );
  }
}
