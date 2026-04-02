import { existsSync, statSync, readdirSync } from "fs";
import { join } from "path";
import { MastDb } from "mast-memory";
import { DIMENSIONS } from "../config.js";
import type { BenchmarkSystem } from "./interface.js";

const DB_PATH = ".bench-data/mast";
const COLLECTION = "bench";

function dirSize(dir: string): number {
  if (!existsSync(dir)) return 0;
  let total = 0;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      total += dirSize(full);
    } else {
      total += statSync(full).size;
    }
  }
  return total;
}

// No-op embedder — we always pass pre-computed embeddings directly,
// so this callback is never actually invoked.
const noopEmbedFn = (_text: string): number[] => {
  throw new Error("embedFn should not be called when embedding is provided");
};

export class MastSystem implements BenchmarkSystem {
  name = "MAST";
  private db: MastDb | null = null;

  async setup(): Promise<void> {
    const { mkdirSync, rmSync } = await import("fs");
    if (existsSync(DB_PATH)) rmSync(DB_PATH, { recursive: true });
    mkdirSync(DB_PATH, { recursive: true });
    this.db = new MastDb(join(DB_PATH, "bench.db"), "eventual");
  }

  async store(
    id: string,
    text: string,
    embedding: number[],
    metadata?: Record<string, string>
  ): Promise<void> {
    const meta = { ...metadata, _bench_id: id };
    this.db!.store(
      COLLECTION, text, noopEmbedFn, DIMENSIONS, meta, null, null, embedding
    );
  }

  async storeBatch(
    items: {
      id: string;
      text: string;
      embedding: number[];
      metadata?: Record<string, string>;
    }[]
  ): Promise<void> {
    const batchItems = items.map(item => ({
      content: item.text,
      embedding: item.embedding,
      metadata: { ...item.metadata, _bench_id: item.id },
    }));
    this.db!.storeBatch(COLLECTION, batchItems, noopEmbedFn, DIMENSIONS);
  }

  async query(
    embedding: number[],
    limit: number
  ): Promise<{ id: string; score: number }[]> {
    const results = this.db!.recall(
      COLLECTION,
      noopEmbedFn,
      DIMENSIONS,
      null,       // query text
      null,       // text_query
      null,       // search_mode
      limit,
      embedding,  // query_embedding
    );
    return results.map((r) => ({
      id: r.memory.metadata._bench_id ?? r.memory.id,
      score: r.score,
    }));
  }

  async count(): Promise<number> {
    const infos = this.db!.info();
    const col = infos.find((c) => c.name === COLLECTION);
    return col ? Number(col.memoryCount) : 0;
  }

  async diskSize(): Promise<number> {
    return dirSize(DB_PATH);
  }

  async teardown(): Promise<void> {
    if (this.db) {
      this.db.close();
      this.db = null;
    }
  }
}
