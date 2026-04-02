import { existsSync, statSync, readdirSync } from "fs";
import { join } from "path";
import { MastDb } from "mast-memory";
import type { BenchmarkSystem } from "./interface.js";

const DB_PATH = ".bench-data/mast-local";
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

/**
 * MAST system using the built-in local embedder (fastembed).
 * Requires mast-node built with the `local-embedder` feature flag.
 * Measures end-to-end performance including embedding time.
 */
export class MastLocalSystem implements BenchmarkSystem {
  name = "MAST (local)";
  private db: MastDb | null = null;

  async setup(): Promise<void> {
    const { mkdirSync, rmSync } = await import("fs");
    if (existsSync(DB_PATH)) rmSync(DB_PATH, { recursive: true });
    mkdirSync(DB_PATH, { recursive: true });
    this.db = new MastDb(join(DB_PATH, "bench.db"));
  }

  async store(
    id: string,
    text: string,
    _embedding: number[],
    metadata?: Record<string, string>
  ): Promise<void> {
    const db = this.db as any;
    if (typeof db.storeLocal !== "function") {
      throw new Error(
        "storeLocal not available — rebuild mast-node with `local-embedder` feature"
      );
    }
    const meta = { ...metadata, _bench_id: id };
    db.storeLocal(COLLECTION, text, meta, null, null);
  }

  async storeBatch(
    items: {
      id: string;
      text: string;
      embedding: number[];
      metadata?: Record<string, string>;
    }[]
  ): Promise<void> {
    for (const item of items) {
      const meta = { ...item.metadata, _bench_id: item.id };
      const db = this.db as any;
      db.storeLocal(COLLECTION, item.text, meta, null, null);
    }
  }

  async query(
    _embedding: number[],
    limit: number,
    queryText?: string
  ): Promise<{ id: string; score: number }[]> {
    const db = this.db as any;
    if (typeof db.recallLocal !== "function") {
      throw new Error(
        "recallLocal not available — rebuild mast-node with `local-embedder` feature"
      );
    }
    const text = queryText ?? "test query";
    const results = db.recallLocal(COLLECTION, text, null, null, limit);
    return results.map((r: any) => ({
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
