// TODO: Remove this file once mast-memory is published to npm.
// Types will come from the published package's index.d.ts.
declare module "mast-memory" {
  export interface JsMemory {
    id: string;
    collection: string;
    content: string;
    metadata: Record<string, string>;
    tier: string;
    createdAt: number;
    accessCount: number;
    lastAccessed: number;
  }

  export interface JsRecallResult {
    memory: JsMemory;
    score: number;
  }

  export interface JsEdge {
    id: string;
    source: string;
    target: string;
    relation: string;
    weight: number;
  }

  export interface JsCollectionInfo {
    name: string;
    dimensions: number;
    memoryCount: number;
  }

  export class MastDb {
    constructor(path: string);

    store(
      collection: string,
      content: string,
      embedFn: (text: string) => Promise<number[]>,
      dimensions: number,
      metadata?: Record<string, string>,
      tier?: string,
      ttl?: number
    ): JsMemory;

    storeAsync(
      collection: string,
      content: string,
      embedFn: (text: string) => Promise<number[]>,
      dimensions: number,
      metadata?: Record<string, string>,
      tier?: string,
      ttl?: number
    ): Promise<JsMemory>;

    recall(
      collection: string,
      embedFn: (text: string) => Promise<number[]>,
      dimensions: number,
      query?: string,
      textQuery?: string,
      searchMode?: string,
      limit?: number
    ): JsRecallResult[];

    recallAsync(
      collection: string,
      embedFn: (text: string) => Promise<number[]>,
      dimensions: number,
      query?: string,
      textQuery?: string,
      searchMode?: string,
      limit?: number
    ): Promise<JsRecallResult[]>;

    get(collection: string, id: string): JsMemory | null;
    delete(collection: string, id: string): void;
    list(collection: string): JsMemory[];
    info(): JsCollectionInfo[];
    vacuum(collection: string): number;

    relate(
      collection: string,
      source: string,
      target: string,
      relation: string,
      weight?: number
    ): string;

    unrelate(
      collection: string,
      source: string,
      target: string,
      relation: string
    ): void;

    traverse(
      collection: string,
      start: string,
      maxDepth?: number,
      relation?: string,
      minWeight?: number
    ): JsEdge[];

    subgraph(collection: string): JsEdge[];

    close(): void;
  }
}
