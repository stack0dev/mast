import { parseConfig } from "./config";
import { MastClient } from "./mast";
import { mastStore } from "./tools/store";
import { mastSearch } from "./tools/search";
import { mastForget } from "./tools/forget";
import { mastRelate } from "./tools/relate";
import { mastTraverse } from "./tools/traverse";
import { mastGraph } from "./tools/graph";
import { beforeAgentStart } from "./hooks/recall";
import { agentEnd } from "./hooks/capture";
import type {
  PluginConfig,
  StoreParams,
  SearchParams,
  ForgetParams,
  RelateParams,
  TraverseParams,
  ToolResult,
  HookContext,
  HookResult,
} from "./types";

let client: MastClient | null = null;
let pluginConfig: PluginConfig | null = null;

export function register(rawConfig: Partial<PluginConfig> = {}) {
  pluginConfig = parseConfig(rawConfig);
  client = new MastClient(pluginConfig);

  return {
    tools: {
      mast_store: (params: StoreParams) => getClient().then((c) => mastStore(c, params)),
      mast_search: (params: SearchParams) => getClient().then((c) => mastSearch(c, params)),
      mast_forget: (params: ForgetParams): ToolResult => mastForget(ensureClient(), params),
      mast_relate: (params: RelateParams): ToolResult => mastRelate(ensureClient(), params),
      mast_traverse: (params: TraverseParams): ToolResult => mastTraverse(ensureClient(), params),
      mast_graph: (): ToolResult => mastGraph(ensureClient()),
    },
    hooks: {
      before_agent_start: (context: HookContext): Promise<HookResult> =>
        getClient().then((c) => beforeAgentStart(c, pluginConfig!, context)),
      agent_end: (context: HookContext): Promise<void> =>
        getClient().then((c) => agentEnd(c, pluginConfig!, context)),
    },
    shutdown: () => {
      if (client) {
        client.close();
        client = null;
      }
    },
  };
}

async function getClient(): Promise<MastClient> {
  if (!client) throw new Error("Plugin not registered. Call register() first.");
  return client;
}

function ensureClient(): MastClient {
  if (!client) throw new Error("Plugin not registered. Call register() first.");
  return client;
}
