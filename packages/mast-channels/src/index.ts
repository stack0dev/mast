import type { Channel } from "./types.js";
import { CliChannel } from "./cli.js";
import { HttpChannel } from "./http.js";
import { TelegramChannel } from "./telegram.js";
import { SlackChannel } from "./slack.js";

export type { Channel, AgentHandler } from "./types.js";
export { CliChannel } from "./cli.js";
export { HttpChannel } from "./http.js";
export type { HttpChannelConfig, WsClientMessage, WsServerMessage } from "./http.js";
export { TelegramChannel } from "./telegram.js";
export type { TelegramChannelConfig } from "./telegram.js";
export { SlackChannel } from "./slack.js";
export type { SlackChannelConfig } from "./slack.js";

export interface ChannelsConfig {
  cli?: boolean;
  http?: { port: number; host?: string };
  telegram?: { token: string };
  slack?: { token: string; appToken: string; signingSecret: string };
}

export function createChannels(config: ChannelsConfig, agentName?: string): Channel[] {
  const channels: Channel[] = [];
  if (config.cli) channels.push(new CliChannel());
  if (config.http) channels.push(new HttpChannel(config.http, agentName));
  if (config.telegram) channels.push(new TelegramChannel(config.telegram));
  if (config.slack) channels.push(new SlackChannel(config.slack));
  return channels;
}
