import { writeFileSync } from "fs";
import { generateDataset } from "./dataset.js";
import { embedAll } from "./embed.js";
import { formatBytes } from "./stats.js";
import { DATASET_SIZE, NUM_QUERIES, TOP_K, DIMENSIONS } from "./config.js";
import type { BenchmarkSystem } from "./systems/interface.js";
import { MastSystem } from "./systems/mast.js";
import { ChromaSystem } from "./systems/chroma.js";
import { LanceSystem } from "./systems/lance.js";
import { MastLocalSystem } from "./systems/mast-local.js";
import type { StoreResult } from "./benchmarks/store.js";
import type { RecallResult } from "./benchmarks/recall.js";
import type { QualityResult } from "./benchmarks/quality.js";
import { storeBenchmark } from "./benchmarks/store.js";
import { recallBenchmark } from "./benchmarks/recall.js";
import { qualityBenchmark } from "./benchmarks/quality.js";
import {
  benchmarkOpenAIEmbedder,
  benchmarkLocalEmbedder,
  printEmbedderTable,
  type EmbedderResult,
} from "./benchmarks/embedder.js";

interface SystemResult {
  system: string;
  store: StoreResult;
  recall: RecallResult;
  quality: QualityResult;
  diskSize: string;
  diskSizeBytes: number;
}

function printMarkdownTable(results: SystemResult[]): void {
  console.log("\n## Results\n");
  console.log(`Dataset: ${DATASET_SIZE} documents, ${NUM_QUERIES} queries, ${DIMENSIONS}d embeddings, top-${TOP_K}\n`);

  // Store performance
  console.log("### Store Performance\n");
  console.log("| System | Single p50 (ms) | Single p99 (ms) | Batch 1K (ms) | Throughput (ops/s) |");
  console.log("|--------|----------------|----------------|--------------|-------------------|");
  for (const r of results) {
    console.log(
      `| ${r.system} | ${r.store.singleP50Ms} | ${r.store.singleP99Ms} | ${r.store.batchTotalMs} | ${r.store.batchThroughputOps} |`
    );
  }

  // Recall performance
  console.log("\n### Recall Performance\n");
  console.log("| System | Query p50 (ms) | Query p99 (ms) | QPS |");
  console.log("|--------|---------------|---------------|-----|");
  for (const r of results) {
    console.log(
      `| ${r.system} | ${r.recall.queryP50Ms} | ${r.recall.queryP99Ms} | ${r.recall.qps} |`
    );
  }

  // Quality
  console.log("\n### Quality\n");
  console.log(`| System | Precision@${TOP_K} | Disk Size |`);
  console.log("|--------|------------|-----------|");
  for (const r of results) {
    console.log(
      `| ${r.system} | ${r.quality.precisionAtK} | ${r.diskSize} |`
    );
  }
}

async function runSystem(
  system: BenchmarkSystem,
  documents: ReturnType<typeof generateDataset>["documents"],
  queries: ReturnType<typeof generateDataset>["queries"],
  docEmbeddings: Map<string, number[]>,
  queryEmbeddings: Map<string, number[]>
): Promise<SystemResult> {
  console.log(`\n--- ${system.name} ---`);

  console.log("  Setting up...");
  await system.setup();

  console.log("  Running store benchmark...");
  const store = await storeBenchmark(system, documents, docEmbeddings);
  console.log(`    Single p50=${store.singleP50Ms}ms p99=${store.singleP99Ms}ms`);
  console.log(`    Batch: ${store.batchTotalMs}ms (${store.batchThroughputOps} ops/s)`);

  console.log("  Running recall benchmark...");
  const recall = await recallBenchmark(system, queries, queryEmbeddings);
  console.log(`    p50=${recall.queryP50Ms}ms p99=${recall.queryP99Ms}ms QPS=${recall.qps}`);

  console.log("  Running quality benchmark...");
  const quality = await qualityBenchmark(system, queries, queryEmbeddings);
  console.log(`    Precision@${TOP_K}=${quality.precisionAtK}`);

  const diskSizeBytes = await system.diskSize();
  const diskSize = diskSizeBytes > 0 ? formatBytes(diskSizeBytes) : "N/A (server)";
  console.log(`  Disk size: ${diskSize}`);

  console.log("  Tearing down...");
  await system.teardown();

  return { system: system.name, store, recall, quality, diskSize, diskSizeBytes };
}

