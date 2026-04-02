import fs from "node:fs";
import path from "node:path";
import https from "node:https";
import type { LocomoSample, LocomoSession, LocomoTurn, LocomoQA } from "./types.js";

const LOCOMO_URL =
  "https://raw.githubusercontent.com/snap-research/locomo/main/data/locomo10.json";

function httpsGet(url: string): Promise<string> {
  return new Promise((resolve, reject) => {
    https.get(url, (res) => {
      if (res.statusCode === 301 || res.statusCode === 302) {
        const loc = res.headers.location;
        if (loc) return httpsGet(loc).then(resolve, reject);
        return reject(new Error(`Redirect without location`));
      }
      if (res.statusCode !== 200) {
        return reject(new Error(`HTTP ${res.statusCode}`));
      }
      const chunks: Buffer[] = [];
      res.on("data", (chunk: Buffer) => chunks.push(chunk));
      res.on("end", () => resolve(Buffer.concat(chunks).toString("utf-8")));
      res.on("error", reject);
    }).on("error", reject);
  });
}

export async function loadData(dataPath: string): Promise<LocomoSample[]> {
  let raw: string;

  if (fs.existsSync(dataPath)) {
    console.log(`Loading data from ${dataPath}`);
    raw = fs.readFileSync(dataPath, "utf-8");
  } else {
    const cacheDir = path.dirname(dataPath);
    fs.mkdirSync(cacheDir, { recursive: true });
    console.log(`Downloading LOCOMO dataset...`);
    raw = await httpsGet(LOCOMO_URL);
    fs.writeFileSync(dataPath, raw);
    console.log(`Cached to ${dataPath}`);
  }

  const data: Record<string, unknown>[] = JSON.parse(raw);
  return data.map((item, idx) => parseSample(item, idx));
}

function parseSample(raw: Record<string, unknown>, index: number): LocomoSample {
  const conv = raw.conversation as Record<string, unknown> | undefined;
  if (!conv) {
    throw new Error(`Sample ${index}: no "conversation" key`);
  }

  const speakerA = String(conv.speaker_a ?? "");
  const speakerB = String(conv.speaker_b ?? "");

  // Session keys: session_1, session_2, ... inside conversation object
  const sessionKeys = Object.keys(conv)
    .filter((k) => /^session_\d+$/.test(k))
    .sort((a, b) => {
      const na = parseInt(a.split("_")[1]);
      const nb = parseInt(b.split("_")[1]);
      return na - nb;
    });

  if (sessionKeys.length === 0) {
    throw new Error(`Sample ${index}: no session keys found`);
  }

  const sessions: LocomoSession[] = [];
  for (const key of sessionKeys) {
    const sessionData = conv[key] as Record<string, unknown>[];
    const turns: LocomoTurn[] = [];

    for (const turn of sessionData) {
      const speaker = String(turn.speaker ?? "");
      const text = String(turn.text ?? "");
      const diaId = String(turn.dia_id ?? "");

      const t: LocomoTurn = { speaker, dia_id: diaId, text };
      if (turn.blip_caption) t.blip_caption = String(turn.blip_caption);
      turns.push(t);
    }

    sessions.push({ turns });
  }

  // Parse QA
  const qaRaw = (raw.qa ?? []) as Record<string, unknown>[];
  const qa: LocomoQA[] = qaRaw.map((q) => ({
    question: String(q.question ?? ""),
    answer: String(q.answer ?? ""),
    evidence: Array.isArray(q.evidence)
      ? q.evidence.map(String)
      : typeof q.evidence === "string"
        ? [q.evidence]
        : [],
    category: Number(q.category ?? 0),
    ...(q.adversarial_answer != null
      ? { adversarial_answer: String(q.adversarial_answer) }
      : {}),
  }));

  return {
    id: index,
    conversation: { speaker_a: speakerA, speaker_b: speakerB, sessions },
    qa,
  };
}
