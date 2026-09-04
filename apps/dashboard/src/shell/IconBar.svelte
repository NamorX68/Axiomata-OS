<!--
  Icon buttons directly under the top bar: Settings, Add module, New note,
  Search, Theme. Settings / Add / New note / Search only fire a shell-bus
  event — their dialogs land elsewhere (steps 7, 11, 13; New note in
  `App.svelte`'s `NewNotePanel`). Theme cycles the built-in themes.
-->
<script lang="ts">
  import { emit } from "../core/bus";
  import { activeTheme } from "../core/stores";
  import { THEMES, applyTheme, nextTheme } from "../core/themes";

  interface IconButton {
    id: string;
    label: string;
    /** Inline SVG path data on a 24×24 grid, stroked with currentColor. */
    path: string;
    onClick: () => void;
  }

  const currentLabel = $derived(THEMES.find((t) => t.id === $activeTheme)?.label ?? $activeTheme);

  const buttons: IconButton[] = [
    {
      id: "settings",
      label: "Settings",
      path: "M12 15.5a3.5 3.5 0 100-7 3.5 3.5 0 000 7z M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 01-2.83 2.83l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09a1.65 1.65 0 00-1-1.51 1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06A1.65 1.65 0 004.6 15a1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09a1.65 1.65 0 001.51-1 1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06a1.65 1.65 0 001.82.33H9a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06a1.65 1.65 0 00-.33 1.82V9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z",
      onClick: () => emit("shell:settings"),
    },
    {
      id: "add",
      label: "Add module",
      path: "M4 4h6v6H4z M14 4h6v6h-6z M4 14h6v6H4z M17 14v6 M14 17h6",
      onClick: () => emit("shell:add-module"),
    },
    {
      id: "new-note",
      label: "New note",
      path: "M6 3h8l5 5v12a1 1 0 01-1 1H6a1 1 0 01-1-1V4a1 1 0 011-1z M14 3v5h5 M9 15h6 M12 12v6",
      onClick: () => emit("shell:new-note"),
    },
    {
      id: "search",
      label: "Search",
      path: "M11 4a7 7 0 100 14 7 7 0 000-14z M20 20l-4-4",
      onClick: () => emit("shell:search"),
    },
    {
      id: "theme",
      label: "Theme",
      path: "M12 3a9 9 0 000 18c.9 0 1.5-.7 1.5-1.5 0-.4-.2-.8-.4-1.1-.3-.3-.4-.7-.4-1.1 0-.9.7-1.5 1.5-1.5H16a5 5 0 005-5c0-4.4-4-7.8-9-7.8z M7.5 11a1 1 0 100-2 1 1 0 000 2z M10.5 7.5a1 1 0 100-2 1 1 0 000 2z M14.5 7.5a1 1 0 100-2 1 1 0 000 2z M17.5 11a1 1 0 100-2 1 1 0 000 2z",
      onClick: () => applyTheme(nextTheme($activeTheme)),
    },
  ];
</script>

<nav class="iconbar" aria-label="Dashboard actions">
  {#each buttons as b (b.id)}
    <button
      type="button"
      class="icon"
      title={b.id === "theme" ? `Theme: ${currentLabel}` : b.label}
      aria-label={b.label}
      onclick={b.onClick}
    >
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path
          d={b.path}
          fill="none"
          stroke="currentColor"
          stroke-width="1.6"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
    </button>
  {/each}
</nav>

<style>
  .iconbar {
    display: flex;
    justify-content: center;
    gap: var(--ax-space-3);
    flex: 0 0 auto;
    padding-bottom: var(--ax-space-3);
  }

  .icon {
    width: 34px;
    height: 34px;
    padding: 0;
    display: grid;
    place-items: center;
    background: transparent;
    border-color: transparent;
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text-muted);
  }

  .icon:hover:not(:disabled) {
    color: var(--ax-text);
    background: var(--ax-surface-2);
    border-color: var(--ax-border);
  }

  .icon svg {
    width: 18px;
    height: 18px;
  }
</style>
