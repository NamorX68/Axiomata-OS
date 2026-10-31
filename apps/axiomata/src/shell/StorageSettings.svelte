<!--
  Settings → Storage (`docs/plans/maintenance.md`, MT4): how much Axiomata's own data takes, how long each kind stays
  (the rules are editable), when the last clean-up ran, "Clean up now" (it says first what goes) and the orphaned
  worktrees that wait for the owner. Glut's row comes from `glut clean --dry-run --json`; without a Glut that can
  answer it is simply not there. Nothing here touches projects, the vault or `~/.glut` directly.
-->
<script lang="ts">
  import { messageOf } from "../core/errors";
  import { onMount } from "svelte";

  import { invokeBackend } from "../core/backend";
  import { absoluteTime, formatBytes, shortPath } from "../core/format";
  import {
    type CleanOutcome,
    type MaintenanceItem,
    type MaintenanceSettings,
    type OrphanWorktree,
    type RuleField,
    type StorageView,
    freeableTotal,
    glutLabel,
    ITEM_RULES,
    itemLabel,
    keepText,
    RULE_BOUNDS,
    ruleProblem,
    rulesChanged,
    unitCount,
    waitingWorktrees,
  } from "../core/maintenance";
  import { toast } from "../core/toast";
  import { tStorage } from "../i18n/storage";
  import { deleteAgentsBackup, listAgentsBackups, type AgentsBackup } from "../ide/agentsMigration";
  import IconButton from "../ui/IconButton.svelte";

  let view = $state<StorageView | null>(null);
  let draft = $state<MaintenanceSettings | null>(null);
  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let cleaning = $state(false);
  let saving = $state(false);
  let removing = $state<string | null>(null);
  /** The agents migration's backup folders (agents-model.md, AM6): kept until the owner deletes them here. */
  let backups = $state<AgentsBackup[]>([]);

  const problem = $derived(draft ? ruleProblem(draft) : null);
  const changed = $derived(view !== null && draft !== null && rulesChanged(view.settings, draft));
  const waiting = $derived(view ? waitingWorktrees(view.axiomata) : []);
  const freeable = $derived(view ? freeableTotal(view) : 0);
  /** The texts around a folder name, which is shown as code. */
  const lead = $derived(tStorage("lead").split("{home}"));
  const glutKeep = $derived(tStorage("glutKeep").split("{folder}"));

  async function load() {
    loading = true;
    try {
      const loaded = await invokeBackend<StorageView>("maintenance_report");
      view = loaded;
      backups = await listAgentsBackups().catch(() => []);
      // Keep what the owner is typing across a reload after a clean-up.
      if (!draft || !changed) draft = { ...loaded.settings };
      loadError = null;
    } catch (err) {
      loadError = messageOf(err);
    } finally {
      loading = false;
    }
  }

  async function saveRules() {
    if (!draft || problem) return;
    saving = true;
    try {
      const saved = await invokeBackend<MaintenanceSettings>("maintenance_save_settings", { settings: draft });
      draft = { ...saved };
      toast(tStorage("rulesSaved"), "info");
      await load();
    } catch (err) {
      toast(tStorage("saveRulesFailed", { error: messageOf(err) }), "danger");
    } finally {
      saving = false;
    }
  }

  async function cleanNow() {
    if (!view) return;
    const question =
      freeable > 0
        ? tStorage("confirmCleanFrees", { size: formatBytes(freeable) })
        : tStorage("confirmCleanNothing");
    if (!confirm(question)) return;
    cleaning = true;
    try {
      const outcome = await invokeBackend<CleanOutcome>("maintenance_clean");
      toast(tStorage("cleaned", { size: formatBytes(outcome.freed) }), "info");
      if (outcome.glut_note) toast(outcome.glut_note, "warning");
      view = outcome.view;
    } catch (err) {
      toast(tStorage("cleanFailed", { error: messageOf(err) }), "danger");
    } finally {
      cleaning = false;
    }
  }

  async function removeWorktree(orphan: OrphanWorktree) {
    const what = orphan.dirty ? tStorage("worktreeDirty") : tStorage("worktreeUnknown");
    if (!confirm(tStorage("confirmDeleteWorktree", { path: orphan.path, what }))) return;
    removing = orphan.path;
    try {
      await invokeBackend<void>("maintenance_remove_worktree", { path: orphan.path });
      toast(tStorage("worktreeDeleted"), "info");
      await load();
    } catch (err) {
      toast(tStorage("worktreeDeleteFailed", { error: messageOf(err) }), "danger");
    } finally {
      removing = null;
    }
  }

  async function removeBackup(backup: AgentsBackup) {
    if (!confirm(tStorage("confirmDeleteBackup", { path: backup.path }))) return;
    removing = backup.path;
    try {
      await deleteAgentsBackup(backup.path);
      toast(tStorage("backupDeleted"), "info");
      await load();
    } catch (err) {
      toast(tStorage("backupDeleteFailed", { error: messageOf(err) }), "danger");
    } finally {
      removing = null;
    }
  }

  function setRule(field: RuleField, raw: string) {
    if (!draft) return;
    draft = { ...draft, [field]: raw.trim() === "" ? Number.NaN : Number(raw) };
  }

  function amount(item: MaintenanceItem): string {
    const size = item.bytes !== null ? formatBytes(item.bytes) : "";
    const count = item.unit ? unitCount(item.count, item.unit) : "";
    return [size, count].filter(Boolean).join(" · ");
  }

  function frees(item: MaintenanceItem): string {
    if (item.freeableBytes) return formatBytes(item.freeableBytes);
    if (item.freeableCount > 0) return unitCount(item.freeableCount, item.unit);
    return "—";
  }

  onMount(load);
