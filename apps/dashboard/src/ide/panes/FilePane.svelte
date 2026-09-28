<!--
  A file in the editor, as an IDE dock pane (`docs/plans/git-layer.md`, H5,
  H14): opened from a diff on the agent's worktree, but general — any root the
  file service knows, so the later IDE file view uses the same pane.

  * **The editing is `FileEditor`'s**, the same component as the file app's:
    banners for external changes (the agent may write this file while it is
    open), recovery, autosave, Compare.
  * **A new `jump` in the tab's config moves the cursor** — opening the same
    file from the diff again brings this pane forward at the new line.
  * **A missing file or root says so and stays**: a discarded worktree must
    not make panes vanish from the layout without a word.
  * **G6**: while the agent owning the worktree is `working`, a hint says that
    it may change the file under your hands.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import FileEditor from "../../fileapp/FileEditor.svelte";
  import { agentStatus } from "../agentStatus";
  import { worktreeAgent, type FilePaneConfig } from "../paneKinds";
  import { session } from "../projectSession";
  import type { LocationList } from "../../fileapp/locationList";

  let {
    config,
    visible,
    onQuit,
    onOpenFile,
    onShowLocations,
  }: {
    config: FilePaneConfig;
    visible: boolean;
    /** Vi's `ZZ`/`ZQ`: close this pane's tab. */
    onQuit?: () => void;
    /** A definition in another file (ED6.2): the dock opens it as a tab. */
    onOpenFile?: (file: { root: string; rel: string }, line: number) => void;
    /** A language server's list of places (ED6.3): the IDE's Search pane shows it. */
    onShowLocations?: (list: LocationList) => void;
  } = $props();

  let editor = $state<FileEditor | null>(null);
  let failure = $state<string | null>(null);
  /** The last `jump` acted on; a newer one moves the cursor. */
  let lastJump = 0;

  const statuses = agentStatus.statuses;
  const ownerAgent = $derived(worktreeAgent(config.root));
  const notice = $derived.by(() => {
    if (ownerAgent === null || $statuses.byAgent.get(ownerAgent)?.state !== "working") return null;
    const name = $session.agents.find((a) => a.id === ownerAgent)?.name ?? "The agent";
    return `${name} is working in this worktree and may change this file while you edit it — you'll be asked before anything is overwritten.`;
  });

  onMount(() => {
    lastJump = config.jump;
    void editor?.open({ root: config.root, rel: config.rel }, config.line).then((result) => {
      if (result.ok) return;
      failure =
        result.kind === "UnknownRoot"
          ? `${config.rel}: the place it lived in is gone (a discarded worktree, or a removed project).`
          : result.kind === "NotFound"
            ? `${config.rel} does not exist (any more).`
            : `Could not open ${config.rel}: ${result.message}`;
    });
  });

  // Opened again from the diff: jump to the new line.
  $effect(() => {
    const jump = config.jump;
    const line = config.line;
    if (jump === lastJump) return;
    lastJump = jump;
    if (line !== null) editor?.goToLine(line);
  });
</script>

<div class="file-pane">
  {#if failure}
    <p class="failure">{failure}</p>
  {:else}
    <FileEditor bind:this={editor} {visible} {notice} {onQuit} {onOpenFile} {onShowLocations} />
  {/if}
</div>

<style>
  .file-pane {
    position: absolute;
    inset: 0;
    display: flex;
  }

  .failure {
    margin: 0;
    padding: var(--ax-space-4);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
</style>
