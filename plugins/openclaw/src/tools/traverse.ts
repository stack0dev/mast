import type { MastClient } from "../mast";
import type { TraverseParams, ToolResult } from "../types";

export function mastTraverse(
  mast: MastClient,
  params: TraverseParams
): ToolResult {
  const edges = mast.traverse(
    params.start,
    params.maxDepth,
    params.relation,
    params.minWeight
  );

  if (edges.length === 0) {
    return { content: `No edges found from "${params.start}".` };
  }

  const lines = edges.map(
    (e: { id: string; source: string; target: string; relation: string; weight: number }) =>
      `- ${e.source} -[${e.relation}, w=${e.weight.toFixed(2)}]-> ${e.target} (${e.id})`
  );

  return {
    content: `Traversal from "${params.start}" (depth ${params.maxDepth ?? 3}):\n${lines.join("\n")}`,
  };
}
