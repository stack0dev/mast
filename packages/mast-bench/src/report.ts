import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import type {
  QAResult,
  ExperimentResult,
  CategoryBreakdown,
  BenchmarkReport,
  ExperimentConfig,
} from "./types.js";

function mean(values: number[]): number {
  if (values.length === 0) return 0;
  return values.reduce((s, v) => s + v, 0) / values.length;
}

export function computeCategories(results: QAResult[]): CategoryBreakdown[] {
  const groups = new Map<number, QAResult[]>();
  for (const r of results) {
    const cat = r.category;
    if (!groups.has(cat)) groups.set(cat, []);
    groups.get(cat)!.push(r);
  }

  const categories: CategoryBreakdown[] = [];
  for (const [category, items] of [...groups.entries()].sort((a, b) => a[0] - b[0])) {
    const breakdown: CategoryBreakdown = {
      category,
      count: items.length,
      meanF1: mean(items.map((r) => r.f1)),
      meanBleu1: mean(items.map((r) => r.bleu1)),
      recallHitRate: mean(items.map((r) => (r.recallHit ? 1 : 0))),
      sessionHitRate: mean(items.map((r) => (r.sessionHit ? 1 : 0))),
    };
    const judgeScores = items.filter((r) => r.judgeScore != null).map((r) => r.judgeScore!);
    if (judgeScores.length > 0) breakdown.meanJudge = mean(judgeScores);
    categories.push(breakdown);
  }

  return categories;
}

export function buildExperimentResult(
  config: ExperimentConfig,
  results: QAResult[],
  durationMs: number,
): ExperimentResult {
  const categories = computeCategories(results);
  const hasJudge = results.some((r) => r.judgeScore != null);

  return {
    config,
    results,
    categories,
    overallF1: mean(results.map((r) => r.f1)),
    overallBleu1: mean(results.map((r) => r.bleu1)),
    ...(hasJudge ? { overallJudge: mean(results.map((r) => r.judgeScore ?? 0)) } : {}),
    overallRecallHit: mean(results.map((r) => (r.recallHit ? 1 : 0))),
    overallSessionHit: mean(results.map((r) => (r.sessionHit ? 1 : 0))),
    durationMs,
  };
}

function pad(s: string, len: number): string {
  return s.padEnd(len);
}

function fmtPct(n: number): string {
  return (n * 100).toFixed(1) + "%";
}

function fmtScore(n: number): string {
  return n.toFixed(3);
}

const CATEGORY_NAMES: Record<number, string> = {
  1: "Multi-hop",
  2: "Temporal",
  3: "Open",
  4: "Single",
  5: "Advers.",
};
const CATEGORIES = [1, 2, 3, 4, 5];

/** One row per experiment: a value per category, then overall. */
function table(
  report: BenchmarkReport,
  perCategory: (cat: CategoryBreakdown) => string,
  overall: (exp: ExperimentResult) => string,
): string[] {
  let header = `${pad("Search", 10)} ${pad("TopK", 5)}`;
  for (const c of CATEGORIES) header += ` ${pad(CATEGORY_NAMES[c], 9)}`;
  header += ` ${pad("Overall", 8)}`;
  const lines = [header, "─".repeat(header.length)];

  for (const exp of report.experiments) {
    let row = `${pad(exp.config.searchMode.mode, 10)} ${pad(String(exp.config.topK), 5)}`;
    for (const c of CATEGORIES) {
      const cat = exp.categories.find((x) => x.category === c);
      row += ` ${pad(cat ? perCategory(cat) : "-", 9)}`;
    }
    row += ` ${pad(overall(exp), 8)}`;
    lines.push(row);
  }
  return lines;
}

/**
 * Retrieval and answer quality are separate tables: retrieval recall says whether an
 * evidence turn was in the top-K; answer scores say whether the model answered right.
 */
