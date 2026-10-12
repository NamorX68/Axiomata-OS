<!--
  The sounds (`docs/plans/ui-ux-round.md` UX7d), nothing to see: while they are switched on (Appearance → Sound,
  `core/effects.ts`, off by default), the watcher (`core/soundWatch.ts`) looks at all agents and routines and plays a
  cue when Axiomata lies in the background. The first click or key press wakes the audio (`shell/soundPlayer.ts`).
-->
<script lang="ts">
  import { invokeBackend, type AgentStatus, type IdeProject, type Routine } from "../core/backend";
  import { effects } from "../core/effects";
  import { startSoundWatch } from "../core/soundWatch";
  import { sceneOf, type Watched } from "../core/sounds";

  import { closeAudio, playCue, unlockAudio } from "./soundPlayer";

  /** Only a real flip restarts the watcher; switching one of the other effects must not lose its scene. */
  const on = $derived($effects.sounds);

  /**
   * All agents of all projects and all routines, in one look. A project whose states cannot be read keeps the ones of
   * its last good look: one broken project must neither silence the others nor make its agents vanish and come back
   * as "new and waiting".
   */
  function looker(): () => Promise<Watched> {
    const lastByProject = new Map<number, AgentStatus[]>();
    return async () => {
      const [projects, routines] = await Promise.all([
        invokeBackend<IdeProject[]>("list_ide_projects"),
        invokeBackend<Routine[]>("list_routines"),
      ]);
      const answers = await Promise.allSettled(
        projects.map((p) => invokeBackend<AgentStatus[]>("ide_agent_states", { projectId: p.id })),
      );
      const states = projects.map((p, i) => {
        const answer = answers[i];
        if (answer?.status === "fulfilled") lastByProject.set(p.id, answer.value);
        return lastByProject.get(p.id) ?? [];
      });
      return sceneOf(states, routines);
    };
  }

  $effect(() => {
    if (!on) return;
    const wake = (): void => void unlockAudio();
    window.addEventListener("pointerdown", wake, { capture: true });
    window.addEventListener("keydown", wake, { capture: true });
    const stop = startSoundWatch({ look: looker(), play: playCue, inBackground: () => !document.hasFocus() });
    return () => {
      stop();
      closeAudio();
      window.removeEventListener("pointerdown", wake, { capture: true });
      window.removeEventListener("keydown", wake, { capture: true });
    };
  });
</script>
