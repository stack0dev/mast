import * as readline from "readline";
import chalk from "chalk";
import type { AgentHandler, Channel } from "./types.js";

export class CliChannel implements Channel {
  readonly name = "cli";
  private rl: readline.Interface | null = null;
  private agent: AgentHandler | null = null;

  async start(agent: AgentHandler): Promise<void> {
    this.agent = agent;

    this.rl = readline.createInterface({
      input: process.stdin,
      output: process.stdout,
    });

    this.rl.on("close", () => {
      this.stop();
    });

    console.log(chalk.dim("Type /quit to exit, /clear to clear screen.\n"));

    this.prompt();
  }

  async stop(): Promise<void> {
    if (this.rl) {
      this.rl.close();
      this.rl = null;
    }
    this.agent = null;
  }

  private prompt(): void {
    if (!this.rl || !this.agent) return;

    this.rl.question(chalk.green("> "), async (input) => {
      const trimmed = input.trim();
      if (!trimmed) {
        this.prompt();
        return;
      }

      if (trimmed === "/quit") {
        await this.stop();
        return;
      }

      if (trimmed === "/clear") {
        console.clear();
        this.prompt();
        return;
      }

      const thinking = this.showThinking();
      try {
        const response = await this.agent!.handleMessage(
          "cli-user",
          "cli",
          trimmed,
        );
        thinking.stop();
        console.log(chalk.cyan("assistant:"), response, "\n");
      } catch (err) {
        thinking.stop();
        console.error(chalk.red("error:"), err, "\n");
      }

      this.prompt();
    });
  }

  private showThinking(): { stop: () => void } {
    const frames = ["|", "/", "-", "\\"];
    let i = 0;
    const interval = setInterval(() => {
      process.stdout.write(`\r${chalk.dim("thinking")} ${frames[i++ % frames.length]}`);
    }, 100);

    return {
      stop() {
        clearInterval(interval);
        process.stdout.write("\r" + " ".repeat(20) + "\r");
      },
    };
  }
}
