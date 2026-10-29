<!--
  Settings → Workspace → Tags (`docs/plans/tags-links.md` TL4): the vault's one tag list (`TAGS.md`, written only from
  here — "nothing by hand in files"), each tag with its line of description; add, rename (the notes are rewritten
  along, after a count of what would change) and remove (the notes keep the tag). Above it the tags agents proposed,
  with Accept — the tag goes onto the list and onto the notes that asked for it — and Turn down.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import { invokeBackend, type RenameCount, type TagAnswer, type TagEntry, type TagProposal } from "../core/backend";
  import { relativeTime } from "../core/format";
  import { normaliseTag, refreshTagProposals, tagProblem, tagProposals, waitingLabel } from "../core/tagProposals";
  import { toast } from "../core/toast";
  import HexCount from "../ui/HexCount.svelte";
  import IconButton from "../ui/IconButton.svelte";

  let list = $state<TagEntry[]>([]);
  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let busy = $state(false);

  let newTag = $state("");
  let newDescription = $state("");
  const newProblem = $derived(newTag.trim() === "" ? null : tagProblem(newTag, list));

  /** The description being edited, by tag: saved when the field is left. */
  let descriptions = $state<Record<string, string>>({});
  /** A proposal's description as the owner may correct it before accepting, by id. */
  let proposalDescriptions = $state<Record<number, string>>({});

  /** The rename under way: which tag, to what, and what a dry run counted. */
  let renaming = $state<{ from: string; to: string; count: RenameCount | null } | null>(null);
  const renameProblem = $derived(renaming ? tagProblem(renaming.to, list, renaming.from) : null);

  const fetchProposals = () => invokeBackend<TagProposal[]>("tag_proposals");

  async function load() {
    loading = true;
    try {
      list = await invokeBackend<TagEntry[]>("get_tag_list");
      descriptions = Object.fromEntries(list.map((entry) => [entry.tag, entry.description]));
      loadError = null;
    } catch (err) {
      loadError = String(err);
    } finally {
      loading = false;
    }
    await refreshTagProposals(fetchProposals);
  }

  async function save(next: TagEntry[], done: string): Promise<boolean> {
    busy = true;
    try {
      list = await invokeBackend<TagEntry[]>("save_tag_list", { entries: next });
      descriptions = Object.fromEntries(list.map((entry) => [entry.tag, entry.description]));
      toast(done, "info");
      return true;
    } catch (err) {
      toast(`Could not save the tag list: ${String(err)}`, "danger");
      return false;
    } finally {
      busy = false;
    }
  }

  async function addTag() {
    if (newTag.trim() === "" || newProblem) return;
    const tag = normaliseTag(newTag);
    if (await save([...list, { tag, description: newDescription.trim() }], `#${tag} added`)) {
      newTag = "";
      newDescription = "";
    }
  }

  async function saveDescription(tag: string) {
    const entry = list.find((e) => e.tag === tag);
    const text = (descriptions[tag] ?? "").trim();
    if (!entry || entry.description === text) return;
    await save(
      list.map((e) => (e.tag === tag ? { ...e, description: text } : e)),
      `#${tag}: description saved`,
    );
  }

  async function removeTag(tag: string) {
    if (!confirm(`Take #${tag} off the list? Notes that carry it keep it; agents no longer choose it.`)) return;
    await save(
      list.filter((e) => e.tag !== tag),
      `#${tag} is off the list`,
    );
  }

  async function countRename() {
    if (!renaming || renameProblem) return;
    busy = true;
    try {
      const count = await invokeBackend<RenameCount>("rename_tag", {
        old: renaming.from,
        new: normaliseTag(renaming.to),
        dryRun: true,
      });
      renaming = { ...renaming, count };
    } catch (err) {
      toast(`Could not count the uses: ${String(err)}`, "danger");
    } finally {
      busy = false;
    }
  }

  async function applyRename() {
    if (!renaming?.count || renameProblem) return;
    const { from, to } = renaming;
    busy = true;
    try {
      const done = await invokeBackend<RenameCount>("rename_tag", { old: from, new: normaliseTag(to), dryRun: false });
      toast(`#${from} is #${normaliseTag(to)} now — ${done.links} uses in ${done.files} notes rewritten`, "info");
      renaming = null;
      await load();
    } catch (err) {
      toast(`Renaming failed: ${String(err)}`, "danger");
    } finally {
      busy = false;
    }
  }

  async function answer(proposal: TagProposal, accept: boolean) {
    busy = true;
    try {
      const result = await invokeBackend<TagAnswer>("answer_tag_proposal", {
        id: proposal.id,
        accept,
        description: accept ? (proposalDescriptions[proposal.id] ?? proposal.description) : null,
      });
      if (accept) {
        const notes = result.notes_tagged === 1 ? "1 note" : `${result.notes_tagged} notes`;
        toast(`#${result.tag} is on the list and set on ${notes}`, "info");
        for (const failure of result.notes_failed) toast(`#${result.tag} not set: ${failure}`, "warning");
      } else {
        toast(`#${result.tag} turned down`, "info");
      }
      await load();
    } catch (err) {
      toast(`Could not answer the proposal: ${String(err)}`, "danger");
    } finally {
      busy = false;
    }
  }

  onMount(load);
</script>

