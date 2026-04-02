import type { MastClient } from "../mast";
import type { SearchParams, ToolResult } from "../types";

export async function mastSearch(
  mast: MastClient,
  params: SearchParams
): Promise<ToolResult> {
  const results = await mast.search(params.query, params.limit, params.mode);

  if (results.length === 0) {
    return { content: "No memories found matching the query." };
  }

  const lines = results.map(
    (r: { memory: { id: string; content: string; tier: string; metadata: Record<string, string> }; score: number }, i: number) => {
      const meta = Object.entries(r.memory.metadata)
        .map(([k, v]) => `${k}=${v}`)
        .join(", ");
      return `${i + 1}. [${r.memory.id}] (score: ${r.score.toFixed(3)}, tier: ${r.memory.tier}${meta ? `, ${meta}` : ""})\n   ${r.memory.content.slice(0, 200)}${r.memory.content.length > 200 ? "..." : ""}`;
    }
  );

  return {
    content: `Found ${results.length} memories:\n\n${lines.join("\n\n")}`,
  };
}
