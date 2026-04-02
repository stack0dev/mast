import { createHash } from "crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "fs";
import { join } from "path";
import OpenAI from "openai";
import { CACHE_DIR, DIMENSIONS } from "./config.js";

const openai = new OpenAI();

function hashText(text: string): string {
  return createHash("sha256").update(text).digest("hex").slice(0, 16);
}

function getCachePath(hash: string): string {
  const dir = join(CACHE_DIR, "embeddings");
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
  return join(dir, `${hash}.json`);
}

function loadCached(hash: string): number[] | null {
  const path = getCachePath(hash);
  if (!existsSync(path)) return null;
  return JSON.parse(readFileSync(path, "utf-8"));
}

function saveCache(hash: string, embedding: number[]): void {
  writeFileSync(getCachePath(hash), JSON.stringify(embedding));
}

export async function embedAll(texts: string[]): Promise<number[][]> {
  const results: number[][] = new Array(texts.length);
  const uncachedIndices: number[] = [];
  const uncachedTexts: string[] = [];

  // Check cache first
  for (let i = 0; i < texts.length; i++) {
    const hash = hashText(texts[i]);
    const cached = loadCached(hash);
    if (cached) {
      results[i] = cached;
    } else {
      uncachedIndices.push(i);
      uncachedTexts.push(texts[i]);
    }
  }

  if (uncachedTexts.length === 0) {
    console.log(`  All ${texts.length} embeddings loaded from cache`);
    return results;
  }

  console.log(
    `  Embedding ${uncachedTexts.length} texts (${texts.length - uncachedTexts.length} cached)...`
  );

  // Batch embed in chunks of 100 (OpenAI limit is 2048 but smaller is safer)
  const BATCH_SIZE = 100;
  for (let start = 0; start < uncachedTexts.length; start += BATCH_SIZE) {
    const batch = uncachedTexts.slice(start, start + BATCH_SIZE);
    const response = await openai.embeddings.create({
      model: "text-embedding-3-small",
      input: batch,
      dimensions: DIMENSIONS,
    });

    for (let j = 0; j < response.data.length; j++) {
      const globalIdx = uncachedIndices[start + j];
      const embedding = response.data[j].embedding;
      results[globalIdx] = embedding;
      saveCache(hashText(texts[globalIdx]), embedding);
    }

    if (start + BATCH_SIZE < uncachedTexts.length) {
      // Brief pause between batches to respect rate limits
      await new Promise((r) => setTimeout(r, 200));
    }
  }

  return results;
}
