import { Client, Events, GatewayIntentBits, type Message } from "discord.js";
import type { AgentHandler, Channel } from "./types.js";

export interface DiscordChannelConfig {
  token: string;
  /** Reply only when @mentioned in guild channels. DMs always reply. Default: true */
  requireMention?: boolean;
}

export class DiscordChannel implements Channel {
  readonly name = "discord";
  private client: Client;
  private agent: AgentHandler | null = null;
  private config: DiscordChannelConfig;

  constructor(config: DiscordChannelConfig) {
    this.config = config;
    this.client = new Client({
      intents: [
        GatewayIntentBits.Guilds,
        GatewayIntentBits.GuildMessages,
        GatewayIntentBits.DirectMessages,
        GatewayIntentBits.MessageContent,
      ],
    });
  }

  async start(agent: AgentHandler): Promise<void> {
    this.agent = agent;

    this.client.on(Events.MessageCreate, async (message: Message) => {
      if (message.author.bot) return;

      const isDM = !message.guild;
      const mentioned =
        message.mentions.has(this.client.user!) ||
        message.content.startsWith(`<@${this.client.user!.id}>`);
      const requireMention = this.config.requireMention ?? true;

      if (!isDM && requireMention && !mentioned) return;

      const text = message.content
        .replace(/<@!?\d+>/g, "")
        .trim();
      if (!text) return;

      const userId = `discord:${message.author.id}`;

      if ("sendTyping" in message.channel) {
        await message.channel.sendTyping();
      }

      try {
        const response = await this.agent!.handleMessage(userId, "discord", text);

        // Discord has a 2000 char limit per message
        for (let i = 0; i < response.length; i += 2000) {
          await message.reply(response.slice(i, i + 2000));
        }
      } catch (err) {
        console.error("[discord] handler error:", err);
        await message.reply("Sorry, something went wrong. Please try again.");
      }
    });

    await this.client.login(this.config.token);
    console.log(`[discord] bot started as ${this.client.user!.tag}`);
  }

  async stop(): Promise<void> {
    await this.client.destroy();
    this.agent = null;
  }
}
