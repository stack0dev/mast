import type { ChatMessage } from "./providers/types.js";
import type { RecalledMemory } from "./memory.js";

export interface ContextBudget {
  maxTokens: number;
  systemPromptTokens: number;
  memoryTokens: number;
  recentMessageTokens: number;
  toolResultTokens: number;
}

const DEFAULT_BUDGET: ContextBudget = {
  maxTokens: 100_000,
  systemPromptTokens: 4_000,
  memoryTokens: 8_000,
  recentMessageTokens: 80_000,
  toolResultTokens: 8_000,
};

export interface AssembledContext {
  systemPrompt: string;
  memories: string[];
  messages: ChatMessage[];
}

/** Rough token estimate: ~4 chars per token. */
function estimateTokens(text: string): number {
  return Math.ceil(text.length / 4);
}

function formatMemory(m: RecalledMemory): string {
  const meta = Object.entries(m.metadata)
    .map(([k, v]) => `${k}=${v}`)
    .join(", ");
  const metaStr = meta ? ` [${meta}]` : "";
  return `[${m.tier}|${m.score.toFixed(2)}]${metaStr} ${m.content}`;
}

export function assembleContext(opts: {
  systemPrompt: string;
  memories: RecalledMemory[];
  recentMessages: ChatMessage[];
  budget?: Partial<ContextBudget>;
}): AssembledContext {
  const budget = { ...DEFAULT_BUDGET, ...opts.budget };

  // Fit memories within budget
  const formattedMemories: string[] = [];
  let memoryTokensUsed = 0;
  for (const m of opts.memories) {
    const formatted = formatMemory(m);
    const tokens = estimateTokens(formatted);
    if (memoryTokensUsed + tokens > budget.memoryTokens) break;
    formattedMemories.push(formatted);
    memoryTokensUsed += tokens;
  }

  // Build system prompt with memories section
  let systemPrompt = opts.systemPrompt;
  if (formattedMemories.length > 0) {
    systemPrompt += "\n\n<recalled-memories>\n" + formattedMemories.join("\n") + "\n</recalled-memories>";
  }

  // Trim system prompt to budget
  const sysTokens = estimateTokens(systemPrompt);
  if (sysTokens > budget.systemPromptTokens + budget.memoryTokens) {
    const charLimit = (budget.systemPromptTokens + budget.memoryTokens) * 4;
    systemPrompt = systemPrompt.slice(0, charLimit);
  }

  // Sliding window: take most recent messages that fit budget
  const messages: ChatMessage[] = [];
  let messageTokensUsed = 0;
  for (let i = opts.recentMessages.length - 1; i >= 0; i--) {
    const msg = opts.recentMessages[i];
    const tokens = estimateTokens(msg.content);
    if (messageTokensUsed + tokens > budget.recentMessageTokens) break;
    messages.unshift(msg);
    messageTokensUsed += tokens;
  }

  return { systemPrompt, memories: formattedMemories, messages };
}
