# mast-bench

LOCOMO benchmark for MAST: 10 long conversations, 1,986 questions in five categories (multi-hop, temporal, open-domain, single-hop, adversarial).

## Run

```bash
npm run bench -- --recall-only --split dev                   # retrieval only, no API key
OPENAI_API_KEY=... npm run bench -- --split dev --judge      # answers, F1 and LLM judge
```

Options: `--search-modes vector,fulltext,hybrid`, `--top-k 5,10`, `--extract` (LLM fact extraction), `--output <dir>`, `--record <name>`.

## Rules

- `splits/locomo10.json` fixes a dev split: 20% of each category, 401 questions, chosen with a seeded shuffle. The other 1,585 questions are held-out. `--split` defaults to `dev`.
- Tune on dev. Run held-out once for a change you want to report. Any tuning after looking at held-out results turns them into dev results.
- Retrieval recall and answer quality are different numbers. Recall says an evidence turn was in the top K; F1 and the judge score the answer. The summary prints them in separate tables. Do not compare one with the other or with other projects' QA accuracy.
- `--record <name>` copies `results.json`, `detailed.json` (every question) and `summary.txt` into `results/<name>/`. Commit that folder with the change it measures.
- Each turn is stored with its session date as `created_at`. The answer prompt shows that date on every context line. `INGEST_VERSION` in `src/ingest.ts` names the collections; bump it when the stored content changes, or an old database is reused.

## Results

Retrieval recall, local embedder (`--recall-only`), 2026-10-29.

| Hybrid scoring | Dev @5 | Dev @10 | Held-out @5 | Held-out @10 |
|---|---|---|---|---|
| Min-max of both lists (before) | 58.6% | 68.3% | 57.1% | 67.2% |
| Reciprocal rank fusion (not used) | 57.9% | 69.1% | — | — |
| BM25 divided by max, vector as is (now) | 61.6% | 71.6% | 58.2% | 68.8% |

Per-question files: `results/2026-10-29-retrieval-dev/` and `results/2026-10-29-retrieval-held-out/`. The "before" row comes from the same harness run on the earlier code and scored against this split.
