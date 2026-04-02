import { parse } from "smol-toml";
import { readFileSync } from "fs";
import { resolve, dirname } from "path";
import { config as loadDotenv } from "dotenv";

export interface AgentConfig {
  agent: {
    name: string;
    provider: "anthropic" | "openai" | "openrouter";
    model: string;
    systemPrompt?: string;
    systemPromptFile?: string;
    apiKey?: string;
    embeddingModel?: string;
    permissionMode?: "default" | "bypassPermissions" | "acceptEdits";
    maxTurns?: number;
    effort?: "low" | "medium" | "high";
  };
  memory: {
    dbPath: string;
    durability: "immediate" | "eventual";
  };
  channels: {
    cli?: boolean;
    http?: { port: number; host?: string };
    telegram?: { token: string };
    slack?: { token: string; appToken: string; signingSecret: string };
  };
  tools?: {
    mcpServers?: Array<{ name: string; command: string; args?: string[] }>;
    stack0?: { apiKey: string; projectSlug?: string };
  };
}

/**
 * Recursively resolve `$ENV_VAR` references in string values.
 * Non-string values pass through unchanged.
 */
function resolveEnvVars(value: unknown): unknown {
  if (typeof value === "string") {
    return value.replace(/\$([A-Z_][A-Z0-9_]*)/g, (match, varName) => {
      const envVal = process.env[varName];
      if (envVal === undefined) {
        throw new Error(`Environment variable ${varName} is not set (referenced as $${varName})`);
      }
      return envVal;
    });
  }
  if (Array.isArray(value)) {
    return value.map(resolveEnvVars);
  }
  if (value !== null && typeof value === "object") {
    const result: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value)) {
      result[k] = resolveEnvVars(v);
    }
    return result;
  }
  return value;
}

export function loadConfig(configPath: string): AgentConfig {
  const absPath = resolve(configPath);
  const configDir = dirname(absPath);

  // Load .env from config file directory
  loadDotenv({ path: resolve(configDir, ".env") });

  let raw: string;
  try {
    raw = readFileSync(absPath, "utf-8");
  } catch (err) {
    throw new Error(`Failed to read config file: ${absPath} — ${(err as Error).message}`);
  }

  let parsed: Record<string, unknown>;
  try {
    parsed = parse(raw) as Record<string, unknown>;
  } catch (err) {
    throw new Error(`Failed to parse TOML config: ${(err as Error).message}`);
  }

  // Resolve env vars in all string values
  const resolved = resolveEnvVars(parsed) as Record<string, unknown>;

  // Validate required sections
  const agent = resolved.agent as Record<string, unknown> | undefined;
  if (!agent) throw new Error("Config missing required [agent] section");
  if (!agent.name) throw new Error("Config missing required agent.name");
  if (!agent.provider) throw new Error("Config missing required agent.provider");
  if (!agent.model) throw new Error("Config missing required agent.model");

  const validProviders = ["anthropic", "openai", "openrouter"];
  if (!validProviders.includes(agent.provider as string)) {
    throw new Error(`Invalid agent.provider: ${agent.provider} (expected: ${validProviders.join(", ")})`);
  }

  // Load system prompt from file if specified
  if (agent.systemPromptFile && !agent.systemPrompt) {
    const promptPath = resolve(configDir, agent.systemPromptFile as string);
    try {
      agent.systemPrompt = readFileSync(promptPath, "utf-8");
    } catch (err) {
      throw new Error(`Failed to read system prompt file: ${promptPath} — ${(err as Error).message}`);
    }
  }

  // Apply memory defaults
  const memory = (resolved.memory ?? {}) as Record<string, unknown>;
  if (!memory.dbPath) {
    memory.dbPath = resolve(configDir, "data", "memory.db");
  } else {
    memory.dbPath = resolve(configDir, memory.dbPath as string);
  }
  if (!memory.durability) {
    memory.durability = "eventual";
  }
  if (memory.durability !== "immediate" && memory.durability !== "eventual") {
    throw new Error(`Invalid memory.durability: ${memory.durability} (expected: immediate, eventual)`);
  }

  return {
    agent: agent as AgentConfig["agent"],
    memory: memory as AgentConfig["memory"],
    channels: (resolved.channels ?? {}) as AgentConfig["channels"],
    tools: resolved.tools as AgentConfig["tools"],
  };
}
