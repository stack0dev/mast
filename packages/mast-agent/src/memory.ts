import { MastDb } from "mast-memory";

export interface StoredMemory {
  id: string;
  content: string;
  collection: string;
}

export interface RecalledMemory {
  id: string;
  content: string;
  score: number;
  metadata: Record<string, string>;
  tier: string;
}

export type EmbedFn = (text: string) => Promise<number[]> | number[];

export interface MemoryManagerOptions {
  dbPath: string;
  durability: "immediate" | "eventual";
  embedFn?: EmbedFn;
  dimensions?: number;
  useLocalEmbedder?: boolean;
}

export interface MemoryManager {
  store(collection: string, content: string, metadata?: Record<string, string>, tier?: string): Promise<StoredMemory>;
  recall(collection: string, query: string, limit?: number): Promise<RecalledMemory[]>;
  forget(collection: string, id: string): Promise<void>;
  getCollection(channel: string, userId: string): string;
  close(): void;
}

export function createMemoryManager(opts: MemoryManagerOptions): MemoryManager {
  const { dbPath, durability, embedFn, dimensions, useLocalEmbedder } = opts;

  if (!useLocalEmbedder && !embedFn) {
    throw new Error("Either embedFn + dimensions or useLocalEmbedder must be provided");
  }
  if (embedFn && !dimensions) {
    throw new Error("dimensions is required when using embedFn");
  }

  const db = new MastDb(dbPath, durability);

  return {
    async store(
      collection: string,
      content: string,
      metadata?: Record<string, string>,
      tier?: string,
    ): Promise<StoredMemory> {
      let result;
      if (useLocalEmbedder) {
        result = db.storeLocal(collection, content, metadata, tier);
      } else {
        result = db.store(
          collection,
          content,
          embedFn!,
          dimensions!,
          metadata,
          tier,
        );
      }
      return {
        id: result.id,
        content: result.content,
        collection: result.collection,
      };
    },

    async recall(
      collection: string,
      query: string,
      limit?: number,
    ): Promise<RecalledMemory[]> {
      let results;
      if (useLocalEmbedder) {
        results = db.recallLocal(collection, query, undefined, undefined, limit);
      } else if (embedFn) {
        results = db.recall(
          collection,
          embedFn,
          dimensions!,
          query,
          undefined,
          undefined,
          limit,
        );
      } else {
        // No embedder — fall back to fulltext search
        results = db.recallLocal(collection, undefined, query, "fulltext", limit);
      }

      return results.map((r: { memory: { id: string; content: string; metadata: Record<string, string>; tier: string }; score: number }) => ({
        id: r.memory.id,
        content: r.memory.content,
        score: r.score,
        metadata: r.memory.metadata,
        tier: r.memory.tier,
      }));
    },

    async forget(collection: string, id: string): Promise<void> {
      db.delete(collection, id);
    },

    getCollection(channel: string, userId: string): string {
      return `${channel}:${userId}`;
    },

    close(): void {
      db.close();
    },
  };
}