</script>

<section>
  <h3>{tStorage("title")}</h3>
  <p class="lead">{lead[0]}<code>~/.axiomata</code>{lead[1]}</p>

  {#if loadError}
    <p class="status error">{tStorage("measureError", { error: loadError })}</p>
  {:else if !view || !draft}
    <p class="status">{loading ? tStorage("measuring") : "—"}</p>
  {:else}
    {#if view.axiomata.error}
      <p class="status error">{view.axiomata.error}</p>
    {/if}
    <dl class="summary">
      <dt>{shortPath(view.axiomata.home, 1)}</dt>
      <dd>{formatBytes(view.axiomata.totalBytes)}</dd>
      <dt>{tStorage("lastCleanup")}</dt>
      <dd>{view.axiomata.lastRun ? absoluteTime(view.axiomata.lastRun) : tStorage("never")}</dd>
      <dt>{tStorage("wouldFree")}</dt>
      <dd>{formatBytes(freeable)}</dd>
    </dl>

    <ul class="items">
      {#each view.axiomata.items as item (item.key)}
        <li class="item">
          <div class="head">
            <span class="title">{itemLabel(item.key, item.label)}</span>
            <span class="amount">{amount(item)}</span>
          </div>
          <div class="rule">
            {#if ITEM_RULES[item.key]}
              {#each ITEM_RULES[item.key] as rule (rule.field)}
                <label class="rule-field">
                  <input
                    type="number"
                    min={RULE_BOUNDS[rule.field].min}
                    max={RULE_BOUNDS[rule.field].max}
                    step="1"
                    value={Number.isNaN(draft[rule.field]) ? "" : draft[rule.field]}
                    oninput={(e) => setRule(rule.field, e.currentTarget.value)}
                  />
                  <span>{rule.unit === "days" ? tStorage("days") : tStorage("kept")}</span>
                </label>
              {/each}
            {:else}
              <span class="keep">{keepText(item)}</span>
            {/if}
            <span class="frees">{tStorage("frees", { amount: frees(item) })}</span>
          </div>
          {#if item.error}
            <p class="item-error">{item.error}</p>
          {/if}
        </li>
      {/each}
      {#if view.glut}
        <li class="item">
          {#if view.glut.state === "known"}
            <div class="head">
              <span class="title">Glut</span>
              <span class="amount">{formatBytes(view.glut.report.totalBytes)}</span>
            </div>
            <div class="rule">
              <span class="keep">{glutKeep[0]}<code>~/.glut</code>{glutKeep[1]}</span>
              <span class="frees">
                {tStorage("frees", {
                  amount: view.glut.report.freeableBytes ? formatBytes(view.glut.report.freeableBytes) : "—",
                })}
              </span>
            </div>
            <ul class="glut-items">
              {#each view.glut.report.items.filter((part) => part.bytes > 0) as part (part.name)}
                <li>
                  <span class="glut-name">{glutLabel(part.name)}</span>
                  <span class="amount">{formatBytes(part.bytes)}{part.keep ? ` · ${part.keep}` : ""}</span>
                  <span class="frees">{part.freeableBytes ? formatBytes(part.freeableBytes) : "—"}</span>
                </li>
              {/each}
            </ul>
          {:else if view.glut.state === "failed"}
            <div class="head"><span class="title">Glut</span></div>
            <p class="item-error">{view.glut.message}</p>
          {:else}
            <div class="head"><span class="title">{tStorage("glutUnknown")}</span></div>
          {/if}
        </li>
      {/if}
    </ul>

    <label class="opt">
      <input type="checkbox" bind:checked={draft.enabled} />
      {tStorage("weekly")}
    </label>
    {#if problem}
      <p class="status invalid">{problem}</p>
    {/if}

    <div class="actions">
      <button type="button" disabled={!changed || !!problem || saving} onclick={saveRules}>
        {saving ? tStorage("saving") : tStorage("saveRules")}
      </button>
      <button type="button" disabled={cleaning || changed} onclick={cleanNow}>
        {cleaning ? tStorage("cleaningUp") : tStorage("cleanNow")}
      </button>
    </div>
    {#if changed}
      <p class="hint">{tStorage("saveFirst")}</p>
    {/if}

    {#if view.axiomata.endedSessions.length > 0}
      {@const sessions = view.axiomata.endedSessions}
      <h3 class="sub">{view.axiomata.dryRun ? tStorage("endedToRemove") : tStorage("endedRemoved")}</h3>
      <p class="hint">{tStorage("endedHint")}</p>
      <ul class="orphans">
        {#each sessions as session (session.id)}
          <li class="orphan"><span class="path">{session.name}</span></li>
        {/each}
      </ul>
    {/if}

    {#if waiting.length > 0}
      <h3 class="sub">{tStorage("orphansTitle")}</h3>
      <p class="hint">{tStorage("orphansHint")}</p>
      <ul class="orphans">
        {#each waiting as orphan (orphan.path)}
          <li class="orphan">
            <span class="path" title={orphan.path}>{shortPath(orphan.path, 2)}</span>
            <span class="amount">
              {formatBytes(orphan.bytes)}{orphan.dirty === null ? ` · ${tStorage("unknownState")}` : ""}
            </span>
            <IconButton
              icon="trash-2"
              label={tStorage("deleteWorktree", { path: orphan.path })}
              size="sm"
              disabled={removing !== null}
              onclick={() => void removeWorktree(orphan)}
            />
          </li>
        {/each}
      </ul>
    {/if}

    {#if backups.length > 0}
      <h3 class="sub">{tStorage("backupsTitle")}</h3>
      <p class="hint">{tStorage("backupsHint")}</p>
      <ul class="orphans">
        {#each backups as backup (backup.path)}
          <li class="orphan">
            <span class="path" title={backup.path}>{shortPath(backup.path, 2)}</span>
            <span class="amount">{formatBytes(backup.bytes)}</span>
            <IconButton
              icon="trash-2"
              label={tStorage("deleteBackup", { path: backup.path })}
              size="sm"
              disabled={removing !== null}
              onclick={() => void removeBackup(backup)}
            />
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  h3 {
    font: var(--ax-type-label);
    color: var(--ax-text-muted);
    margin-bottom: var(--ax-space-2);
  }
  h3.sub {
    margin-top: var(--ax-space-4);
  }
  .lead {
    margin: 0 0 var(--ax-space-3);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }
  .lead code,
  .keep code {
    font-family: var(--ax-font-mono);
    color: var(--ax-text);
  }
  .summary {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--ax-space-1) var(--ax-space-4);
    margin: 0 0 var(--ax-space-3);
    font-size: var(--ax-font-size-sm);
  }
  .summary dt {
    color: var(--ax-text-muted);
  }
  .summary dd {
    margin: 0;
  }
  .items,
  .orphans {
    list-style: none;
    margin: 0 0 var(--ax-space-3);
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
  }
  .item {
    padding: var(--ax-space-2);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    font-size: var(--ax-font-size-sm);
  }
  .head,
  .rule,
  .orphan {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    min-width: 0;
  }
  .rule {
    margin-top: var(--ax-space-1);
    color: var(--ax-text-muted);
    flex-wrap: wrap;
  }
  .title {
    font-weight: 600;
    flex: 1 1 auto;
    min-width: 0;
  }
  .amount,
  .frees {
    color: var(--ax-text-muted);
    white-space: nowrap;
  }
  .frees {
    margin-left: auto;
  }
  .keep {
    min-width: 0;
  }
  .rule-field {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-1);
  }
  .rule-field input {
    width: calc(64px * var(--ax-ui-scale));
    padding: 0 var(--ax-space-1);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text);
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
  }
  .rule-field input:focus {
    outline: none;
    border-color: var(--ax-accent);
  }
  .glut-items {
    list-style: none;
    margin: var(--ax-space-1) 0 0;
    padding: 0;
    color: var(--ax-text-muted);
  }
  .glut-items li {
    display: flex;
    gap: var(--ax-space-2);
  }
  .glut-name {
    flex: 1 1 auto;
    min-width: 0;
  }
  .item-error {
    margin: var(--ax-space-1) 0 0;
    color: var(--ax-warning);
    word-break: break-word;
  }
  .orphan {
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    font-size: var(--ax-font-size-sm);
  }
  .orphan .path {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--ax-font-mono);
  }
  .opt {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    margin-bottom: var(--ax-space-2);
  }
  .status {
    margin: 0 0 var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }
  .status.invalid,
  .status.error {
    color: var(--ax-warning);
  }
  .hint {
    margin: var(--ax-space-2) 0 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
  .actions {
    display: flex;
    gap: var(--ax-space-2);
  }
</style>
