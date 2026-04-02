import type { MastClient } from "../mast";
import type { StoreParams, ToolResult } from "../types";

export async function mastStore(
  mast: MastClient,
  params: StoreParams
): Promise<ToolResult> {
  const memory = await mast.store(
    params.content,
    params.metadata,
    params.tier,
    params.ttl
  );
  return {
    content: `Stored memory ${memory.id}:\n- collection: ${memory.collection}\n- tier: ${memory.tier}\n- content: ${memory.content.slice(0, 120)}${memory.content.length > 120 ? "..." : ""}`,
  };
}