export function formatSummary(report: BenchmarkReport): string {
  const questions = report.experiments[0]?.results.length ?? 0;
  const lines: string[] = [];
  lines.push("LOCOMO Benchmark — MAST Results");
  lines.push(
    `Split: ${report.split} (${questions} questions) | Samples: ${report.sampleCount} | Model: ${report.model}`,
  );
  lines.push(`Total time: ${(report.totalDurationMs / 1000).toFixed(1)}s`);
  lines.push("");

  lines.push("Turn recall (an evidence turn is in the top K turns)");
  lines.push(
    ...table(
      report,
      (cat) => fmtPct(cat.recallHitRate),
      (exp) => fmtPct(exp.overallRecallHit),
    ),
  );

  lines.push("");
  lines.push("Session recall (an evidence session is in the top K sessions)");
  lines.push(
    ...table(
      report,
      (cat) => fmtPct(cat.sessionHitRate),
      (exp) => fmtPct(exp.overallSessionHit),
    ),
  );

  if (report.model !== "(recall-only)") {
    lines.push("");
    lines.push("Answer F1 (adversarial: 1 when the model declines)");
    lines.push(
      ...table(
        report,
        (cat) => fmtScore(cat.meanF1),
        (exp) => fmtScore(exp.overallF1),
      ),
    );

    if (report.experiments.some((e) => e.overallJudge != null)) {
      lines.push("");
      lines.push("LLM judge (adversarial not judged)");
      lines.push(
        ...table(
          report,
          (cat) => (cat.meanJudge != null ? fmtScore(cat.meanJudge) : "-"),
          (exp) => (exp.overallJudge != null ? fmtScore(exp.overallJudge) : "-"),
        ),
      );
    }
  }

  return lines.join("\n");
}

export function writeResults(
  report: BenchmarkReport,
  outputDir: string,
): void {
  fs.mkdirSync(outputDir, { recursive: true });

  // Full results JSON (strip raw results to keep file manageable)
  const summary = {
    model: report.model,
    split: report.split,
    sampleCount: report.sampleCount,
    totalDurationMs: report.totalDurationMs,
    experiments: report.experiments.map((e) => ({
      config: e.config,
      categories: e.categories,
      overallF1: e.overallF1,
      overallBleu1: e.overallBleu1,
      overallJudge: e.overallJudge,
      overallRecallHit: e.overallRecallHit,
      overallSessionHit: e.overallSessionHit,
      durationMs: e.durationMs,
      questionCount: e.results.length,
    })),
  };
  fs.writeFileSync(
    path.join(outputDir, "results.json"),
    JSON.stringify(summary, null, 2),
  );

  // Detailed per-question results
  const detailed = report.experiments.flatMap((e) =>
    e.results.map((r) => ({
      searchMode: e.config.searchMode.mode,
      topK: e.config.topK,
      ...r,
    })),
  );
  fs.writeFileSync(
    path.join(outputDir, "detailed.json"),
    JSON.stringify(detailed, null, 2),
  );

  // ASCII summary
  const text = formatSummary(report);
  fs.writeFileSync(path.join(outputDir, "summary.txt"), text);

  console.log("\n" + text);
  console.log(`\nResults written to ${outputDir}/`);
}

const RECORDED_FILES = ["results.json", "detailed.json", "summary.txt"];

/** Copy a run's results into results/<name>/ so they can be committed. */
export function recordResults(outputDir: string, name: string): void {
  if (!/^[\w.-]+$/.test(name)) {
    throw new Error(`--record name may only use letters, digits, ".", "_" and "-": ${name}`);
  }
  const dest = path.join(path.dirname(fileURLToPath(import.meta.url)), "..", "results", name);
  fs.mkdirSync(dest, { recursive: true });
  for (const file of RECORDED_FILES) {
    fs.copyFileSync(path.join(outputDir, file), path.join(dest, file));
  }
  console.log(`Recorded results in ${dest}/`);
}
