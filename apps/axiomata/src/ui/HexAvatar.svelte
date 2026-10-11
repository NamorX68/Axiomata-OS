<!--
  An agent's avatar as a hexagon (`docs/plans/ui-ux-round.md` U1/U5): two letters ("CC", "OC") on a face inside a
  ring. The ring takes the harness's or the project's colour when given one (G19), and the glow while the agent works
  — then it breathes and its letters light up, the one place on an agent's card that says "alive".
-->
<script lang="ts">
  interface Props {
    /** One or two letters — the nicknames the owner uses. */
    text: string;
    /** The agent works: glow and breath. */
    live?: boolean;
    /** A CSS colour for the ring at rest (a harness's, a project's); the hairline without it. */
    ring?: string;
    size?: "sm" | "md" | "lg";
    /** Read out by assistive technology and shown as tooltip; without it the avatar is decorative. */
    label?: string;
    title?: string;
  }

  let { text, live = false, ring, size = "md", label, title }: Props = $props();
</script>

<span
  class="avatar {size}"
  class:live
  style:--ring={ring}
  role={label ? "img" : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : "true"}
  title={title ?? label}
>
  <span class="ring"></span>
  <span class="face">{text}</span>
</span>

<style>
  .avatar {
    position: relative;
    display: inline-grid;
    place-items: center;
    flex: none;
  }

  .sm {
    width: calc(16px * var(--ax-ui-scale));
    height: calc(18px * var(--ax-ui-scale));
    font-size: calc(7px * var(--ax-ui-scale));
  }

  .md {
    width: calc(28px * var(--ax-ui-scale));
    height: calc(32px * var(--ax-ui-scale));
    font-size: calc(10px * var(--ax-ui-scale));
  }

  .lg {
    width: calc(40px * var(--ax-ui-scale));
    height: calc(46px * var(--ax-ui-scale));
    font-size: calc(13px * var(--ax-ui-scale));
  }

  .ring,
  .face {
    position: absolute;
    clip-path: var(--ax-hex-clip);
  }

  .ring {
    inset: 0;
    background: var(--ring, var(--ax-border-strong));
  }

  .face {
    inset: calc(1.5px * var(--ax-ui-scale));
    display: grid;
    place-items: center;
    background: var(--ax-elev-3);
    color: color-mix(in srgb, var(--ring, var(--ax-text-muted)) 70%, var(--ax-text));
    font-family: var(--ax-font-sans);
    font-weight: 600;
    letter-spacing: 0.02em;
    line-height: 1;
  }

  .sm .face {
    inset: calc(1px * var(--ax-ui-scale));
  }

  .live {
    filter: var(--ax-glow-drop);
  }

  .live .ring {
    background: var(--ax-glow);
    animation: var(--ax-anim-breathe);
  }

  .live .face {
    color: var(--ax-glow);
  }
</style>
