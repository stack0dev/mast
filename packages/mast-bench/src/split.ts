// Fixed dev / held-out split of the LOCOMO questions.
//
// Tune retrieval on "dev". Run "held-out" once per change you intend to report;
// any tuning after looking at held-out results makes them dev results.
//
// The split lives in splits/locomo10.json and is committed. Regenerate it only
// when the dataset changes: `npm run split`.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import type { LocomoSample, SplitName } from "./types.js";

const SEED = 20261029;
const DEV_FRACTION = 0.2;

export const SPLIT_PATH = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
  "splits",
  "locomo10.json",
);

interface SplitFile {
  seed: number;
  devFraction: number;
  /** "<sampleId>:<questionIndex>" for every dev question. */
  dev: string[];
}

export function questionKey(sampleId: number, questionIndex: number): string {
  return `${sampleId}:${questionIndex}`;
}

/** Deterministic PRNG (mulberry32), so the split is reproducible. */
function mulberry32(seed: number): () => number {
  let a = seed;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Take DEV_FRACTION of each category, so dev covers every question type. */
export function makeSplit(samples: LocomoSample[]): SplitFile {
  const byCategory = new Map<number, string[]>();
  for (const sample of samples) {
    sample.qa.forEach((qa, i) => {
      const keys = byCategory.get(qa.category) ?? [];
      keys.push(questionKey(sample.id, i));
      byCategory.set(qa.category, keys);
    });
  }

  const random = mulberry32(SEED);
  const dev: string[] = [];
  for (const category of [...byCategory.keys()].sort((a, b) => a - b)) {
    const keys = byCategory.get(category)!;
    for (let i = keys.length - 1; i > 0; i--) {
      const j = Math.floor(random() * (i + 1));
      [keys[i], keys[j]] = [keys[j], keys[i]];
    }
    dev.push(...keys.slice(0, Math.ceil(keys.length * DEV_FRACTION)));
  }
  dev.sort();
  return { seed: SEED, devFraction: DEV_FRACTION, dev };
}

/** Question indices of `sample` that belong to `split`. */
export function selectQuestions(
  sample: LocomoSample,
  split: SplitName,
  devKeys: Set<string>,
): number[] {
  const all = sample.qa.map((_, i) => i);
  if (split === "all") return all;
  const wantDev = split === "dev";
  return all.filter((i) => devKeys.has(questionKey(sample.id, i)) === wantDev);
}

export function loadDevKeys(): Set<string> {
  if (!fs.existsSync(SPLIT_PATH)) {
    throw new Error(`Split file missing: ${SPLIT_PATH}. Run \`npm run split\`.`);
  }
  const file: SplitFile = JSON.parse(fs.readFileSync(SPLIT_PATH, "utf-8"));
  return new Set(file.dev);
}

// `tsx src/split.ts [path/to/locomo10.json]` writes splits/locomo10.json.
if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  const { loadData } = await import("./data.js");
  const dataPath = process.argv[2] ?? path.join("bench-results", "locomo10.json");
  const split = makeSplit(await loadData(dataPath));
  fs.mkdirSync(path.dirname(SPLIT_PATH), { recursive: true });
  fs.writeFileSync(SPLIT_PATH, JSON.stringify(split, null, 2) + "\n");
  console.log(`Wrote ${split.dev.length} dev questions to ${SPLIT_PATH}`);
}
