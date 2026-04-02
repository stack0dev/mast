import fs from "node:fs";
import path from "node:path";
import { MastDb } from "mast-memory";
import type {
  RunConfig,
  SearchModeConfig,
  ExperimentConfig,
  QAResult,
  ExperimentResult,
  BenchmarkReport,
  Checkpoint,
} from "./types.js";
import { loadData } from "./data.js";
import { ingestAll } from "./ingest.js";
import { extractAll } from "./extract.js";
import { evaluateSample } from "./evaluate.js";
import { createLLMClient } from "./llm.js";
import { buildExperimentResult, writeResults } from "./report.js";

// ── CLI parsing ──

function parseSearchModes(csv: string): SearchModeConfig[] {
  return csv.split(",").map((s) => {
    const mode = s.trim().toLowerCase();
    if (mode === "vector" || mode === "fulltext" || mode === "hybrid") {
      return { mode } as SearchModeConfig;
    }
    throw new Error(`Unknown search mode: ${s}`);
  });
}

function parseArgs(argv: string[]): RunConfig {
  const args = argv.slice(2);
  const get = (flag: string): string | undefined => {
    const idx = args.indexOf(flag);
    return idx >= 0 && idx + 1 < args.length ? args[idx + 1] : undefined;
  };
  const has = (flag: string) => args.includes(flag);

  const outputDir = get("--output") ?? "./bench-results";

  const apiKey = get("--api-key") ?? process.env.OPENAI_API_KEY ?? "";

  return {
    dataPath: get("--data") ?? path.join(outputDir, "locomo10.json"),
    outputDir,
    model: get("--model") ?? "gpt-4o-mini",
    baseUrl: get("--base-url"),
    apiKey,
    recallOnly: has("--recall-only") || !apiKey,
    judge: has("--judge"),
    judgeModel: get("--judge-model") ?? get("--model") ?? "gpt-4o-mini",
    concurrency: parseInt(get("--concurrency") ?? "5", 10),
    searchModes: parseSearchModes(get("--search-modes") ?? "vector,fulltext,hybrid"),
    topKs: (get("--top-k") ?? "5,10,20").split(",").map((s) => parseInt(s.trim(), 10)),
    samples: get("--samples")
      ? get("--samples")!.split(",").map((s) => parseInt(s.trim(), 10))
      : undefined,
    resume: has("--resume"),
    extract: has("--extract"),
    extractModel: get("--extract-model") ?? get("--model") ?? "gpt-4o-mini",
  };
}

// ── Checkpoint ──

function checkpointPath(outputDir: string): string {
  return path.join(outputDir, "checkpoint.json");
}

function experimentKey(config: ExperimentConfig): string {
  return `${config.searchMode.mode}:${config.topK}`;
}

function loadCheckpoint(outputDir: string): Checkpoint {
  const p = checkpointPath(outputDir);
  if (fs.existsSync(p)) {
    return JSON.parse(fs.readFileSync(p, "utf-8"));
  }
  return { completedExperiments: [], results: {} };
}

function saveCheckpoint(outputDir: string, checkpoint: Checkpoint): void {
  fs.mkdirSync(outputDir, { recursive: true });
  fs.writeFileSync(checkpointPath(outputDir), JSON.stringify(checkpoint, null, 2));
}

function clearCheckpoint(outputDir: string): void {
  const p = checkpointPath(outputDir);
  if (fs.existsSync(p)) fs.unlinkSync(p);
}

// ── Main ──

