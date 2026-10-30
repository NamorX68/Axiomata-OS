<!--
  Settings → Workspace → Tags (`docs/plans/tags-links.md` TL4): the vault's one tag list (`TAGS.md`, written only from
  here — "nothing by hand in files"), each tag with its line of description; add, rename (the notes are rewritten
  along, after a count of what would change) and remove (the notes keep the tag). Above it the tags agents proposed,
  with Accept — the tag goes onto the list and onto the notes that asked for it — and Turn down. Below, the one run
  over the whole vault (`VaultTagging.svelte`, TL6).
-->
<script lang="ts">
  import { messageOf } from "../core/errors";
  import { onMount } from "svelte";

  import { invokeBackend, type RenameCount, type TagAnswer, type TagEntry, type TagProposal } from "../core/backend";
  import { relativeTime } from "../core/format";
  import { normaliseTag, refreshTagProposals, tagProblem, tagProposals, waitingLabel } from "../core/tagProposals";
  import { toast } from "../core/toast";
  import { tTags } from "../i18n/tags";
  import HexCount from "../ui/HexCount.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import VaultTagging from "./VaultTagging.svelte";

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

  /** The lead around the file name, which is shown as code. */
  const lead = $derived(tTags("lead").split("{file}"));

  const fetchProposals = () => invokeBackend<TagProposal[]>("tag_proposals");

  async function load() {
    loading = true;
    try {
      list = await invokeBackend<TagEntry[]>("get_tag_list");
      descriptions = Object.fromEntries(list.map((entry) => [entry.tag, entry.description]));
      loadError = null;
    } catch (err) {
      loadError = messageOf(err);
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
      toast(tTags("saveFailed", { error: messageOf(err) }), "danger");
      return false;
    } finally {
      busy = false;
    }
  }

  async function addTag() {
    if (newTag.trim() === "" || newProblem) return;
    const tag = normaliseTag(newTag);
    if (await save([...list, { tag, description: newDescription.trim() }], tTags("added", { tag }))) {
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
      tTags("descriptionSaved", { tag }),
    );
  }

  async function removeTag(tag: string) {
    if (!confirm(tTags("confirmRemove", { tag }))) return;
    await save(
      list.filter((e) => e.tag !== tag),
      tTags("removed", { tag }),
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
      toast(tTags("countFailed", { error: messageOf(err) }), "danger");
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
      toast(tTags("renamed", { from, to: normaliseTag(to), links: done.links, n: done.files }), "info");
      renaming = null;
      await load();
    } catch (err) {
      toast(tTags("renameFailed", { error: messageOf(err) }), "danger");
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
        toast(tTags("acceptedOn", { tag: result.tag, n: result.notes_tagged }), "info");
        for (const failure of result.notes_failed) toast(tTags("notSet", { tag: result.tag, failure }), "warning");
      } else {
        toast(tTags("turnedDown", { tag: result.tag }), "info");
      }
      await load();
    } catch (err) {
      toast(tTags("answerFailed", { error: messageOf(err) }), "danger");
    } finally {
      busy = false;
    }
  }

  onMount(load);
</script>

