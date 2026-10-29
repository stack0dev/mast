import type { MastDb } from "mast-memory";
import type { LocomoSample } from "./types.js";

/** Bump when stored content or fields change, so old databases are not reused. */
export const INGEST_VERSION = 2;

export function collectionName(sampleId: number): string {
  return `locomo_${sampleId}_v${INGEST_VERSION}`;
}

export function ingestSample(db: MastDb, sample: LocomoSample): number {
  const collection = collectionName(sample.id);

  // Check if already ingested
  const collections = db.info();
  const existing = collections.find((c) => c.name === collection);
  if (existing && existing.memoryCount > 0) {
    console.log(
      `  Sample ${sample.id}: already ingested (${existing.memoryCount} memories), skipping`,
    );
    return existing.memoryCount;
  }

  let count = 0;
  for (let si = 0; si < sample.conversation.sessions.length; si++) {
    const session = sample.conversation.sessions[si];
    for (const turn of session.turns) {
      let content = `${turn.speaker}: ${turn.text}`;
      if (turn.blip_caption) {
        content += ` [image: ${turn.blip_caption}]`;
      }

      const metadata: Record<string, string> = {
        speaker: turn.speaker,
        session: String(si + 1),
        dia_id: turn.dia_id,
      };

      db.storeLocal(collection, content, metadata, null, null, session.timestamp);
      count++;
    }
  }

  return count;
}

export function ingestAll(db: MastDb, samples: LocomoSample[]): void {
  console.log(`\nIngesting ${samples.length} samples...`);
  let total = 0;
  for (const sample of samples) {
    const n = ingestSample(db, sample);
    total += n;
  }
  console.log(`Ingestion complete: ${total} total memories\n`);
}
