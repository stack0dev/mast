import { App } from "@slack/bolt";
import type { AgentHandler, Channel } from "./types.js";

export interface SlackChannelConfig {
  token: string;
  appToken: string;
  signingSecret: string;
}

export class SlackChannel implements Channel {
  readonly name = "slack";
  private app: App;
  private agent: AgentHandler | null = null;

  constructor(config: SlackChannelConfig) {
    this.app = new App({
      token: config.token,
      appToken: config.appToken,
      signingSecret: config.signingSecret,
      socketMode: true,
    });
  }

  async start(agent: AgentHandler): Promise<void> {
    this.agent = agent;

    // Handle @mentions in channels
    this.app.event("app_mention", async ({ event, say }) => {
      const userId = `slack:${event.user}`;
      const text = event.text.replace(/<@[A-Z0-9]+>/g, "").trim();

      try {
        const response = await this.agent!.handleMessage(userId, "slack", text);
        await say({ text: response, thread_ts: event.thread_ts ?? event.ts });
      } catch (err) {
        console.error("[slack] handler error:", err);
        await say({ text: "Sorry, something went wrong.", thread_ts: event.thread_ts ?? event.ts });
      }
    });

    // Handle DMs
    this.app.message(async ({ message, say }) => {
      if (message.subtype) return; // ignore edits, joins, etc.
      const msg = message as { user: string; text?: string; ts: string; thread_ts?: string };
      if (!msg.text) return;

      const userId = `slack:${msg.user}`;

      try {
        const response = await this.agent!.handleMessage(userId, "slack", msg.text);
        await say({ text: response, thread_ts: msg.thread_ts ?? msg.ts });
      } catch (err) {
        console.error("[slack] handler error:", err);
        await say({ text: "Sorry, something went wrong.", thread_ts: msg.thread_ts ?? msg.ts });
      }
    });

    await this.app.start();
    console.log("[slack] bot started (socket mode)");
  }

  async stop(): Promise<void> {
    await this.app.stop();
    this.agent = null;
  }
}
