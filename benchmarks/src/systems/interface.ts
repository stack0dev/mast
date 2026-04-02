export interface BenchmarkSystem {
  name: string;
  setup(): Promise<void>;
  store(
    id: string,
    text: string,
    embedding: number[],
    metadata?: Record<string, string>
  ): Promise<void>;
  storeBatch(
    items: {
      id: string;
      text: string;
      embedding: number[];
      metadata?: Record<string, string>;
    }[]
  ): Promise<void>;
  query(
    embedding: number[],
    limit: number,
    queryText?: string
  ): Promise<{ id: string; score: number }[]>;
  count(): Promise<number>;
  diskSize(): Promise<number>;
  teardown(): Promise<void>;
}
