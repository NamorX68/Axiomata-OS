/**
 * The browser mock's tags (`docs/plans/tags-links.md`, DEV only): the tag list, the proposals waiting for the owner,
 * the renames and the vault run, so Settings → Workspace → Tags, the badge and the Second Brain's tag filter can be
 * tried in `vite` without the Rust side. `devmock.ts` hands every tag command here.
 */

import type {
  PlannedNote,
  RenameCount,
  TagAnswer,
  TagEntry,
  TagProposal,
  VaultEstimate,
  VaultPhase,
  VaultTagging,
} from "./backend";

let list: TagEntry[] = [
  { tag: "ki-modelle", description: "AI models and their releases, collected by the model-news skill" },
  { tag: "ki-news", description: "AI news, collected by the news skills" },
  { tag: "rust", description: "Rust, the language and its tools" },
  { tag: "rust/async", description: "Async Rust: tokio, futures" },
  { tag: "fotografie", description: "Photography: gear, technique, trips" },
];

let proposals: TagProposal[] = [
  {
    id: 1,
    tag: "projekt/axiomata",
    description: "Notes about building Axiomata-OS",
    notes: ["Entwicklung/Rust/Entwicklung Notiz 1.md", "Arbeit/Arbeit Notiz 3.md"],
    proposed_by: "a skill run",
    proposed_at: new Date(Date.now() - 3_600_000).toISOString(),
  },
  {
    id: 2,
    tag: "reise",
    description: "",
    notes: ["Persönlich/Persönlich Notiz 2.md"],
    proposed_by: "the chat",
    proposed_at: new Date(Date.now() - 600_000).toISOString(),
  },
];

/** Tags the mock graph hands out, by a note's position: a few notes each, some nested. */
const GRAPH_TAGS: readonly string[][] = [
  ["rust"],
  ["rust", "rust/async"],
  ["ki-news"],
  ["ki-modelle", "ki-news"],
  ["fotografie"],
  [],
  [],
];

/** The tags of the mock graph's `index`-th note in `area`. */
export function mockGraphTags(area: string, index: number): string[] {
  if (area === "Entwicklung") return GRAPH_TAGS[index % 2] ?? [];
  if (area === "KI") return GRAPH_TAGS[2 + (index % 2)] ?? [];
  if (area === "Fotografie") return index % 2 === 0 ? ["fotografie"] : [];
  return GRAPH_TAGS[5 + (index % 2)] ?? [];
}

let vaultPhase: VaultPhase | null = null;
let plan: PlannedNote[] = [];

const mockEstimate = (phase: VaultPhase): VaultEstimate => ({
  phase,
  notes: 252,
  bytes: 1_150_000,
  input_tokens: phase === "list" ? 595_000 : 1_240_000,
  output_tokens: phase === "list" ? 9_000 : 37_800,
  model: "deepseek/deepseek-v3.2",
  cost_usd: phase === "list" ? 0.17 : 0.36,
});

const sorted = (entries: TagEntry[]): TagEntry[] => [...entries].sort((a, b) => a.tag.localeCompare(b.tag));

/** Answers a tag command, or `undefined` when `cmd` is none. */
export function mockTagCommand(cmd: string, args: Record<string, unknown>): { value: unknown } | undefined {
  switch (cmd) {
    case "get_tag_list":
      return { value: sorted(list) };
    case "save_tag_list": {
      const entries = (args.entries as TagEntry[]).map((e) => ({
        tag: e.tag.replace(/^#/, "").toLowerCase(),
        description: e.description,
      }));
      const seen = new Set<string>();
      for (const entry of entries) {
        if (!/^[\p{L}\p{N}_\-/]+$/u.test(entry.tag) || /^[\d/]+$/.test(entry.tag)) {
          throw new Error(`tags: "${entry.tag}" is no tag`);
        }
        if (seen.has(entry.tag)) throw new Error(`tags: #${entry.tag} is on the list twice`);
        seen.add(entry.tag);
      }
      list = sorted(entries);
      return { value: list };
    }
    case "tag_proposals":
      return { value: proposals };
    case "answer_tag_proposal": {
      const id = Number(args.id);
      const proposal = proposals.find((p) => p.id === id);
      if (!proposal) throw new Error(`tags: there is no open tag proposal #${id}`);
      proposals = proposals.filter((p) => p.id !== id);
      const accepted = Boolean(args.accept);
      if (accepted && !list.some((e) => e.tag === proposal.tag)) {
        const typed = typeof args.description === "string" ? args.description.trim() : "";
        const description = typed || proposal.description;
        list = sorted([...list, { tag: proposal.tag, description }]);
      }
      const answer: TagAnswer = {
        tag: proposal.tag,
        accepted,
        notes_tagged: accepted ? proposal.notes.length : 0,
        notes_failed: [],
      };
      return { value: answer };
    }
    case "rename_tag": {
      const old = String(args.old).replace(/^#/, "").toLowerCase();
      const next = String(args.new).replace(/^#/, "").toLowerCase();
      if (old === next) throw new Error("tags: the old and the new tag are the same");
      if (!args.dryRun) {
        list = sorted(
          list
            .map((e) =>
              e.tag === old || e.tag.startsWith(`${old}/`) ? { ...e, tag: next + e.tag.slice(old.length) } : e,
            )
            .filter((e, i, all) => all.findIndex((o) => o.tag === e.tag) === i),
        );
      }
      const count: RenameCount = { files: 4, links: 6 };
      return { value: count };
    }
    case "vault_tagging_status": {
      const status: VaultTagging = {
        phase: vaultPhase,
        list_estimate: mockEstimate("list"),
        notes_estimate: mockEstimate("notes"),
        plan,
      };
      return { value: status };
    }
    case "vault_tagging_start": {
      const phase = args.phase as VaultPhase;
      if (phase === "notes") {
        plan = [
          {
            note: "Entwicklung/Rust/Entwicklung Notiz 1.md",
            tags: ["rust"],
            related: ["Entwicklung Notiz 4"],
            proposed: [],
          },
          { note: "KI/KI Notiz 2.md", tags: ["ki-news"], related: [], proposed: ["ki/agenten"] },
          { note: "Persönlich/Persönlich Notiz 1.md", tags: [], related: [], proposed: [] },
        ];
      } else {
        proposals = [
          ...proposals,
          {
            id: 90,
            tag: "werkzeuge",
            description: "Tools and their setup",
            notes: [],
            proposed_by: "a skill run",
            proposed_at: new Date().toISOString(),
          },
        ];
      }
      vaultPhase = null;
      return { value: { id: 1, skill_name: "vault-tagging", status: "success", duration_ms: 1200 } };
    }
    case "vault_tagging_apply": {
      const written = plan.filter((p) => p.tags.length > 0 || p.related.length > 0).length;
      const applied = { written, unchanged: plan.length - written, failed: [] };
      plan = [];
      return { value: applied };
    }
    case "vault_tagging_discard":
      plan = [];
      vaultPhase = null;
      return { value: undefined };
    case "vault_tagging_end":
      vaultPhase = null;
      return { value: undefined };
    case "rename_note_links": {
      const count: RenameCount = { files: 2, links: 3 };
      return { value: count };
    }
    default:
      return undefined;
  }
}
