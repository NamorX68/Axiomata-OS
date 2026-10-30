<!--
  The Studio's models and roles (`docs/plans/agents-model.md`, AM3, Q20; before it, engines and roles —
  `docs/plans/studio-roster-ui.md`, R-1): one large pane in the dock with two tabs. Models: which models are on, their
  tiers, the default engine per tier, the price limits and the owner's own engines. Roles: what an agent is for. They live with the Studio, not in the app's general settings, so they move along when
  the Studio becomes a program of its own (A11); the data behind them (`axiomata-roster`) already takes its paths from
  outside.
  Both sections stay mounted while the other is shown, so a half-filled form survives a look at the other list.
-->
<script lang="ts">
  import ModelsSection from "../ModelsSection.svelte";
  import RolesSection from "../RolesSection.svelte";
  import { tAgents as t } from "../../i18n/agents";

  type Part = "models" | "roles";

  let part = $state<Part>("models");
  /** Ids of the tabs and their panels; unique, as each layout may hold its own pane. */
  const uid = crypto.randomUUID();
</script>

<div class="roster">
  <header>
    <div class="switch" role="tablist" aria-label={t("modelsOrRoles")}>
      <button
        type="button"
        role="tab"
        id="{uid}-tab-models"
        aria-controls="{uid}-panel-models"
        class:active={part === "models"}
        aria-selected={part === "models"}
        onclick={() => (part = "models")}
      >
        {t("models")}
      </button>
      <button
        type="button"
        role="tab"
        id="{uid}-tab-roles"
        aria-controls="{uid}-panel-roles"
        class:active={part === "roles"}
        aria-selected={part === "roles"}
        onclick={() => (part = "roles")}
      >
        {t("roles")}
      </button>
    </div>
    <p class="chain">
      <strong>{t("chainModel")}</strong> {t("chainModelText")}
      <strong>{t("chainRole")}</strong> {t("chainRoleText")}
      <strong>{t("chainAgent")}</strong> {t("chainAgentText")}
    </p>
  </header>
  <div class="part" class:off={part !== "models"} role="tabpanel" id="{uid}-panel-models" aria-labelledby="{uid}-tab-models">
    <ModelsSection />
  </div>
  <div class="part" class:off={part !== "roles"} role="tabpanel" id="{uid}-panel-roles" aria-labelledby="{uid}-tab-roles">
    <RolesSection />
  </div>
</div>

<style>
  .roster {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    background: var(--ax-surface-1);
    /* The sections ask how wide the pane is, not the window: the dock may give it a narrow column. */
    container-type: inline-size;
  }

  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--ax-space-2) var(--ax-space-4);
    padding: var(--ax-space-2) var(--ax-space-4);
    border-bottom: 1px solid var(--ax-border);
    flex: 0 0 auto;
  }

  .switch {
    display: flex;
    flex: 0 0 auto;
    gap: var(--ax-space-1);
    padding: calc(2px * var(--ax-ui-scale));
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
  }

  .switch button {
    padding: var(--ax-space-1) var(--ax-space-4);
    background: none;
    border: 0;
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    cursor: pointer;
  }

  .switch button:hover {
    color: var(--ax-text);
  }

  /* Selected ≠ alive (accent rule): the open tab is a selection, not orange. */
  .switch button.active {
    background: var(--ax-selected-bg);
    color: var(--ax-text);
  }

  .chain {
    flex: 1 1 calc(260px * var(--ax-ui-scale));
    margin: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    line-height: var(--ax-line-height);
  }

  .chain strong {
    color: var(--ax-text);
    font-weight: 600;
  }

  .part {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
  }

  .part.off {
    display: none;
  }
</style>