<section>
  <h3>Tags</h3>
  <p class="lead">
    The vault's tag list, kept in <code>TAGS.md</code>. Agents tag notes only from it; a tag they want that is not on it
    waits here for you. Aim for some 30–60 tags, 1–4 per note.
  </p>

  {#if $tagProposals.length > 0}
    <div class="sub-head">
      <h3 class="sub">Proposed by agents</h3>
      <HexCount count={$tagProposals.length} label={waitingLabel($tagProposals.length)} />
    </div>
    <ul class="rows">
      {#each $tagProposals as proposal (proposal.id)}
        <li class="row proposal">
          <span class="name">#{proposal.tag}</span>
          <input
            type="text"
            class="description"
            placeholder="A line of description"
            aria-label="Description of #{proposal.tag}"
            value={proposalDescriptions[proposal.id] ?? proposal.description}
            oninput={(e) => (proposalDescriptions[proposal.id] = e.currentTarget.value)}
          />
          <span class="meta" title={proposal.notes.join("\n")}>
            {proposal.notes.length === 1 ? "1 note" : `${proposal.notes.length} notes`} · {proposal.proposed_by} ·
            {relativeTime(proposal.proposed_at)}
          </span>
          <span class="buttons">
            <button type="button" class="primary" disabled={busy} onclick={() => void answer(proposal, true)}>Accept</button>
            <button type="button" disabled={busy} onclick={() => void answer(proposal, false)}>Turn down</button>
          </span>
        </li>
      {/each}
    </ul>
  {/if}

  {#if loadError}
    <p class="status error">Could not read the tag list: {loadError}</p>
  {:else if loading && list.length === 0}
    <p class="status">Reading…</p>
  {:else}
    <h3 class="sub">On the list ({list.length})</h3>
    {#if list.length === 0}
      <p class="hint">No tags yet.</p>
    {/if}
    <ul class="rows">
      {#each list as entry (entry.tag)}
        <li class="row">
          {#if renaming?.from === entry.tag}
            <span class="name">#{entry.tag} →</span>
            <input
              type="text"
              class="rename"
              aria-label="New name of #{entry.tag}"
              spellcheck="false"
              value={renaming.to}
              oninput={(e) => renaming && (renaming = { ...renaming, to: e.currentTarget.value, count: null })}
              onkeydown={(e) => e.key === "Enter" && void (renaming?.count ? applyRename() : countRename())}
            />
            <span class="buttons">
              {#if renaming.count}
                <span class="meta">{renaming.count.links} uses in {renaming.count.files} notes</span>
                <button type="button" class="primary" disabled={busy || !!renameProblem} onclick={() => void applyRename()}>
                  Rename
                </button>
              {:else}
                <button type="button" disabled={busy || !!renameProblem} onclick={() => void countRename()}>Count uses</button>
              {/if}
              <button type="button" disabled={busy} onclick={() => (renaming = null)}>Cancel</button>
            </span>
            {#if renameProblem}<p class="status invalid wide">{renameProblem}</p>{/if}
          {:else}
            <span class="name">#{entry.tag}</span>
            <input
              type="text"
              class="description"
              placeholder="A line of description"
              aria-label="Description of #{entry.tag}"
              bind:value={descriptions[entry.tag]}
              onblur={() => void saveDescription(entry.tag)}
              onkeydown={(e) => e.key === "Enter" && e.currentTarget.blur()}
            />
            <span class="buttons">
              <IconButton
                icon="pencil"
                label="Rename #{entry.tag}"
                size="sm"
                disabled={busy}
                onclick={() => (renaming = { from: entry.tag, to: entry.tag, count: null })}
              />
              <IconButton icon="trash-2" label="Take #{entry.tag} off the list" size="sm" disabled={busy} onclick={() => void removeTag(entry.tag)} />
            </span>
          {/if}
        </li>
      {/each}
    </ul>
    <div class="add">
      <input type="text" class="new-tag" placeholder="#new-tag" aria-label="New tag" spellcheck="false" bind:value={newTag} />
      <input
        type="text"
        class="description"
        placeholder="A line of description"
        aria-label="Description of the new tag"
        bind:value={newDescription}
        onkeydown={(e) => e.key === "Enter" && void addTag()}
      />
      <button type="button" disabled={busy || newTag.trim() === "" || !!newProblem} onclick={() => void addTag()}>Add</button>
    </div>
    {#if newProblem}<p class="status invalid">{newProblem}</p>{/if}
  {/if}
</section>

<style>
  h3 {
    font: var(--ax-type-label);
    color: var(--ax-text-muted);
    margin-bottom: var(--ax-space-2);
  }
  h3.sub {
    margin: var(--ax-space-3) 0 var(--ax-space-2);
  }
  .sub-head {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
  }
  .lead,
  .hint {
    margin: 0 0 var(--ax-space-3);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }
  .lead code {
    font-family: var(--ax-font-mono);
    color: var(--ax-text);
  }
  .rows {
    list-style: none;
    margin: 0 0 var(--ax-space-3);
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
  }
  .row,
  .add {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--ax-space-2);
    min-width: 0;
    font-size: var(--ax-font-size-sm);
  }
  .row {
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
  }
  .name {
    flex: 0 0 auto;
    min-width: calc(120px * var(--ax-ui-scale));
    font-family: var(--ax-font-mono);
  }
  .description {
    flex: 1 1 calc(180px * var(--ax-ui-scale));
    min-width: 0;
  }
  .rename,
  .new-tag {
    flex: 0 1 calc(160px * var(--ax-ui-scale));
    min-width: 0;
    font-family: var(--ax-font-mono);
  }
  .meta {
    color: var(--ax-text-muted);
    white-space: nowrap;
  }
  .buttons {
    display: flex;
    align-items: center;
    gap: var(--ax-space-1);
    margin-left: auto;
  }
  .status {
    margin: 0 0 var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }
  .status.error,
  .status.invalid {
    color: var(--ax-danger);
  }
  .wide {
    flex-basis: 100%;
  }
</style>
