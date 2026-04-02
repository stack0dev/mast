import { tool, createSdkMcpServer } from "@anthropic-ai/claude-agent-sdk";
import { z } from "zod/v4";
import type { MemoryManager } from "./memory.js";

/**
 * In-process MCP server exposing memory tools (remember, search_memory, forget)
 * for the Claude Agent SDK path.
 */
export function createMemoryMcpServer(
  memory: MemoryManager,
  getCollection: () => string,
) {
  return createSdkMcpServer({
    name: "memory",
    version: "0.1.0",
    tools: [
      tool(
        "remember",
        "Store something in long-term memory. Use this to save important facts, preferences, or context for later retrieval.",
        {
          content: z.string().describe("The content to remember"),
          metadata: z
            .record(z.string(), z.string())
            .optional()
            .describe("Optional key-value metadata to attach"),
        },
        async (args) => {
          try {
            const collection = getCollection();
            const { id } = await memory.store(
              collection,
              args.content,
              args.metadata,
            );
            return {
              content: [{ type: "text" as const, text: `Stored memory ${id}` }],
            };
          } catch (err) {
            const msg = err instanceof Error ? err.message : String(err);
            return {
              content: [
                { type: "text" as const, text: `Failed to store memory: ${msg}` },
              ],
              isError: true,
            };
          }
        },
      ),
      tool(
        "search_memory",
        "Search long-term memory for relevant information. Returns memories ranked by relevance.",
        {
          query: z.string().describe("The search query"),
          limit: z
            .number()
            .optional()
            .describe("Maximum number of results (default: 5)"),
        },
        async (args) => {
          try {
            const collection = getCollection();
            const results = await memory.recall(
              collection,
              args.query,
              args.limit ?? 5,
            );
            if (results.length === 0) {
              return {
                content: [
                  { type: "text" as const, text: "No matching memories found." },
                ],
              };
            }
            const formatted = results
              .map(
                (r, i) =>
                  `[${i + 1}] (score: ${r.score.toFixed(3)}) ${r.content}`,
              )
              .join("\n\n");
            return {
              content: [{ type: "text" as const, text: formatted }],
            };
          } catch (err) {
            const msg = err instanceof Error ? err.message : String(err);
            return {
              content: [
                {
                  type: "text" as const,
                  text: `Failed to search memory: ${msg}`,
                },
              ],
              isError: true,
            };
          }
        },
      ),
      tool(
        "forget",
        "Remove a specific memory by its ID.",
        {
          memory_id: z.string().describe("The ID of the memory to remove"),
        },
        async (args) => {
          try {
            const collection = getCollection();
            await memory.forget(collection, args.memory_id);
            return {
              content: [
                {
                  type: "text" as const,
                  text: `Removed memory ${args.memory_id}`,
                },
              ],
            };
          } catch (err) {
            const msg = err instanceof Error ? err.message : String(err);
            return {
              content: [
                {
                  type: "text" as const,
                  text: `Failed to remove memory: ${msg}`,
                },
              ],
              isError: true,
            };
          }
        },
      ),
    ],
  });
}

export interface Stack0McpConfig {
  apiKey: string;
  projectSlug?: string;
}

/**
 * In-process MCP server wrapping Stack0 tools for the Claude Agent SDK path.
 * Lazy-loads @stack0/sdk just like stack0.ts does.
 */
