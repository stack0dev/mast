import { existsSync, statSync, readdirSync, rmSync, mkdirSync } from "fs";
import { join } from "path";
import * as lancedb from "@lancedb/lancedb";
import type { BenchmarkSystem } from "./interface.js";

const DB_PATH = ".bench-data/lance";
const TABLE_NAME = "bench";

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

export class LanceSystem implements BenchmarkSystem {
  name = "LanceDB";
  private db: lancedb.Connection | null = null;
  private table: lancedb.Table | null = null;
  private rowCount = 0;

  async setup(): Promise<void> {
    if (existsSync(DB_PATH)) rmSync(DB_PATH, { recursive: true });
    mkdirSync(DB_PATH, { recursive: true });
    this.db = await lancedb.connect(DB_PATH);
    this.rowCount = 0;
  }

  async store(
    id: string,
    text: string,
    embedding: number[],
    metadata?: Record<string, string>
  ): Promise<void> {
    const row = { id, text, vector: embedding, ...metadata };
    if (!this.table) {
      this.table = await this.db!.createTable(TABLE_NAME, [row]);
    } else {
      await this.table.add([row]);
    }
    this.rowCount++;
  }

  async storeBatch(
    items: {
      id: string;
      text: string;
      embedding: number[];
      metadata?: Record<string, string>;
    }[]
  ): Promise<void> {
    const rows = items.map((i) => ({
      id: i.id,
      text: i.text,
      vector: i.embedding,
      ...i.metadata,
    }));
    if (!this.table) {
      this.table = await this.db!.createTable(TABLE_NAME, rows);
    } else {
      await this.table.add(rows);
    }
    this.rowCount += items.length;
  }

  async query(
    embedding: number[],
    limit: number
  ): Promise<{ id: string; score: number }[]> {
    const results = await this.table!.search(embedding).limit(limit).toArray();
    return results.map((r: any) => ({
      id: r.id as string,
      // LanceDB returns _distance (L2); convert to similarity
      score: 1 / (1 + (r._distance ?? 0)),
    }));
  }

  async count(): Promise<number> {
    return this.rowCount;
  }

  async diskSize(): Promise<number> {
    return dirSize(DB_PATH);
  }

  async teardown(): Promise<void> {
    this.db = null;
    this.table = null;
    this.rowCount = 0;
  }
}
