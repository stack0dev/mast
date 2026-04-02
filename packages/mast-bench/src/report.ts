import fs from "node:fs";
import path from "node:path";
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

export function formatSummary(report: BenchmarkReport): string {
  const lines: string[] = [];
  lines.push("LOCOMO Benchmark — MAST Results");
  lines.push(`Model: ${report.model} | Samples: ${report.sampleCount}`);
  lines.push(`Total time: ${(report.totalDurationMs / 1000).toFixed(1)}s`);
  lines.push("");

  const recallOnly = report.model === "(recall-only)";
  const hasJudge = report.experiments.some((e) => e.overallJudge != null);

  const catNums = [1, 2, 3, 4, 5];

  if (recallOnly) {
    // Recall-only: show per-category recall hit rates
    let header = `${pad("Search", 10)} ${pad("TopK", 5)}`;
    for (const c of catNums) header += ` ${pad(`Cat${c}`, 7)}`;
    header += ` ${pad("Overall", 8)}`;
    lines.push(header);
    lines.push("─".repeat(header.length));

    for (const exp of report.experiments) {
      let row = `${pad(exp.config.searchMode.mode, 10)} ${pad(String(exp.config.topK), 5)}`;
      for (const catNum of catNums) {
        const cat = exp.categories.find((c) => c.category === catNum);
        row += ` ${pad(cat ? fmtPct(cat.recallHitRate) : "  -  ", 7)}`;
      }
      row += ` ${pad(fmtPct(exp.overallRecallHit), 8)}`;
      lines.push(row);
    }
  } else {
    // Full mode: F1 per category + recall + optional judge
    let header = `${pad("Search", 10)} ${pad("TopK", 5)}`;
    for (const c of catNums) header += ` ${pad(`Cat${c}`, 7)}`;
    header += ` ${pad("Overall", 8)} ${pad("Recall%", 8)}`;
    if (hasJudge) header += ` ${pad("Judge", 6)}`;
    lines.push(header);
    lines.push("─".repeat(header.length));

    for (const exp of report.experiments) {
      let row = `${pad(exp.config.searchMode.mode, 10)} ${pad(String(exp.config.topK), 5)}`;
      for (const catNum of catNums) {
        const cat = exp.categories.find((c) => c.category === catNum);
        row += ` ${pad(cat ? fmtScore(cat.meanF1) : "  -  ", 7)}`;
      }
      row += ` ${pad(fmtScore(exp.overallF1), 8)} ${pad(fmtPct(exp.overallRecallHit), 8)}`;
      if (hasJudge) row += ` ${pad(exp.overallJudge != null ? fmtScore(exp.overallJudge) : "-", 6)}`;
      lines.push(row);
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
    sampleCount: report.sampleCount,
    totalDurationMs: report.totalDurationMs,
    experiments: report.experiments.map((e) => ({
      config: e.config,
      categories: e.categories,
      overallF1: e.overallF1,
      overallBleu1: e.overallBleu1,
      overallJudge: e.overallJudge,
      overallRecallHit: e.overallRecallHit,
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
