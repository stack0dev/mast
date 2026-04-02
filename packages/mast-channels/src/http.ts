import express from "express";
import { createServer, type Server } from "http";
import { WebSocketServer, type WebSocket } from "ws";
import type { AgentHandler, Channel } from "./types.js";

export interface HttpChannelConfig {
  port: number;
  host?: string;
}

/**
 * Wire protocol for WebSocket messages.
 *
 * Client → Server:
 *   { type: "message", content: string, userId?: string }
 *   { type: "ping" }
 *
 * Server → Client:
 *   { type: "response", content: string, messageId: string }
 *   { type: "error", error: string }
 *   { type: "status", status: "thinking" | "idle" }
 *   { type: "pong" }
 *   { type: "welcome", agentName: string, version: string }
 */

export interface WsClientMessage {
  type: "message" | "ping";
  content?: string;
  userId?: string;
}

export interface WsServerMessage {
  type: "response" | "error" | "status" | "pong" | "welcome";
  content?: string;
  messageId?: string;
  error?: string;
  status?: "thinking" | "idle";
  agentName?: string;
  version?: string;
}

class RateLimiter {
  private requests = new Map<string, number[]>();

  check(key: string, maxRequests: number, windowMs: number): boolean {
    const now = Date.now();
    const timestamps =
      this.requests.get(key)?.filter((t) => t > now - windowMs) ?? [];
    if (timestamps.length >= maxRequests) return false;
    timestamps.push(now);
    this.requests.set(key, timestamps);
    return true;
  }
}

export class HttpChannel implements Channel {
  readonly name = "http";
  private server: Server | null = null;
  private wss: WebSocketServer | null = null;
  private config: Required<HttpChannelConfig>;
  private limiter = new RateLimiter();
  private startTime = 0;
  private agentName = "mast-agent";
  private messageCounter = 0;

  constructor(config: HttpChannelConfig, agentName?: string) {
    this.config = { host: "127.0.0.1", ...config };
    if (agentName) this.agentName = agentName;
  }

  private nextMessageId(): string {
    return `msg_${++this.messageCounter}_${Date.now()}`;
  }

  private send(ws: WebSocket, msg: WsServerMessage): void {
    if (ws.readyState === ws.OPEN) {
      ws.send(JSON.stringify(msg));
    }
  }

  async start(agent: AgentHandler): Promise<void> {
    this.startTime = Date.now();

    const app = express();
    app.use(express.json());

    app.get("/health", (_req, res) => {
      res.json({
        status: "ok",
        agent: this.agentName,
        uptime: Date.now() - this.startTime,
      });
    });

    app.post("/chat", async (req, res) => {
      const ip = req.ip ?? "unknown";
      if (!this.limiter.check(ip, 30, 60_000)) {
        res.status(429).json({ error: "Rate limit exceeded" });
        return;
      }

      const { message, userId } = req.body as {
        message?: string;
        userId?: string;
      };

      if (typeof message !== "string" || !message.trim()) {
        res.status(400).json({ error: "message is required" });
        return;
      }

      try {
        const response = await agent.handleMessage(
          userId ?? `http:${ip}`,
          "http",
          message,
        );
        res.json({ response });
      } catch (err) {
        console.error("[http] handler error:", err);
        res.status(500).json({ error: "Internal server error" });
      }
    });

    this.server = createServer(app);

    this.wss = new WebSocketServer({ server: this.server });
    this.wss.on("connection", (ws, req) => {
      const ip = req.socket.remoteAddress ?? "unknown";

      // Send welcome message
      this.send(ws, {
        type: "welcome",
        agentName: this.agentName,
        version: "0.1.0",
      });

      ws.on("message", async (data) => {
        let parsed: WsClientMessage;
        try {
          parsed = JSON.parse(String(data));
        } catch {
          this.send(ws, { type: "error", error: "Invalid JSON" });
          return;
        }

        if (parsed.type === "ping") {
          this.send(ws, { type: "pong" });
          return;
        }

        if (parsed.type !== "message") {
          this.send(ws, { type: "error", error: `Unknown message type: ${parsed.type}` });
          return;
        }

        if (typeof parsed.content !== "string" || !parsed.content.trim()) {
          this.send(ws, { type: "error", error: "content is required" });
          return;
        }

        if (!this.limiter.check(ip, 30, 60_000)) {
          this.send(ws, { type: "error", error: "Rate limit exceeded" });
          return;
        }

        const userId = parsed.userId ?? `ws:${ip}`;
        const messageId = this.nextMessageId();

        // Signal thinking
        this.send(ws, { type: "status", status: "thinking" });

        try {
          const response = await agent.handleMessage(
            userId,
            "ws",
            parsed.content,
          );
          this.send(ws, { type: "response", content: response, messageId });
        } catch (err) {
          console.error("[ws] handler error:", err);
          this.send(ws, { type: "error", error: "Internal server error" });
        }

        this.send(ws, { type: "status", status: "idle" });
      });
    });

    await new Promise<void>((resolve) => {
      this.server!.listen(this.config.port, this.config.host, () => {
        console.log(
          `[http] listening on ${this.config.host}:${this.config.port}`,
        );
        resolve();
      });
    });
  }

  async stop(): Promise<void> {
    if (this.wss) {
      for (const client of this.wss.clients) {
        client.close();
      }
      this.wss.close();
      this.wss = null;
    }

    if (this.server) {
      await new Promise<void>((resolve, reject) => {
        this.server!.close((err) => (err ? reject(err) : resolve()));
      });
      this.server = null;
    }
  }
}
