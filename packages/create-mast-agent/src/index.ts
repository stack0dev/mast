#!/usr/bin/env node

import { mkdirSync, writeFileSync, existsSync } from "fs";
import { resolve, join } from "path";
import chalk from "chalk";

const AGENT_TOML = `[agent]
name = "{name}"
provider = "anthropic"
model = "claude-sonnet-4-5-20250929"
system_prompt_file = "system-prompt.md"
api_key = "$ANTHROPIC_API_KEY"

[memory]
db_path = "./data/memory.db"
durability = "eventual"

[channels]
cli = true
# http = {{ port = 3000 }}

# [tools.stack0]
# api_key = "$STACK0_API_KEY"
# project_slug = "my-project"
`;

const SYSTEM_PROMPT = `You are {name}, a helpful personal AI assistant with long-term memory.

You can remember things across conversations using your memory tools. When the user tells you something important — preferences, facts about themselves, ongoing projects — store it in memory so you can recall it later.

Be concise, thoughtful, and proactive about using your memory.
`;

const ENV_EXAMPLE = `# LLM API Keys (uncomment the one you need)
ANTHROPIC_API_KEY=
# OPENAI_API_KEY=
# OPENROUTER_API_KEY=

# Stack0 (email, screenshots, CDN, workflows)
# STACK0_API_KEY=
`;

const GITIGNORE = `.env
data/
node_modules/
dist/
`;

const PACKAGE_JSON = `{
  "name": "{name}",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "start": "mast-agent mast-agent.toml"
  },
  "dependencies": {
    "mast-agent": "^0.1.0",
    "mast-channels": "^0.1.0",
    "mast-tools": "^0.1.0",
    "mast-memory": "^0.1.0"
  }
}
`;

function main() {
  const name = process.argv[2];

  if (!name) {
    console.log(chalk.red("Usage: npx create-mast-agent <name>"));
    console.log(chalk.dim("  Example: npx create-mast-agent my-agent"));
    process.exit(1);
  }

  if (!/^[a-z0-9][a-z0-9-]*$/.test(name)) {
    console.log(chalk.red("Name must be lowercase alphanumeric with hyphens (e.g., my-agent)"));
    process.exit(1);
  }

  const dir = resolve(name);

  if (existsSync(dir)) {
    console.log(chalk.red(`Directory "${name}" already exists`));
    process.exit(1);
  }

  console.log(chalk.bold(`Creating MAST agent "${name}"...\n`));

  // Create directory structure
  mkdirSync(dir, { recursive: true });
  mkdirSync(join(dir, "data"), { recursive: true });

  // Write files
  const files: [string, string][] = [
    ["mast-agent.toml", AGENT_TOML.replaceAll("{name}", name)],
    ["system-prompt.md", SYSTEM_PROMPT.replaceAll("{name}", name)],
    [".env.example", ENV_EXAMPLE],
    [".env", ENV_EXAMPLE],
    [".gitignore", GITIGNORE],
    ["package.json", PACKAGE_JSON.replaceAll("{name}", name)],
  ];

  for (const [filename, content] of files) {
    writeFileSync(join(dir, filename), content);
    console.log(`  ${chalk.green("+")} ${filename}`);
  }

  console.log(`\n${chalk.bold("Next steps:")}`);
  console.log(`  cd ${name}`);
  console.log(`  ${chalk.dim("# Add your API key to .env")}`);
  console.log(`  npm install`);
  console.log(`  npm start`);
}

main();
