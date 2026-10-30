/**
 * The browser mock's models, tiers and automatic engines (`docs/plans/agents-model.md`, AM1, DEV only): enough of
 * `model_tiers`, `engines` and `agent_choice` for the Studio's engine pickers, the Models tab (AM3) and the Agents tab
 * (AM4) to be tried in `vite` without the Rust side. A rough copy — Rust is the authority on every rule.
 */

import type { ProviderId } from "./backend";
import type { Engine, EngineEntry, Role, Tier } from "./roster";
import type {
  AgentChoice,
  ModelChoice,
  ModelEntry,
  ModelPrice,
  TierEngines,
  TierLimits,
  TierProposal,
} from "../ide/models";

const TIERS: Tier[] = ["light", "medium", "heavy"];
const SMALL_LIST_MAX = 20;

interface MockModel {
  provider: ProviderId;
  id: string;
  name: string;
  price: ModelPrice | null;
  context: number | null;
  proposal: TierProposal | null;
}

const price = (input: number, output: number): ModelPrice => ({ input_per_m: input, output_per_m: output });
const MILLION = 1_000_000;

/** OpenRouter's long tail: more than SMALL_LIST_MAX models, so the list starts switched off and gets a filter. */
const TAIL: [string, string, number, number][] = [
  ["google/gemini-3-pro", "Google: Gemini 3 Pro", 12, 1_048_576],
  ["google/gemini-3-flash", "Google: Gemini 3 Flash", 1.2, 1_048_576],
  ["openai/gpt-6", "OpenAI: GPT-6", 15, 400_000],
  ["openai/gpt-6-mini", "OpenAI: GPT-6 mini", 0.8, 400_000],
  ["mistralai/mistral-large-3", "Mistral: Mistral Large 3", 6, 262_144],
  ["mistralai/devstral-3", "Mistral: Devstral 3", 0.9, 262_144],
  ["meta-llama/llama-5-405b", "Meta: Llama 5 405B", 3, 131_072],
  ["meta-llama/llama-5-70b", "Meta: Llama 5 70B", 0.6, 131_072],
  ["qwen/qwen3.5-coder", "Qwen: Qwen3.5 Coder", 1.6, 262_144],
  ["qwen/qwen3.5-max", "Qwen: Qwen3.5 Max", 6.4, 262_144],
  ["x-ai/grok-5", "xAI: Grok 5", 15, 256_000],
  ["x-ai/grok-5-fast", "xAI: Grok 5 Fast", 0.5, 2_000_000],
  ["moonshotai/kimi-k3", "MoonshotAI: Kimi K3", 2.5, 262_144],
  ["minimax/minimax-m3", "MiniMax: MiniMax M3", 1.1, 1_000_000],
  ["cohere/command-a-2", "Cohere: Command A 2", 10, 256_000],
  ["amazon/nova-2-pro", "Amazon: Nova 2 Pro", 3.2, 300_000],
  ["microsoft/phi-5", "Microsoft: Phi-5", 0.3, 131_072],
  ["nvidia/nemotron-ultra-2", "NVIDIA: Nemotron Ultra 2", 1.8, 131_072],
  ["ai21/jamba-2-large", "AI21: Jamba 2 Large", 8, 256_000],
  ["anthropic/claude-sonnet-5.5", "Anthropic: Claude Sonnet 5.5", 10, MILLION],
];