export function createStack0McpServer(config: Stack0McpConfig) {
  const { apiKey, projectSlug } = config;
  let clientPromise: Promise<any> | null = null;

  const STACK0_SDK = "@stack0/sdk";
  async function getClient(): Promise<any> {
    if (!clientPromise) {
      clientPromise = (async () => {
        let mod: any;
        try {
          mod = await import(STACK0_SDK);
        } catch {
          throw new Error(
            "Stack0 SDK not installed. Run: npm install @stack0/sdk",
          );
        }
        return new mod.Stack0({ apiKey });
      })();
    }
    return clientPromise;
  }

  return createSdkMcpServer({
    name: "stack0",
    version: "0.1.0",
    tools: [
      tool(
        "send_email",
        "Send an email via Stack0. Supports HTML and plain text.",
        {
          to: z.string().describe("Recipient email address"),
          subject: z.string().describe("Email subject line"),
          html: z.string().optional().describe("HTML body content"),
          text: z.string().optional().describe("Plain text body (fallback)"),
          from: z
            .string()
            .optional()
            .describe("Sender email (must be verified domain)"),
        },
        async (args) => {
          try {
            const s = await getClient();
            const result = await s.mail.send({
              from: args.from ?? "noreply@agent.local",
              to: args.to,
              subject: args.subject,
              html: args.html ?? `<p>${args.text ?? args.subject}</p>`,
              text: args.text,
            });
            return {
              content: [
                {
                  type: "text" as const,
                  text: `Email sent successfully. ID: ${result?.id ?? "ok"}`,
                },
              ],
            };
          } catch (err) {
            const msg = err instanceof Error ? err.message : String(err);
            return {
              content: [
                { type: "text" as const, text: `Failed to send email: ${msg}` },
              ],
              isError: true,
            };
          }
        },
      ),
      tool(
        "screenshot",
        "Capture a screenshot of a webpage. Returns the image URL.",
        {
          url: z.string().describe("URL to screenshot"),
          fullPage: z
            .boolean()
            .optional()
            .describe("Capture full page (default: false)"),
          deviceType: z
            .string()
            .optional()
            .describe("desktop, tablet, or mobile (default: desktop)"),
          format: z
            .string()
            .optional()
            .describe("png, jpeg, or webp (default: png)"),
          blockAds: z
            .boolean()
            .optional()
            .describe("Block ads and trackers (default: true)"),
        },
        async (args) => {
          try {
            const s = await getClient();
            const result = await s.screenshots.captureAndWait({
              url: args.url,
              fullPage: args.fullPage ?? false,
              deviceType: args.deviceType ?? "desktop",
              format: args.format ?? "png",
              blockAds: args.blockAds ?? true,
              blockCookieBanners: true,
            });
            return {
              content: [
                {
                  type: "text" as const,
                  text: `Screenshot captured: ${result.imageUrl} (${result.imageWidth}x${result.imageHeight})`,
                },
              ],
            };
          } catch (err) {
            const msg = err instanceof Error ? err.message : String(err);
            return {
              content: [
                { type: "text" as const, text: `Screenshot failed: ${msg}` },
              ],
              isError: true,
            };
          }
        },
      ),
      tool(
        "extract",
        "Extract content from a webpage using AI. Can return markdown, structured data, or raw HTML.",
        {
          url: z.string().describe("URL to extract from"),
          mode: z
            .string()
            .optional()
            .describe(
              "Extraction mode: auto, markdown, schema, or html (default: markdown)",
            ),
          prompt: z
            .string()
            .optional()
            .describe("Optional prompt to guide extraction"),
        },
        async (args) => {
          try {
            const s = await getClient();
            const opts: Record<string, unknown> = {
              url: args.url,
              mode: args.mode ?? "markdown",
              includeLinks: true,
              includeMetadata: true,
            };
            if (args.prompt) opts.prompt = args.prompt;
            const result = await s.extraction.extractAndWait(opts);
            const data =
              typeof result.extractedData === "string"
                ? result.extractedData
                : JSON.stringify(result.extractedData, null, 2);
            return {
              content: [{ type: "text" as const, text: data }],
            };
          } catch (err) {
            const msg = err instanceof Error ? err.message : String(err);
            return {
              content: [
                { type: "text" as const, text: `Extraction failed: ${msg}` },
              ],
              isError: true,
            };
          }
        },
      ),
      tool(
        "upload",
        "Upload a file to Stack0 CDN. Returns the CDN URL. Accepts base64-encoded content.",
        {
          filename: z.string().describe("Filename including extension"),
          content: z.string().describe("Base64-encoded file content"),
          mimeType: z
            .string()
            .describe("MIME type (e.g., image/png, application/pdf)"),
          folder: z
            .string()
            .optional()
            .describe("CDN folder path (default: /)"),
        },
        async (args) => {
          try {
            const s = await getClient();
            const buffer = Buffer.from(args.content, "base64");
            const result = await s.cdn.upload({
              projectSlug: projectSlug ?? "default",
              file: buffer,
              filename: args.filename,
              mimeType: args.mimeType,
              folder: args.folder,
            });
            return {
              content: [
                {
                  type: "text" as const,
                  text: `Uploaded: ${result.cdnUrl} (${result.size} bytes)`,
                },
              ],
            };
          } catch (err) {
            const msg = err instanceof Error ? err.message : String(err);
            return {
              content: [
                { type: "text" as const, text: `Upload failed: ${msg}` },
              ],
              isError: true,
            };
          }
        },
      ),
      tool(
        "workflow",
        "Run a Stack0 workflow by ID or slug. Workflows are multi-step AI pipelines.",
        {
          workflowId: z.string().describe("Workflow ID or slug"),
          variables: z
            .record(z.string(), z.unknown())
            .optional()
            .describe("Input variables for the workflow"),
        },
        async (args) => {
          try {
            const s = await getClient();
            const result = await s.workflows.runAndWait({
              projectSlug: projectSlug ?? "default",
              workflowId: args.workflowId,
              variables: args.variables ?? {},
            });
            const output =
              typeof result.output === "string"
                ? result.output
                : JSON.stringify(result.output, null, 2);
            return {
              content: [
                {
                  type: "text" as const,
                  text: `Workflow completed (${result.totalDurationMs}ms):\n${output}`,
                },
              ],
            };
          } catch (err) {
            const msg = err instanceof Error ? err.message : String(err);
            return {
              content: [
                { type: "text" as const, text: `Workflow failed: ${msg}` },
              ],
              isError: true,
            };
          }
        },
      ),
      tool(
        "send_slack",
        "Send a message to a Slack channel via Stack0 integration.",
        {
          connectionId: z.string().describe("Stack0 Slack connection ID"),
          channelId: z.string().describe("Slack channel ID"),
          text: z.string().describe("Message text"),
        },
        async (args) => {
          try {
            const s = await getClient();
            await s.integrations.communication.sendMessage({
              connectionId: args.connectionId,
              channelId: args.channelId,
              text: args.text,
            });
            return {
              content: [
                { type: "text" as const, text: "Message sent to Slack" },
              ],
            };
          } catch (err) {
            const msg = err instanceof Error ? err.message : String(err);
            return {
              content: [
                { type: "text" as const, text: `Slack send failed: ${msg}` },
              ],
              isError: true,
            };
          }
        },
      ),
    ],
  });
}
