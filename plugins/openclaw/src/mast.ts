import type { MastDb as NativeMastDb } from "mast-memory";
import type { PluginConfig, EmbedFn } from "./types";
import { createEmbedFn } from "./embedder";

// Lazy-loaded native module to avoid import-time failures
let MastDbClass: typeof NativeMastDb | null = null;

function getNativeMastDb(): typeof NativeMastDb {
  if (!MastDbClass) {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    MastDbClass = require("mast-memory").MastDb;
  }
  return MastDbClass!;
}

export class MastClient {
  private db: NativeMastDb | null = null;
  private readonly config: PluginConfig;
  private readonly embedFn: EmbedFn;

  constructor(config: PluginConfig) {
    this.config = config;
    this.embedFn = createEmbedFn(config);
  }

  private getDb(): NativeMastDb {
    if (!this.db) {
      const Cls = getNativeMastDb();
      this.db = new Cls(this.config.dbPath);
    }
    return this.db;
  }

  async store(
    content: string,
    metadata?: Record<string, string>,
    tier?: string,
    ttl?: number
  ) {
    const db = this.getDb();
    return db.storeAsync(
      this.config.collection,
      content,
      this.embedFn,
      this.config.dimensions,
      metadata ?? undefined,
      tier ?? undefined,
      ttl ?? undefined
    );
  }

  async search(
    query: string,
    limit?: number,
    mode?: string
  ) {
    const db = this.getDb();
    const searchMode = mode ?? this.config.searchMode;
    // For fulltext-only, no vector query needed
    const vectorQuery =
      searchMode === "fulltext" ? undefined : query;
    const textQuery =
      searchMode === "vector" ? undefined : query;

    return db.recallAsync(
      this.config.collection,
      this.embedFn,
      this.config.dimensions,
      vectorQuery,
      textQuery,
      searchMode,
      limit ?? this.config.maxResults
    );
  }

  delete(id: string) {
    return this.getDb().delete(this.config.collection, id);
  }

  relate(
    source: string,
    target: string,
    relation: string,
    weight?: number
  ) {
    return this.getDb().relate(
      this.config.collection,
      source,
      target,
      relation,
      weight
    );
  }

  traverse(
    start: string,
    maxDepth?: number,
    relation?: string,
    minWeight?: number
  ) {
    return this.getDb().traverse(
      this.config.collection,
      start,
      maxDepth,
      relation,
      minWeight
    );
  }

  subgraph() {
    return this.getDb().subgraph(this.config.collection);
  }

  close() {
    if (this.db) {
      this.db.close();
      this.db = null;
    }
  }
}