const catalog: MockModel[] = [
  { provider: "anthropic", id: "claude-fable-5-1", name: "Claude Fable 5.1", price: price(10, 50), context: MILLION, proposal: null },
  { provider: "anthropic", id: "claude-opus-5-5", name: "Claude Opus 5.5", price: price(4, 20), context: MILLION, proposal: null },
  {
    provider: "anthropic",
    id: "claude-sonnet-5-5",
    name: "Claude Sonnet 5.5",
    price: price(2, 10),
    context: MILLION,
    proposal: {
      tier: "medium",
      reason: "Ranks just below Opus 5.5 on the LMArena coding board, at half its price.",
      source: "https://lmarena.ai/leaderboard",
      at: "2026-10-29T18:00:00Z",
    },
  },
  { provider: "anthropic", id: "claude-haiku-5-5", name: "Claude Haiku 5.5", price: price(0.1, 0.5), context: MILLION, proposal: null },
  {
    provider: "open_router",
    id: "deepseek/deepseek-v4.1-flash",
    name: "DeepSeek: DeepSeek V4.1 Flash",
    price: price(0.1, 0.4),
    context: 163_840,
    proposal: null,
  },
  {
    provider: "open_router",
    id: "z-ai/glm-5.3",
    name: "Z.ai: GLM 5.3",
    price: price(0.6, 2.2),
    context: 202_752,
    proposal: {
      tier: "heavy",
      reason: "Scores 64 on the Artificial Analysis index, beside Opus 5.5, at a tenth of its price.",
      source: "https://artificialanalysis.ai/models/glm-5-3",
      at: "2026-10-29T18:00:00Z",
    },
  },
  ...TAIL.map(([id, name, out, context]) => ({
    provider: "open_router" as const,
    id,
    name,
    price: price(out / 4, out),
    context,
    proposal: null,
  })),
  { provider: "ollama", id: "qwen3:32b", name: "qwen3:32b", price: null, context: 40_960, proposal: null },
  {
    provider: "ollama",
    id: "gemma4:12b-mlx",
    name: "gemma4:12b-mlx",
    price: null,
    context: 131_072,
    proposal: {
      tier: "light",
      reason: "A 12B model; the Gemma 4 family ranks with the small hosted ones.",
      source: "https://lmarena.ai/leaderboard",
      at: "2026-10-28T09:00:00Z",
    },
  },
];

/** The owner's word: Opus rated by hand; of OpenRouter's long list a few switched on. */
let choices: Record<string, ModelChoice> = {
  "anthropic/claude-opus-5-5": { tier: "heavy" },
  "openrouter/deepseek/deepseek-v4.1-flash": { enabled: true },
  "openrouter/z-ai/glm-5.3": { enabled: true },
  "openrouter/google/gemini-3-flash": { enabled: true },
  "openrouter/openai/gpt-6": { enabled: true, tier: "heavy" },
};
/** The mock's research run: under way at first, so the Models tab's note can be seen. */
let researchRunning = true;
/** How long the mock's research run "takes". */
const RESEARCH_MS = 8000;
let limits: TierLimits = { light_below: 1, heavy_above: 10 };
let defaults: TierEngines = {};

const prefix = (provider: ProviderId) => (provider === "open_router" ? "openrouter" : provider);
const keyOf = (m: MockModel) => `${prefix(m.provider)}/${m.id}`;
const shortName = (name: string) => (name.split(": ").pop() ?? name).replace(/^Claude /, "");
const slug = (text: string) =>
  text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");

function propose(output: number): Tier {
  if (output < limits.light_below) return "light";
  if (output > limits.heavy_above) return "heavy";
  return "medium";
}

function entry(m: MockModel): ModelEntry {
  const key = keyOf(m);
  const enabledByDefault = catalog.filter((other) => other.provider === m.provider).length <= SMALL_LIST_MAX;
  const ownerTier = choices[key]?.tier ?? null;
  const priceTier = m.price ? propose(m.price.output_per_m) : null;
  const tier = ownerTier ?? m.proposal?.tier ?? priceTier;
  return {
    key,
    provider: m.provider,
    id: m.id,
    name: m.name,
    short_name: shortName(m.name),
    enabled: choices[key]?.enabled ?? enabledByDefault,
    enabled_by_default: enabledByDefault,
    tier,
    tier_source: ownerTier ? "owner" : m.proposal ? "research" : priceTier ? "price" : "unrated",
    owner_tier: ownerTier,
    proposal: m.proposal,
    price_tier: priceTier,
    price: m.price,
    context_length: m.context,
  };
}

const models = (): ModelEntry[] => catalog.map(entry);

/** The engines the list shows: the own ones, then one automatic engine per switched-on model. */
export function mockEngineEntries(own: Engine[], sessions: (id: string) => number): EngineEntry[] {
  const ownEntries: EngineEntry[] = own.map((e) => ({
    ...e,
    sessions: sessions(e.id),
    origin: "own",
    rated_tier: e.tier ?? null,
    tier_source: e.tier ? "owner" : "unrated",
    model_key: null,
    model_name: null,
    output_per_m: null,
    unusable: null,
  }));
  const auto: EngineEntry[] = models()
    .filter((m) => m.enabled)
    .map((m) => {
      const claude = m.provider === "anthropic";
      const model = claude ? m.id : m.key;
      const id = `${claude ? "claude-code" : "glut"}-${slug(model)}`;
      return {
        id,
        label: `${claude ? "Claude Code" : "Glut"} · ${m.name}`,
        harness: claude ? "claude_code" : "glut",
        command: "",
        model,
        env: "",
        billing: claude ? "subscription" : "metered",
        sessions: sessions(id),
        origin: "auto",
        rated_tier: m.tier,
        tier_source: m.tier_source,
        model_key: m.key,
        model_name: m.short_name,
        output_per_m: m.provider === "ollama" ? 0 : (m.price?.output_per_m ?? null),
        unusable: null,
      } satisfies EngineEntry;
    })
    .filter((e) => !own.some((o) => o.id === e.id));
  return [...ownEntries, ...auto];
}

