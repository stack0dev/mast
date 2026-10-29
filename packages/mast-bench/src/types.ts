// ── LOCOMO dataset types ──

export interface LocomoTurn {
  speaker: string;
  dia_id: string;
  text: string;
  blip_caption?: string;
}

export interface LocomoQA {
  question: string;
  answer: string;
  evidence: string[];
  category: number; // 1-5
  adversarial_answer?: string;
}

export interface LocomoSession {
  /** As written in the dataset, e.g. "1:56 pm on 8 May, 2023". */
  dateTime: string;
  /** The same time in unix seconds, read as UTC. */
  timestamp: number;
  turns: LocomoTurn[];
}

export interface LocomoSample {
  id: number;
  conversation: {
    speaker_a: string;
    speaker_b: string;
    sessions: LocomoSession[];
  };
  qa: LocomoQA[];
}

// ── Experiment configuration ──

export type SearchModeConfig =
  | { mode: "vector" }
  | { mode: "fulltext" }
  | { mode: "hybrid" };

export interface ExperimentConfig {
  searchMode: SearchModeConfig;
  topK: number;
}

/** Which questions to run. Tune on "dev"; report "held-out". */
export type SplitName = "dev" | "held-out" | "all";

export interface RunConfig {
  dataPath: string;
  outputDir: string;
  model: string;
  baseUrl?: string;
  apiKey: string;
  recallOnly: boolean;
  judge: boolean;
  judgeModel: string;
  concurrency: number;
  searchModes: SearchModeConfig[];
  topKs: number[];
  samples?: number[];
  resume: boolean;
  extract: boolean;
  extractModel: string;
  split: SplitName;
  /** Copy the results into results/<record>/ to commit them. */
  record?: string;
}

// ── Results ──

export interface QAResult {
  sampleId: number;
  questionIndex: number;
  category: number;
  question: string;
  expectedAnswer: string;
  predictedAnswer: string;
  evidence: string[];
  recalledIds: string[];
  recallHit: boolean;
  f1: number;
  bleu1: number;
  judgeScore?: number;
}

export interface CategoryBreakdown {
  category: number;
  count: number;
  meanF1: number;
  meanBleu1: number;
  meanJudge?: number;
  recallHitRate: number;
}

export interface ExperimentResult {
  config: ExperimentConfig;
  results: QAResult[];
  categories: CategoryBreakdown[];
  overallF1: number;
  overallBleu1: number;
  overallJudge?: number;
  overallRecallHit: number;
  durationMs: number;
}

export interface BenchmarkReport {
  model: string;
  split: SplitName;
  sampleCount: number;
  experiments: ExperimentResult[];
  totalDurationMs: number;
}

// ── Checkpoint ──

export interface Checkpoint {
  completedExperiments: string[]; // "searchMode:topK" keys
  results: Record<string, QAResult[]>;
}
