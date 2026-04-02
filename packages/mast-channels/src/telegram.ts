import { Bot } from "grammy";
import type { AgentHandler, Channel } from "./types.js";

export interface TelegramChannelConfig {
  token: string;
}

export class TelegramChannel implements Channel {
  readonly name = "telegram";
  private bot: Bot;
  private agent: AgentHandler | null = null;

  constructor(config: TelegramChannelConfig) {
    this.bot = new Bot(config.token);
  }

  async start(agent: AgentHandler): Promise<void> {
    this.agent = agent;

    this.bot.command("start", (ctx) =>
      ctx.reply("Hello! I'm your AI assistant. Send me a message to get started."),
    );

    this.bot.command("help", (ctx) =>
      ctx.reply("Just send me a message and I'll respond. Use /forget to clear our conversation history."),
    );

    this.bot.command("forget", async (ctx) => {
      const userId = `telegram:${ctx.from?.id}`;
      await this.agent!.handleMessage(userId, "telegram", "Please forget our conversation history.");
      await ctx.reply("Memory cleared.");
    });

    this.bot.on("message:text", async (ctx) => {
      const userId = `telegram:${ctx.from.id}`;

      await ctx.replyWithChatAction("typing");

      try {
        const response = await this.agent!.handleMessage(
          userId,
          "telegram",
          ctx.message.text,
        );
        await ctx.reply(response);
      } catch (err) {
        console.error("[telegram] handler error:", err);
        await ctx.reply("Sorry, something went wrong. Please try again.");
      }
    });

    this.bot.catch((err) => {
      console.error("[telegram] bot error:", err);
    });

    this.bot.start();
    console.log("[telegram] bot started (long-polling)");
  }

  async stop(): Promise<void> {
    this.bot.stop();
    this.agent = null;
  }
}