<section>
  <h3>{tTags("title")}</h3>
  <p class="lead">{lead[0]}<code>TAGS.md</code>{lead[1]}</p>

  {#if $tagProposals.length > 0}
    <div class="sub-head">
      <h3 class="sub">{tTags("proposedByAgents")}</h3>
      <HexCount count={$tagProposals.length} label={waitingLabel($tagProposals.length)} />
    </div>
    <ul class="rows">
      {#each $tagProposals as proposal (proposal.id)}
        <li class="row proposal">
          <span class="name">#{proposal.tag}</span>
          <input
            type="text"
            class="description"
            placeholder={tTags("descriptionPlaceholder")}
            aria-label={tTags("descriptionOf", { tag: proposal.tag })}
            value={proposalDescriptions[proposal.id] ?? proposal.description}
            oninput={(e) => (proposalDescriptions[proposal.id] = e.currentTarget.value)}
          />
          <span class="meta" title={proposal.notes.join("\n")}>
            {tTags("notes", { n: proposal.notes.length })} · {proposal.proposed_by} ·
            {relativeTime(proposal.proposed_at)}
          </span>
          <span class="buttons">
            <button type="button" disabled={busy} onclick={() => void answer(proposal, true)}>{tTags("accept")}</button>
            <button type="button" disabled={busy} onclick={() => void answer(proposal, false)}>
              {tTags("turnDown")}
            </button>
          </span>
        </li>
      {/each}
    </ul>
  {/if}

  {#if loadError}
    <p class="status error">{tTags("readError", { error: loadError })}</p>
  {:else if loading && list.length === 0}
    <p class="status">{tTags("reading")}</p>
  {:else}
    <h3 class="sub">{tTags("onTheList", { n: list.length })}</h3>
    {#if list.length === 0}
      <p class="hint">{tTags("noTags")}</p>
    {/if}
    <ul class="rows">
      {#each list as entry (entry.tag)}
        <li class="row">
          {#if renaming?.from === entry.tag}
            <span class="name">#{entry.tag} →</span>
            <input
              type="text"
              class="rename"
              aria-label={tTags("newNameOf", { tag: entry.tag })}
              spellcheck="false"
              value={renaming.to}
              oninput={(e) => renaming && (renaming = { ...renaming, to: e.currentTarget.value, count: null })}
              onkeydown={(e) => e.key === "Enter" && void (renaming?.count ? applyRename() : countRename())}
            />
            <span class="buttons">
              {#if renaming.count}
                <span class="meta">{tTags("usesIn", { uses: renaming.count.links, n: renaming.count.files })}</span>
                <button
                  type="button"
                  class="primary"
                  disabled={busy || !!renameProblem}
                  onclick={() => void applyRename()}
                >
                  {tTags("rename")}
                </button>
              {:else}
                <button type="button" disabled={busy || !!renameProblem} onclick={() => void countRename()}>
                  {tTags("countUses")}
                </button>
              {/if}
              <button type="button" disabled={busy} onclick={() => (renaming = null)}>{tTags("cancel")}</button>
            </span>
            {#if renameProblem}<p class="status invalid wide">{renameProblem}</p>{/if}
          {:else}
            <span class="name">#{entry.tag}</span>
            <input
              type="text"
              class="description"
              placeholder={tTags("descriptionPlaceholder")}
              aria-label={tTags("descriptionOf", { tag: entry.tag })}
              bind:value={descriptions[entry.tag]}
              onblur={() => void saveDescription(entry.tag)}
              onkeydown={(e) => e.key === "Enter" && e.currentTarget.blur()}
            />
            <span class="buttons">
              <IconButton
                icon="pencil"
                label={tTags("renameTag", { tag: entry.tag })}
                size="sm"
                disabled={busy}
                onclick={() => (renaming = { from: entry.tag, to: entry.tag, count: null })}
              />
              <IconButton
                icon="trash-2"
                label={tTags("takeOff", { tag: entry.tag })}
                size="sm"
                disabled={busy}
                onclick={() => void removeTag(entry.tag)}
              />
            </span>
          {/if}
        </li>
      {/each}
    </ul>
    <div class="add">
      <input
        type="text"
        class="new-tag"
        placeholder={tTags("newTagPlaceholder")}
        aria-label={tTags("newTag")}
        spellcheck="false"
        bind:value={newTag}
      />
      <input
        type="text"
        class="description"
        placeholder={tTags("descriptionPlaceholder")}
        aria-label={tTags("descriptionOfNew")}
        bind:value={newDescription}
        onkeydown={(e) => e.key === "Enter" && void addTag()}
      />
      <button type="button" disabled={busy || newTag.trim() === "" || !!newProblem} onclick={() => void addTag()}>
        {tTags("add")}
      </button>
    </div>
    {#if newProblem}<p class="status invalid">{newProblem}</p>{/if}
    <VaultTagging onChanged={() => void load()} />
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
