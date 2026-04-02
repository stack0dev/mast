import type { MastClient } from "../mast";
import type { ToolResult } from "../types";

export function mastGraph(mast: MastClient): ToolResult {
  const edges = mast.subgraph();

  if (edges.length === 0) {
    return { content: "No edges in the knowledge graph." };
  }

  const lines = edges.map(
    (e: { id: string; source: string; target: string; relation: string; weight: number }) =>
      `- ${e.source} -[${e.relation}, w=${e.weight.toFixed(2)}]-> ${e.target} (${e.id})`
  );

  return {
    content: `Knowledge graph (${edges.length} edges):\n${lines.join("\n")}`,
  };
}
