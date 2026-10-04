<!--
  A button that is an icon (`docs/plans/editor-look.md`, K6, K11): at least
  `--ax-hit-min` square, whatever the icon size, so it is easy to hit on any
  display; quiet at rest, a soft fill on hover, the accent when `pressed`.
  `label` is required — it is the tooltip and what assistive technology reads.
  As a tab (`tab`, inside a `role="tablist"`) it reports `aria-selected` instead
  of `aria-pressed`.
-->
<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { IconName } from "./icons/lucide";

  interface Props {
    icon: IconName;
    label: string;
    /** A toggle that is on (a panel that is open) — or, as a tab, the selected one. */
    pressed?: boolean;
    /** One tab of a tab list: `role="tab"` and `aria-selected` from `pressed`. */
    tab?: boolean;
    disabled?: boolean;
    size?: "sm" | "md" | "lg";
    onclick?: (e: MouseEvent) => void;
  }

  let { icon, label, pressed, tab = false, disabled = false, size = "md", onclick }: Props = $props();
</script>

<button
  type="button"
  class="icon-button"
  class:pressed
  title={label}
  aria-label={label}
  role={tab ? "tab" : undefined}
  aria-selected={tab ? (pressed ?? false) : undefined}
  aria-pressed={tab ? undefined : pressed}
  {disabled}
  {onclick}
>
  <Icon name={icon} {size} />
</button>

<style>
  .icon-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: var(--ax-hit-min);
    min-height: var(--ax-hit-min);
    padding: 0;
    border: 0;
    border-radius: var(--ax-radius-md);
    background: transparent;
    color: var(--ax-text-muted);
    cursor: pointer;
    transition:
      background var(--ax-dur-fast) var(--ax-ease),
      color var(--ax-dur-fast) var(--ax-ease);
  }
  .icon-button:hover:not(:disabled) {
    background: var(--ax-surface-3);
    color: var(--ax-text);
  }
  .icon-button:focus-visible {
    outline: 2px solid var(--ax-focus-ring);
    outline-offset: 1px;
  }
  .icon-button.pressed {
    background: var(--ax-accent-muted);
    color: var(--ax-accent);
  }
  .icon-button:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
