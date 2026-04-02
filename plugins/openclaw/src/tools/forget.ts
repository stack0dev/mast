import type { MastClient } from "../mast";
import type { ForgetParams, ToolResult } from "../types";

export function mastForget(
  mast: MastClient,
  params: ForgetParams
): ToolResult {
  mast.delete(params.id);
  return { content: `Deleted memory ${params.id}.` };
}
