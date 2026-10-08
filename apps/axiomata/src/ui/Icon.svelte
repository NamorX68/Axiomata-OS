<!--
  One UI icon (`docs/plans/editor-look.md`, K6): a Lucide glyph by name, drawn
  in the text colour around it, at one of the icon-size tokens — so it follows
  the theme and the UI scale. Decorative unless it gets a `label`.

  The drawing comes from `icons/lucide.ts`, vendored by `scripts/vendor-icons.sh`
  (static, checked in, checked for scripts and handlers when generated), which
  is why it may go in with `{@html}`.
-->
<script lang="ts">
  import { LUCIDE, type IconName } from "./icons/lucide";

  interface Props {
    name: IconName;
    size?: "sm" | "md" | "lg" | "xl";
    /** Read out by assistive technology; without it the icon is decorative. */
    label?: string;
  }

  let { name, size = "md", label }: Props = $props();
</script>

<svg
  class="icon {size}"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-linecap="round"
  stroke-linejoin="round"
  role={label ? "img" : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : "true"}
>
  {@html LUCIDE[name]}
</svg>

<style>
  .icon {
    flex: none;
    stroke-width: var(--ax-icon-stroke);
    vertical-align: middle;
  }
  .sm {
    width: var(--ax-icon-sm);
    height: var(--ax-icon-sm);
  }
  .md {
    width: var(--ax-icon-md);
    height: var(--ax-icon-md);
  }
  .lg {
    width: var(--ax-icon-lg);
    height: var(--ax-icon-lg);
  }
  .xl {
    width: var(--ax-icon-xl);
    height: var(--ax-icon-xl);
  }
</style>
