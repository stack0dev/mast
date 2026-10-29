import type { MastDb } from "mast-memory";
import type { LocomoSample } from "./types.js";
import type { LLMClient } from "./llm.js";
import { INGEST_VERSION } from "./ingest.js";

function extractionCollectionName(sampleId: number): string {
  return `locomo_${sampleId}_ext_v${INGEST_VERSION}`;
}

export { extractionCollectionName };

const EXTRACTION_PROMPT = `Extract all factual information from this conversation between {speaker_a} and {speaker_b}.
Include: events, dates, opinions, plans, relationships, preferences, emotions.
Output one fact per line. Be specific — include names, dates, and details.
Do not include greetings, small talk, or conversational filler.`;

function buildSessionText(
  sample: LocomoSample,
  sessionIndex: number,
): { text: string; diaIds: string[] } {
  const session = sample.conversation.sessions[sessionIndex];
  const diaIds: string[] = [];
  const lines: string[] = [`Conversation on ${session.dateTime}:`];

  for (const turn of session.turns) {
    diaIds.push(turn.dia_id);
    let line = `${turn.speaker}: ${turn.text}`;
    if (turn.blip_caption) line += ` [image: ${turn.blip_caption}]`;
    lines.push(line);
  }

  return { text: lines.join("\n"), diaIds };
}

export async function extractSample(
  db: MastDb,
  llm: LLMClient,
  sample: LocomoSample,
): Promise<number> {
  const collection = extractionCollectionName(sample.id);

  // Skip if already extracted
  const collections = db.info();
  const existing = collections.find((c) => c.name === collection);
  if (existing && existing.memoryCount > 0) {
    console.log(
      `  Sample ${sample.id}: already extracted (${existing.memoryCount} facts), skipping`,
    );
    return existing.memoryCount;
  }

  const { speaker_a, speaker_b } = sample.conversation;
  const systemPrompt = EXTRACTION_PROMPT
    .replace("{speaker_a}", speaker_a)
    .replace("{speaker_b}", speaker_b);

  let totalFacts = 0;

  for (let si = 0; si < sample.conversation.sessions.length; si++) {
    const { text, diaIds } = buildSessionText(sample, si);
    const sessionLabel = String(si + 1);
    const sourceDiaIds = diaIds.join(",");

    const response = await llm.complete([
      { role: "system", content: systemPrompt },
      { role: "user", content: text },
    ]);

    const facts = response
      .split("\n")
      .map((line) => line.replace(/^[-•*\d.)\s]+/, "").trim())
      .filter((line) => line.length > 0);

    for (const fact of facts) {
      db.storeLocal(
        collection,
        fact,
        { session: sessionLabel, source_dia_ids: sourceDiaIds, type: "extracted" },
        null,
        null,
        sample.conversation.sessions[si].timestamp,
      );
      totalFacts++;
    }
  }

  return totalFacts;
}

export async function extractAll(
  db: MastDb,
  llm: LLMClient,
  samples: LocomoSample[],
): Promise<void> {
  console.log(`\nExtracting facts from ${samples.length} samples...`);
  let total = 0;
  for (const sample of samples) {
    const n = await extractSample(db, llm, sample);
    console.log(`  Sample ${sample.id}: ${n} facts`);
    total += n;
  }
  console.log(`Extraction complete: ${total} total facts\n`);
}