async function main() {
  const config = parseArgs(process.argv);

  console.log("LOCOMO Benchmark Harness for MAST");
  if (config.recallOnly) {
    console.log("Mode: recall-only (no LLM — measuring retrieval metrics only)");
  } else {
    console.log(`Model: ${config.model}`);
  }
  console.log(`Search modes: ${config.searchModes.map((s) => s.mode).join(", ")}`);
  console.log(`Top-K values: ${config.topKs.join(", ")}`);
  if (config.extract) {
    console.log(`Fact extraction: ON (model: ${config.extractModel})`);
  }
  if (!config.recallOnly) {
    console.log(`Concurrency: ${config.concurrency}`);
    if (config.judge) console.log(`Judge model: ${config.judgeModel}`);
  }

  // Load data
  const allSamples = await loadData(config.dataPath);
  const samples = config.samples
    ? allSamples.filter((s) => config.samples!.includes(s.id))
    : allSamples;
  console.log(`\nLoaded ${allSamples.length} samples, using ${samples.length}`);

  // Create MAST db
  const dbPath = path.join(config.outputDir, "mast.db");
  fs.mkdirSync(config.outputDir, { recursive: true });
  const db = new MastDb(dbPath, "eventual");

  // Ingest raw turns (always)
  ingestAll(db, samples);

  // Extract facts (with --extract)
  if (config.extract) {
    const extractLlm = createLLMClient({
      model: config.extractModel,
      apiKey: config.apiKey,
      baseUrl: config.baseUrl,
      maxTokens: 1024,
    });
    await extractAll(db, extractLlm, samples);
  }

  // Build experiment configs
  const experiments: ExperimentConfig[] = [];
  for (const searchMode of config.searchModes) {
    for (const topK of config.topKs) {
      experiments.push({ searchMode, topK });
    }
  }

  // LLM clients (undefined in recall-only mode)
  const llm = config.recallOnly
    ? undefined
    : createLLMClient({
        model: config.model,
        apiKey: config.apiKey,
        baseUrl: config.baseUrl,
      });
  const judgeLlm =
    !config.recallOnly && config.judge
      ? createLLMClient({
          model: config.judgeModel,
          apiKey: config.apiKey,
          baseUrl: config.baseUrl,
        })
      : undefined;

  // Checkpoint
  const checkpoint = config.resume ? loadCheckpoint(config.outputDir) : {
    completedExperiments: [] as string[],
    results: {} as Record<string, QAResult[]>,
  };

  // Run experiments
  const experimentResults: ExperimentResult[] = [];
  const totalStart = Date.now();

  for (const expConfig of experiments) {
    const key = experimentKey(expConfig);
    const modeName = expConfig.searchMode.mode;

    if (checkpoint.completedExperiments.includes(key)) {
      console.log(`\n[${modeName} k=${expConfig.topK}] Resuming from checkpoint`);
      const cached = checkpoint.results[key] ?? [];
      experimentResults.push(buildExperimentResult(expConfig, cached, 0));
      continue;
    }

    console.log(`\n[${modeName} k=${expConfig.topK}] Running...`);
    const expStart = Date.now();

    const allResults: QAResult[] = [];
    for (const sample of samples) {
      const totalQA = sample.qa.length;
      process.stdout.write(`  Sample ${sample.id} (${totalQA} questions)...`);

      const results = await evaluateSample(
        db,
        llm,
        sample,
        expConfig.searchMode,
        expConfig.topK,
        config.concurrency,
        judgeLlm,
        config.extract,
      );

      if (config.recallOnly) {
        const hitRate = results.filter((r) => r.recallHit).length / results.length;
        console.log(` Recall=${(hitRate * 100).toFixed(1)}%`);
      } else {
        const avgF1 = results.reduce((s, r) => s + r.f1, 0) / results.length;
        console.log(` F1=${avgF1.toFixed(3)}`);
      }
      allResults.push(...results);
    }

    const expResult = buildExperimentResult(expConfig, allResults, Date.now() - expStart);
    experimentResults.push(expResult);

    // Save checkpoint
    checkpoint.completedExperiments.push(key);
    checkpoint.results[key] = allResults;
    saveCheckpoint(config.outputDir, checkpoint);

    const elapsed = ((Date.now() - expStart) / 1000).toFixed(1);
    const recallPct = (expResult.overallRecallHit * 100).toFixed(1);
    if (config.recallOnly) {
      console.log(`  Overall Recall=${recallPct}% (${elapsed}s)`);
    } else {
      console.log(`  Overall F1=${expResult.overallF1.toFixed(3)} Recall=${recallPct}% (${elapsed}s)`);
    }
  }

  // Report
  const report: BenchmarkReport = {
    model: config.recallOnly ? "(recall-only)" : config.model,
    sampleCount: samples.length,
    experiments: experimentResults,
    totalDurationMs: Date.now() - totalStart,
  };

  writeResults(report, config.outputDir);
  clearCheckpoint(config.outputDir);

  db.close();
}

main().catch((err) => {
  console.error("Fatal error:", err);
  process.exit(1);
});
