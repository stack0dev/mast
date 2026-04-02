import { ChromaClient } from "chromadb";
import type { BenchmarkSystem } from "./interface.js";

const COLLECTION = "bench";

export class ChromaSystem implements BenchmarkSystem {
  name = "ChromaDB";
  private client: ChromaClient | null = null;
  private collection: any = null;
  // Track stored IDs in order for count/query mapping
  private storedIds: string[] = [];

  async setup(): Promise<void> {
    this.client = new ChromaClient({ path: "http://localhost:8000" });
    // Delete existing collection if present
    try {
      await this.client.deleteCollection({ name: COLLECTION });
    } catch {
      // ignore
    }
    this.collection = await this.client.createCollection({ name: COLLECTION });
    this.storedIds = [];
  }

  async store(
    id: string,
    text: string,
    embedding: number[],
    metadata?: Record<string, string>
  ): Promise<void> {
    await this.collection.add({
      ids: [id],
      documents: [text],
      embeddings: [embedding],
      metadatas: metadata ? [metadata] : undefined,
    });
    this.storedIds.push(id);
  }

  async storeBatch(
    items: {
      id: string;
      text: string;
      embedding: number[];
      metadata?: Record<string, string>;
    }[]
  ): Promise<void> {
    // ChromaDB supports batch add
    await this.collection.add({
      ids: items.map((i) => i.id),
      documents: items.map((i) => i.text),
      embeddings: items.map((i) => i.embedding),
      metadatas: items.map((i) => i.metadata ?? {}),
    });
    this.storedIds.push(...items.map((i) => i.id));
  }

  async query(
    embedding: number[],
    limit: number
  ): Promise<{ id: string; score: number }[]> {
    const results = await this.collection.query({
      queryEmbeddings: [embedding],
      nResults: limit,
    });
    const ids: string[] = results.ids[0] ?? [];
    const distances: number[] = results.distances?.[0] ?? [];
    return ids.map((id, i) => ({
      id,
      // ChromaDB returns L2 distances by default; convert to similarity
      score: 1 / (1 + distances[i]),
    }));
  }

  async count(): Promise<number> {
    return await this.collection.count();
  }

  async diskSize(): Promise<number> {
    // ChromaDB is a server; disk size not directly measurable from client
    return 0;
  }

  async teardown(): Promise<void> {
    if (this.client && this.collection) {
      try {
        await this.client.deleteCollection({ name: COLLECTION });
      } catch {
        // ignore
      }
    }
    this.client = null;
    this.collection = null;
    this.storedIds = [];
  }
}