/** The engines of `tier` in the order a start tries them: the default first, then by output price. */
function engineFor(tier: Tier, engines: EngineEntry[]): EngineEntry[] {
  const chosen = defaults[tier] ?? null;
  const first = engines.filter((e) => e.id === chosen);
  const rest = engines
    .filter((e) => e.rated_tier === tier && e.id !== chosen)
    .sort((a, b) => (a.output_per_m ?? Infinity) - (b.output_per_m ?? Infinity) || a.id.localeCompare(b.id));
  return [...first, ...rest];
}

const label = (word: string) => word.charAt(0).toUpperCase() + word.slice(1);

function choicesFor(roles: Role[], engines: EngineEntry[]): AgentChoice[] {
  const out: AgentChoice[] = [];
  for (const role of roles) {
    for (const tier of TIERS) {
      engineFor(tier, engines).forEach((e, at) => {
        const harness = e.harness === "claude_code" ? "Claude" : "Glut";
        const model = e.model_name ?? e.model ?? "default";
        const roleName = role.name.split(/[-_]/).map(label).join(" ");
        out.push({
          role: role.name,
          tier,
          engine_id: e.id,
          standard: at === 0,
          name: `${harness} ${label(tier)} ${roleName} (${model})`,
          title: `${roleName} · ${label(tier)}`,
          subtitle: `${harness} · ${model}`,
        });
      });
    }
  }
  return out;
}

/** Answers the model commands; `undefined` for any other command. */
export function mockModelCommand(
  cmd: string,
  args: Record<string, unknown>,
  engines: () => EngineEntry[],
  roles: () => Role[],
): { value: unknown } | undefined {
  switch (cmd) {
    case "list_models":
      return { value: { models: models(), tier_limits: { ...limits }, default_engines: { ...defaults } } };
    case "set_model_choice": {
      const key = String(args.key);
      if (!/^(anthropic|openrouter|ollama)\/\S/.test(key)) {
        throw new Error(`roster: invalid model: “${key}” is no model key`);
      }
      const choice = args.choice as ModelChoice;
      const next = { ...choices };
      if (choice.enabled == null && choice.tier == null) delete next[key];
      else next[key] = choice;
      choices = next;
      return { value: undefined };
    }
    case "set_tier_limits": {
      const next = args.limits as TierLimits;
      if (!(next.light_below >= 0 && next.light_below <= next.heavy_above)) {
        throw new Error("roster: invalid tier_limits: light_below must not be above heavy_above");
      }
      limits = { ...next };
      return { value: undefined };
    }
    case "set_default_engine": {
      const tier = args.tier as Tier;
      const engine = (args.engine as string | null) ?? null;
      if (engine && !engines().some((e) => e.id === engine)) {
        throw new Error(`roster: invalid engine: there is no engine “${engine}”`);
      }
      defaults = { ...defaults, [tier]: engine };
      return { value: undefined };
    }
    case "list_agent_choices":
      return { value: choicesFor(roles(), engines()) };
    case "model_research_running":
      return { value: researchRunning };
    case "research_model_tiers": {
      // The mock has no skill to run: it forgets the proposals asked about, as the app does before its run starts.
      const asked = (args.keys as string[] | null) ?? models().filter((m) => m.enabled).map((m) => m.key);
      const unknown = asked.find((key) => !catalog.some((m) => keyOf(m) === key));
      if (unknown) throw new Error(`roster: invalid model: the catalog offers no model “${unknown}”`);
      // Like the app, the old proposals stay until the run records new ones; the mock's run "ends" a moment later.
      researchRunning = true;
      setTimeout(() => (researchRunning = false), RESEARCH_MS);
      return { value: asked };
    }
    default:
      return undefined;
  }
}