async function main(): Promise<void> {
  console.log("MAST Benchmark Suite");
  console.log("====================\n");

  // 1. Generate dataset
  console.log("Generating dataset...");
  const { documents, queries } = generateDataset();
  console.log(`  ${documents.length} documents, ${queries.length} queries`);

  // 2. Embed all texts
  console.log("\nEmbedding documents...");
  const docTexts = documents.map((d) => d.text);
  const docVectors = await embedAll(docTexts);
  const docEmbeddings = new Map<string, number[]>();
  for (let i = 0; i < documents.length; i++) {
    docEmbeddings.set(documents[i].id, docVectors[i]);
    // Also map by text for MAST's embedder callback
    docEmbeddings.set(documents[i].text, docVectors[i]);
  }

  console.log("Embedding queries...");
  const queryTexts = queries.map((q) => q.text);
  const queryVectors = await embedAll(queryTexts);
  const queryEmbeddings = new Map<string, number[]>();
  for (let i = 0; i < queries.length; i++) {
    queryEmbeddings.set(queries[i].text, queryVectors[i]);
  }

  // 3. Build systems list — skip unavailable systems gracefully
  const systems: BenchmarkSystem[] = [new MastSystem()];

  // Check if ChromaDB is available
  try {
    const testClient = new (await import("chromadb")).ChromaClient({
      path: "http://localhost:8000",
    });
    await testClient.heartbeat();
    systems.push(new ChromaSystem());
    console.log("\nChromaDB: available");
  } catch {
    console.log("\nChromaDB: not available (skipping — start with: docker run -p 8000:8000 chromadb/chroma)");
  }

  systems.push(new LanceSystem());

  // Check if local embedder is available
  try {
    const { MastDb } = await import("mast-memory");
    const testDb = new MastDb(".bench-data/local-test.db") as any;
    if (typeof testDb.storeLocal === "function") {
      systems.push(new MastLocalSystem());
      console.log("MAST (local): available");
    } else {
      console.log("MAST (local): not available (build mast-node with local-embedder feature)");
    }
    testDb.close();
  } catch {
    console.log("MAST (local): not available");
  }

  // 4. Run benchmarks for each system
  const results: SystemResult[] = [];
  for (const system of systems) {
    try {
      const result = await runSystem(
        system,
        documents,
        queries,
        docEmbeddings,
        queryEmbeddings
      );
      results.push(result);
    } catch (err) {
      console.error(`  ERROR: ${system.name} failed:`, err);
      try {
        await system.teardown();
      } catch {
        // ignore teardown errors
      }
    }
  }

  // 5. Embedder comparison benchmark
  console.log("\n--- Embedder Comparison ---");
  const embedderResults: EmbedderResult[] = [];

  const allTexts = documents.map((d) => d.text);
  console.log("  Benchmarking OpenAI embedder...");
  const openaiResult = await benchmarkOpenAIEmbedder(allTexts);
  embedderResults.push(openaiResult);

  console.log("  Benchmarking local embedder...");
  const localResult = await benchmarkLocalEmbedder(allTexts);
  if (localResult) embedderResults.push(localResult);

  // 6. Print results
  printMarkdownTable(results);
  if (embedderResults.length > 0) printEmbedderTable(embedderResults);

  // 7. Write results.json
  writeFileSync(
    "results.json",
    JSON.stringify({ systems: results, embedders: embedderResults }, null, 2)
  );
  console.log("\nResults written to results.json");
}

main().catch((err) => {
  console.error("Fatal error:", err);
  process.exit(1);
});
