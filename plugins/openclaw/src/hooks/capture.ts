import type { MastClient } from "../mast";
import type { PluginConfig, HookContext } from "../types";

export async function agentEnd(
  mast: MastClient,
  config: PluginConfig,
  context: HookContext
): Promise<void> {
  if (!config.autoCapture) return;

  const lastUserMsg = findLastUserMessage(context.messages);
  if (!lastUserMsg) return;

  // Skip very short messages that are unlikely to be worth remembering
  if (lastUserMsg.length < 20) return;

  await mast.store(lastUserMsg, { source: "openclaw_auto" }, "background");
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
