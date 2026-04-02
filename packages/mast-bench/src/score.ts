import { porterStem } from "./stem.js";
import type { LLMClient } from "./llm.js";

/**
 * Matches LOCOMO's normalize_answer():
 * 1. Strip commas
 * 2. Lowercase
 * 3. Remove punctuation
 * 4. Remove articles (a, an, the, and)
 * 5. Collapse whitespace
 */
export function normalizeAnswer(text: string): string {
  let s = text.replace(/,/g, "");
  s = s.toLowerCase();
  s = s.replace(/[^\w\s]/g, "");
  s = s
    .split(/\s+/)
    .filter((w) => w.length > 0 && w !== "a" && w !== "an" && w !== "the" && w !== "and")
    .join(" ");
  return s;
}

function tokenizeAndStem(text: string): string[] {
  return normalizeAnswer(text)
    .split(/\s+/)
    .filter((w) => w.length > 0)
    .map(porterStem);
}

/**
 * Token-level F1 using Counter intersection (matches LOCOMO's f1_score).
 * Handles duplicate tokens correctly via count-based overlap.
 */
export function f1Score(predicted: string, expected: string): number {
  const predTokens = tokenizeAndStem(predicted);
  const expTokens = tokenizeAndStem(expected);

  if (predTokens.length === 0 && expTokens.length === 0) return 1;
  if (predTokens.length === 0 || expTokens.length === 0) return 0;

  // Counter intersection — same as Python's Counter & Counter
  const predCounts = new Map<string, number>();
  for (const t of predTokens) predCounts.set(t, (predCounts.get(t) ?? 0) + 1);

  const expCounts = new Map<string, number>();
  for (const t of expTokens) expCounts.set(t, (expCounts.get(t) ?? 0) + 1);

  let numSame = 0;
  for (const [token, count] of predCounts) {
    numSame += Math.min(count, expCounts.get(token) ?? 0);
  }

  if (numSame === 0) return 0;

  const precision = numSame / predTokens.length;
  const recall = numSame / expTokens.length;
  return (2 * precision * recall) / (precision + recall);
}

/**
 * Matches LOCOMO's f1() wrapper — handles comma-separated multi-answers.
 * Returns mean of max F1 across ground-truth parts.
 */
export function computeF1(predicted: string, expected: string): number {
  const predictions = predicted.split(",").map((s) => s.trim()).filter(Boolean);
  const groundTruths = expected.split(",").map((s) => s.trim()).filter(Boolean);

  if (predictions.length === 0 && groundTruths.length === 0) return 1;
  if (predictions.length === 0 || groundTruths.length === 0) return 0;

  const scores = groundTruths.map((gt) =>
    Math.max(...predictions.map((pred) => f1Score(pred, gt))),
  );
  return scores.reduce((s, v) => s + v, 0) / scores.length;
}

/**
 * Cat5 adversarial: binary rejection detection.
 * Matches LOCOMO's eval — checks for "no information available" or "not mentioned".
 */
const REJECTION_PHRASES = ["no information available", "not mentioned"];

export function scoreAdversarial(predicted: string): number {
  const lower = predicted.toLowerCase();
  return REJECTION_PHRASES.some((p) => lower.includes(p)) ? 1 : 0;
}

export function computeBleu1(predicted: string, expected: string): number {
  const predTokens = tokenizeAndStem(predicted);
  const expTokens = tokenizeAndStem(expected);

  if (predTokens.length === 0 && expTokens.length === 0) return 1;
  if (predTokens.length === 0 || expTokens.length === 0) return 0;

  const expCounts = new Map<string, number>();
  for (const t of expTokens) expCounts.set(t, (expCounts.get(t) ?? 0) + 1);

  const predCounts = new Map<string, number>();
  for (const t of predTokens) predCounts.set(t, (predCounts.get(t) ?? 0) + 1);

  let matchCount = 0;
  for (const [token, count] of predCounts) {
    matchCount += Math.min(count, expCounts.get(token) ?? 0);
  }

  const precision = matchCount / predTokens.length;
  const bp =
    predTokens.length >= expTokens.length
      ? 1
      : Math.exp(1 - expTokens.length / predTokens.length);

  return bp * precision;
}

const JUDGE_SYSTEM = `You are an answer evaluator. Given a question, the ground-truth answer, and a predicted answer, rate the predicted answer's correctness on a scale of 0-100.

Respond with ONLY an integer between 0 and 100. No other text.

Scoring guide:
- 100: Perfect match or semantically identical
- 70-99: Mostly correct with minor omissions
- 30-69: Partially correct
- 1-29: Mostly wrong but has some relevant info
- 0: Completely wrong or irrelevant`;

export async function judgeAnswer(
  llm: LLMClient,
  question: string,
  expected: string,
  predicted: string,
): Promise<number> {
  const prompt = `Question: ${question}\nGround-truth answer: ${expected}\nPredicted answer: ${predicted}\n\nScore (0-100):`;

  const response = await llm.complete([
    { role: "system", content: JUDGE_SYSTEM },
    { role: "user", content: prompt },
  ]);

  const match = response.match(/\d+/);
  if (!match) return 0;
  return Math.min(100, Math.max(0, parseInt(match[0], 10)));
}
