import express, { type Request, type Response } from "express";
import { createServer, type Server } from "http";
import type { AgentHandler, Channel } from "./types.js";

export interface WhatsAppChannelConfig {
  /** WhatsApp Cloud API access token */
  accessToken: string;
  /** Phone number ID from Meta dashboard */
  phoneNumberId: string;
  /** Webhook verify token (you choose this, must match Meta dashboard) */
  verifyToken: string;
  /** Port for the webhook server */
  port: number;
  host?: string;
  /** Graph API version. Default: "v21.0" */
  apiVersion?: string;
}

export class WhatsAppChannel implements Channel {
  readonly name = "whatsapp";
  private server: Server | null = null;
  private agent: AgentHandler | null = null;
  private config: WhatsAppChannelConfig;
  private apiVersion: string;

  constructor(config: WhatsAppChannelConfig) {
    this.config = config;
    this.apiVersion = config.apiVersion ?? "v21.0";
  }

  async start(agent: AgentHandler): Promise<void> {
    this.agent = agent;

    const app = express();
    app.use(express.json());

    // Webhook verification — Meta sends a GET to confirm the endpoint
    app.get("/webhook", (req: Request, res: Response) => {
      const mode = req.query["hub.mode"];
      const token = req.query["hub.verify_token"];
      const challenge = req.query["hub.challenge"];

      if (mode === "subscribe" && token === this.config.verifyToken) {
        console.log("[whatsapp] webhook verified");
        res.status(200).send(challenge);
      } else {
        res.sendStatus(403);
      }
    });

    // Incoming messages
    app.post("/webhook", async (req: Request, res: Response) => {
      // Always respond 200 quickly — Meta retries on timeout
      res.sendStatus(200);

      const body = req.body as WebhookBody;
      if (body.object !== "whatsapp_business_account") return;

      for (const entry of body.entry ?? []) {
        for (const change of entry.changes ?? []) {
          if (change.field !== "messages") continue;

          for (const message of change.value?.messages ?? []) {
            if (message.type !== "text") continue;

            const from = message.from;
            const text = message.text?.body;
            if (!from || !text) continue;

            await this.handleIncoming(from, text);
          }
        }
      }
    });

    this.server = createServer(app);
    const host = this.config.host ?? "0.0.0.0";

    await new Promise<void>((resolve) => {
      this.server!.listen(this.config.port, host, () => {
        console.log(`[whatsapp] webhook listening on ${host}:${this.config.port}`);
        resolve();
      });
    });
  }

  private async handleIncoming(from: string, text: string): Promise<void> {
    const userId = `whatsapp:${from}`;

    await this.markRead(from);

    try {
      const response = await this.agent!.handleMessage(userId, "whatsapp", text);
      await this.sendMessage(from, response);
    } catch (err) {
      console.error("[whatsapp] handler error:", err);
      await this.sendMessage(from, "Sorry, something went wrong. Please try again.");
    }
  }

  private async sendMessage(to: string, text: string): Promise<void> {
    const url = `https://graph.facebook.com/${this.apiVersion}/${this.config.phoneNumberId}/messages`;

    const res = await fetch(url, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${this.config.accessToken}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        messaging_product: "whatsapp",
        to,
        type: "text",
        text: { body: text },
      }),
    });

    if (!res.ok) {
      const body = await res.text();
      console.error(`[whatsapp] send failed (${res.status}): ${body}`);
    }
  }

  private async markRead(from: string): Promise<void> {
    const url = `https://graph.facebook.com/${this.apiVersion}/${this.config.phoneNumberId}/messages`;

    await fetch(url, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${this.config.accessToken}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        messaging_product: "whatsapp",
        status: "read",
        message_id: from,
      }),
    }).catch(() => {}); // best-effort
  }

  async stop(): Promise<void> {
    if (this.server) {
      await new Promise<void>((resolve, reject) => {
        this.server!.close((err) => (err ? reject(err) : resolve()));
      });
      this.server = null;
    }
    this.agent = null;
  }
}

// WhatsApp Cloud API webhook payload types (minimal)

interface WebhookBody {
  object: string;
  entry?: WebhookEntry[];
}

interface WebhookEntry {
  id: string;
  changes?: WebhookChange[];
}

interface WebhookChange {
  field: string;
  value?: {
    messages?: WebhookMessage[];
  };
}

interface WebhookMessage {
  from: string;
  type: string;
  text?: { body: string };
}
