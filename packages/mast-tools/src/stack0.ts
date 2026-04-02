import type { Tool, ToolResult } from "./types.js";

/**
 * First-class Stack0 integration.
 * Uses @stack0/sdk to provide email, screenshots, web extraction,
 * CDN file management, and workflow execution as agent tools.
 */

export interface Stack0Config {
  apiKey: string;
  projectSlug?: string;
}

// Lazy-load the SDK to avoid hard dependency at compile time
const STACK0_SDK = "@stack0/sdk";
async function getClient(apiKey: string): Promise<any> {
  let mod: any;
  try {
    mod = await import(STACK0_SDK);
  } catch {
    throw new Error(
      "Stack0 SDK not installed. Run: npm install @stack0/sdk",
    );
  }
  return new mod.Stack0({ apiKey });
}

export function createStack0Tools(config: Stack0Config): Tool[] {
  const { apiKey, projectSlug } = config;
  let clientPromise: Promise<any> | null = null;

  function client(): Promise<any> {
    if (!clientPromise) clientPromise = getClient(apiKey);
    return clientPromise;
  }

  // --- Email Tools ---

  const sendEmail: Tool = {
    definition: {
      name: "stack0_send_email",
      description: "Send an email via Stack0. Supports HTML and plain text.",
      parameters: {
        type: "object",
        properties: {
          to: { type: "string", description: "Recipient email address" },
          subject: { type: "string", description: "Email subject line" },
          html: { type: "string", description: "HTML body content" },
          text: { type: "string", description: "Plain text body (fallback)" },
          from: { type: "string", description: "Sender email (must be verified domain)" },
        },
        required: ["to", "subject"],
      },
    },
    async execute(args): Promise<ToolResult> {
      try {
        const s = await client();
        const result = await s.mail.send({
          from: args.from as string ?? "noreply@agent.local",
          to: args.to as string,
          subject: args.subject as string,
          html: args.html as string ?? `<p>${args.text ?? args.subject}</p>`,
          text: args.text as string,
        });
        return { content: `Email sent successfully. ID: ${result?.id ?? "ok"}` };
      } catch (err) {
        return { content: `Failed to send email: ${err instanceof Error ? err.message : err}`, isError: true };
      }
    },
  };

  // --- Screenshot Tools ---

  const takeScreenshot: Tool = {
    definition: {
      name: "stack0_screenshot",
      description: "Capture a screenshot of a webpage. Returns the image URL.",
      parameters: {
        type: "object",
        properties: {
          url: { type: "string", description: "URL to screenshot" },
          fullPage: { type: "boolean", description: "Capture full page (default: false)" },
          deviceType: { type: "string", description: "desktop, tablet, or mobile (default: desktop)" },
          format: { type: "string", description: "png, jpeg, or webp (default: png)" },
          blockAds: { type: "boolean", description: "Block ads and trackers (default: true)" },
        },
        required: ["url"],
      },
    },
    async execute(args): Promise<ToolResult> {
      try {
        const s = await client();
        const result = await s.screenshots.captureAndWait({
          url: args.url as string,
          fullPage: args.fullPage as boolean ?? false,
          deviceType: args.deviceType as string ?? "desktop",
          format: args.format as string ?? "png",
          blockAds: args.blockAds as boolean ?? true,
          blockCookieBanners: true,
        });
        return { content: `Screenshot captured: ${result.imageUrl} (${result.imageWidth}x${result.imageHeight})` };
      } catch (err) {
        return { content: `Screenshot failed: ${err instanceof Error ? err.message : err}`, isError: true };
      }
    },
  };

  // --- Extraction Tools ---

  const extractWebpage: Tool = {
    definition: {
      name: "stack0_extract",
      description: "Extract content from a webpage using AI. Can return markdown, structured data, or raw HTML.",
      parameters: {
        type: "object",
        properties: {
          url: { type: "string", description: "URL to extract from" },
          mode: { type: "string", description: "Extraction mode: auto, markdown, schema, or html (default: markdown)" },
          prompt: { type: "string", description: "Optional prompt to guide extraction" },
          schema: { type: "object", description: "JSON Schema for structured extraction (when mode=schema)" },
        },
        required: ["url"],
      },
    },
    async execute(args): Promise<ToolResult> {
      try {
        const s = await client();
        const opts: Record<string, unknown> = {
          url: args.url as string,
          mode: args.mode as string ?? "markdown",
          includeLinks: true,
          includeMetadata: true,
        };
        if (args.prompt) opts.prompt = args.prompt;
        if (args.schema) opts.schema = args.schema;

        const result = await s.extraction.extractAndWait(opts);
        const data = typeof result.extractedData === "string"
          ? result.extractedData
          : JSON.stringify(result.extractedData, null, 2);
        return { content: data };
      } catch (err) {
        return { content: `Extraction failed: ${err instanceof Error ? err.message : err}`, isError: true };
      }
    },
  };

  // --- CDN Tools ---

  const uploadFile: Tool = {
    definition: {
      name: "stack0_upload",
      description: "Upload a file to Stack0 CDN. Returns the CDN URL. Accepts base64-encoded content.",
      parameters: {
        type: "object",
        properties: {
          filename: { type: "string", description: "Filename including extension" },
          content: { type: "string", description: "Base64-encoded file content" },
          mimeType: { type: "string", description: "MIME type (e.g., image/png, application/pdf)" },
          folder: { type: "string", description: "CDN folder path (default: /)" },
        },
        required: ["filename", "content", "mimeType"],
      },
    },
    async execute(args): Promise<ToolResult> {
      try {
        const s = await client();
        const buffer = Buffer.from(args.content as string, "base64");
        const result = await s.cdn.upload({
          projectSlug: projectSlug ?? "default",
          file: buffer,
          filename: args.filename as string,
          mimeType: args.mimeType as string,
          folder: args.folder as string,
        });
        return { content: `Uploaded: ${result.cdnUrl} (${result.size} bytes)` };
      } catch (err) {
        return { content: `Upload failed: ${err instanceof Error ? err.message : err}`, isError: true };
      }
    },
  };

  // --- Workflow Tools ---

  const runWorkflow: Tool = {
    definition: {
      name: "stack0_workflow",
      description: "Run a Stack0 workflow by ID or slug. Workflows are multi-step AI pipelines that can chain LLM calls, image generation, and more.",
      parameters: {
        type: "object",
        properties: {
          workflowId: { type: "string", description: "Workflow ID or slug" },
          variables: { type: "object", description: "Input variables for the workflow" },
        },
        required: ["workflowId"],
      },
    },
    async execute(args): Promise<ToolResult> {
      try {
        const s = await client();
        const result = await s.workflows.runAndWait({
          projectSlug: projectSlug ?? "default",
          workflowId: args.workflowId as string,
          variables: (args.variables as Record<string, unknown>) ?? {},
        });
        const output = typeof result.output === "string"
          ? result.output
          : JSON.stringify(result.output, null, 2);
        return { content: `Workflow completed (${result.totalDurationMs}ms):\n${output}` };
      } catch (err) {
        return { content: `Workflow failed: ${err instanceof Error ? err.message : err}`, isError: true };
      }
    },
  };

  // --- Integration Tools ---

  const sendSlackMessage: Tool = {
    definition: {
      name: "stack0_send_slack",
      description: "Send a message to a Slack channel via Stack0 integration. Requires a connected Slack integration.",
      parameters: {
        type: "object",
        properties: {
          connectionId: { type: "string", description: "Stack0 Slack connection ID" },
          channelId: { type: "string", description: "Slack channel ID" },
          text: { type: "string", description: "Message text" },
        },
        required: ["connectionId", "channelId", "text"],
      },
    },
    async execute(args): Promise<ToolResult> {
      try {
        const s = await client();
        await s.integrations.communication.sendMessage({
          connectionId: args.connectionId as string,
          channelId: args.channelId as string,
          text: args.text as string,
        });
        return { content: "Message sent to Slack" };
      } catch (err) {
        return { content: `Slack send failed: ${err instanceof Error ? err.message : err}`, isError: true };
      }
    },
  };

  return [
    sendEmail,
    takeScreenshot,
    extractWebpage,
    uploadFile,
    runWorkflow,
    sendSlackMessage,
  ];
}
