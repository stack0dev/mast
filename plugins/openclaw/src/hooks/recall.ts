import type { MastClient } from "../mast";
import type { PluginConfig, HookContext, HookResult } from "../types";

export async function beforeAgentStart(
  mast: MastClient,
  config: PluginConfig,
  context: HookContext
): Promise<HookResult> {
  if (!config.autoRecall) return {};

  const lastUserMsg = findLastUserMessage(context.messages);
  if (!lastUserMsg) return {};

  const results = await mast.search(
    lastUserMsg,
    config.maxResults,
    config.searchMode
  );

  if (results.length === 0) return {};

  const memories = results.map(
    (r: { memory: { id: string; content: string; tier: string }; score: number }) =>
      `<memory id="${r.memory.id}" score="${r.score.toFixed(3)}" tier="${r.memory.tier}">\n${r.memory.content}\n</memory>`
  );

  const block = `<mast-context>\n${memories.join("\n")}\n</mast-context>`;

  return { prependContext: block };
}

function findLastUserMessage(
  messages: Array<{ role: string; content: string }>
): string | undefined {
  for (let i = messages.length - 1; i >= 0; i--) {
    if (messages[i].role === "user" && messages[i].content) {
      return messages[i].content;
    }
  }
  return undefined;
}
