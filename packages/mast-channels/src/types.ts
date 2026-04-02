export interface AgentHandler {
  handleMessage(
    userId: string,
    channel: string,
    message: string,
  ): Promise<string>;
}

export interface Channel {
  readonly name: string;
  start(agent: AgentHandler): Promise<void>;
  stop(): Promise<void>;
}
