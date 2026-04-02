import type { MastClient } from "../mast";
import type { RelateParams, ToolResult } from "../types";

export function mastRelate(
  mast: MastClient,
  params: RelateParams
): ToolResult {
  const edgeId = mast.relate(
    params.source,
    params.target,
    params.relation,
    params.weight
  );
  return {
    content: `Created edge ${edgeId}: ${params.source} -[${params.relation}]-> ${params.target}${params.weight != null ? ` (weight: ${params.weight})` : ""}`,
  };
}
