import type { MastDb } from "mast-memory";
import type { LocomoSample, SearchModeConfig, QAResult } from "./types.js";
import type { LLMClient } from "./llm.js";
import { computeF1, computeBleu1, scoreAdversarial, judgeAnswer } from "./score.js";
import { collectionName } from "./ingest.js";
import { extractionCollectionName } from "./extract.js";

const ANSWER_SYSTEM = `You answer questions about conversations using ONLY the provided context.
Each context line starts with the date that part of the conversation happened.
Rules:
- Be concise and specific
- For temporal questions, use those dates. Resolve relative times ("last week", "yesterday") against the date of the line they appear in
- If multiple pieces of context are relevant, synthesize them
- If the information is not mentioned in the context, respond exactly: "Not mentioned."`;

/** Unix seconds → "8 May 2023", in UTC like the session dates. */
function formatDate(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleDateString("en-GB", {
    day: "numeric",
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  });
}

function formatContext(
  memories: Array<{ memory: { content: string; createdAt: number }; score: number }>,
): string {
  return memories
    .map((m, i) => `[${i + 1}] (${formatDate(m.memory.createdAt)}) ${m.memory.content}`)
    .join("\n");
}

function searchModeString(config: SearchModeConfig): string {
  return config.mode;
}

export async function evaluateQuestion(
  db: MastDb,
  llm: LLMClient | undefined,
  sample: LocomoSample,
  qaIndex: number,
  searchMode: SearchModeConfig,
  topK: number,
  judgeLlm?: LLMClient,
  useExtracted?: boolean,
): Promise<QAResult> {
  const qa = sample.qa[qaIndex];
  const collection = useExtracted
    ? extractionCollectionName(sample.id)
    : collectionName(sample.id);

  // Recall from MAST
  const query = searchMode.mode === "fulltext" ? undefined : qa.question;
  const textQuery = searchMode.mode === "vector" ? undefined : qa.question;
  const mode = searchModeString(searchMode);

  // Questions are asked after the last session.
  const sessions = sample.conversation.sessions;
  const referenceTime = sessions[sessions.length - 1].timestamp;
  const recalled = db.recallLocal(collection, query, textQuery, mode, topK, referenceTime);

  // Check recall hit — any evidence dia_id found in recalled memories
  const recalledIds: string[] = [];
  for (const r of recalled) {
    const diaId = r.memory.metadata.dia_id;
    if (diaId) {
      recalledIds.push(diaId);
    }
    const sourceDiaIds = r.memory.metadata.source_dia_ids;
    if (sourceDiaIds) {
      recalledIds.push(...sourceDiaIds.split(","));
    }
  }
  const evidenceSet = new Set(qa.evidence);
  const recallHit = recalledIds.some((id) => evidenceSet.has(id));

  let predictedAnswer = "";
  let f1 = 0;
  let bleu1 = 0;
  let judgeScore: number | undefined;

  if (llm) {
    // Generate answer via LLM
    const context = formatContext(recalled);
    const needsReasoning = qa.category === 2 || qa.category === 3;
    const prompt = needsReasoning
      ? `Context:\n${context}\n\nQuestion: ${qa.question}\n\nThink step by step, then give a concise final answer:`
      : `Context:\n${context}\n\nQuestion: ${qa.question}\n\nAnswer concisely:`;

    predictedAnswer = await llm.complete([
      { role: "system", content: ANSWER_SYSTEM },
      { role: "user", content: prompt },
    ]);

    if (qa.category === 5) {
      // Adversarial: binary rejection detection (matches LOCOMO eval)
      f1 = scoreAdversarial(predictedAnswer);
      bleu1 = f1;
    } else {
      f1 = computeF1(predictedAnswer, qa.answer);
      bleu1 = computeBleu1(predictedAnswer, qa.answer);
    }

    if (judgeLlm && qa.category !== 5) {
      judgeScore = await judgeAnswer(judgeLlm, qa.question, qa.answer, predictedAnswer);
    }
  }

  return {
    sampleId: sample.id,
    questionIndex: qaIndex,
    category: qa.category,
    question: qa.question,
    expectedAnswer: qa.answer,
    predictedAnswer,
    evidence: qa.evidence,
    recalledIds,
    recallHit,
    f1,
    bleu1,
    judgeScore,
  };
}

export async function evaluateSample(
  db: MastDb,
  llm: LLMClient | undefined,
  sample: LocomoSample,
  searchMode: SearchModeConfig,
  topK: number,
  concurrency: number,
  qaIndices: number[],
  judgeLlm?: LLMClient,
  useExtracted?: boolean,
): Promise<QAResult[]> {
  const results: QAResult[] = [];

  // Process in batches for concurrency control
  for (let i = 0; i < qaIndices.length; i += concurrency) {
    const batch = qaIndices.slice(i, i + concurrency);
    const batchResults = await Promise.all(
      batch.map((qi) =>
        evaluateQuestion(db, llm, sample, qi, searchMode, topK, judgeLlm, useExtracted),
      ),
    );
    results.push(...batchResults);
  }

  return results;
}
