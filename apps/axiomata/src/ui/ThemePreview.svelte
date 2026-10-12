<!--
  A theme in miniature (`docs/plans/ui-ux-round.md` UX7b, G21): a real piece of the app — the dock, a tile, a card at
  work with its glow and hex avatar — drawn in that theme's own colours by scoping `data-theme` to the scene (the theme
  files are scoped to `[data-theme]`, not `:root`, for exactly this). Nothing of the app is recoloured to show it.

  Only tokens every theme sets itself are used: the derived ones in `tokens.css` are worked out on `:root`, in the
  active theme. The glow is derived here again, at the lowest specificity, so a theme with a glow of its own keeps it.
-->
<script lang="ts">
  interface Props {
    /** The theme to show (`core/themes.ts` id). */
    theme: string;
  }

  let { theme }: Props = $props();
</script>

<span class="scene" data-theme={theme} aria-hidden="true">
  <span class="dock">
    <i class="logo ax-hex"></i>
    <i class="item"></i>
    <i class="item"></i>
    <i class="item short"></i>
  </span>
  <span class="tile">
    <i class="title"></i>
    <i class="line"></i>
    <i class="line short"></i>
    <i class="chip"></i>
  </span>
  <span class="card">
    <i class="avatar ax-hex"></i>
    <i class="line"></i>
    <i class="line short"></i>
  </span>
</span>

<style>
  /* The theme's glow, or its accent where it has none; zero specificity, so a theme's own `--ax-glow` wins. */
  :where(.scene) {
    --ax-glow: var(--ax-accent);
  }

  .scene {
    position: relative;
    display: block;
    width: 100%;
    aspect-ratio: 16 / 9;
    overflow: hidden;
    border-radius: var(--ax-radius-sm);
    background:
      radial-gradient(circle at 75% 70%, color-mix(in srgb, var(--ax-glow) 10%, transparent), transparent 55%),
      var(--ax-bg);
    box-shadow: inset 0 0 0 1px var(--ax-border);
  }

  .scene i {
    position: absolute;
    display: block;
  }

  /* The dock: a pill with the hex logo glowing and a few targets, in one row. */
  .dock {
    position: absolute;
    top: 7%;
    left: 22%;
    right: 22%;
    height: 13%;
    display: flex;
    align-items: center;
    gap: 5%;
    padding-left: 5%;
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-1);
    box-shadow: inset 0 0 0 1px var(--ax-border);
  }

  .scene .dock i {
    position: static;
    flex: none;
  }

  .dock .logo {
    width: 7%;
    height: 56%;
    margin-right: 6%;
    background: var(--ax-glow);
  }

  .dock .item {
    width: 16%;
    height: 22%;
    border-radius: var(--ax-radius-pill);
    background: color-mix(in srgb, var(--ax-text-muted) 55%, transparent);
  }

  .dock .item.short {
    width: 9%;
  }

  .tile,
  .card {
    position: absolute;
    border-radius: 8%;
  }

  /* A tile: the surface the dashboard's modules lie on, with its title and a few lines. */
  .tile {
    left: 7%;
    top: 30%;
    width: 44%;
    height: 58%;
    border-radius: 10% / 14%;
    background: var(--ax-surface-2);
    box-shadow: inset 0 0 0 1px var(--ax-border);
  }

  .tile .title {
    top: 14%;
    left: 10%;
    width: 46%;
    height: 9%;
    border-radius: var(--ax-radius-pill);
    background: var(--ax-text);
  }

  .line {
    left: 10%;
    width: 76%;
    height: 7%;
    border-radius: var(--ax-radius-pill);
    background: color-mix(in srgb, var(--ax-text-muted) 45%, transparent);
  }

  .tile .line {
    top: 38%;
  }

  .tile .line.short {
    top: 54%;
    width: 50%;
  }

  .tile .chip {
    left: 10%;
    bottom: 12%;
    width: 30%;
    height: 12%;
    border-radius: var(--ax-radius-pill);
    background: color-mix(in srgb, var(--ax-accent) 28%, transparent);
  }

  /* A card at work: the glow round it, the agent's hexagon lit — the one place the scene says "alive". */
  .card {
    right: 7%;
    top: 36%;
    width: 36%;
    height: 40%;
    border-radius: 10% / 16%;
    background: var(--ax-surface-3);
    box-shadow:
      inset 0 0 0 1px var(--ax-glow),
      0 0 calc(10px * var(--ax-ui-scale)) color-mix(in srgb, var(--ax-glow) 45%, transparent);
  }

  .card .avatar {
    top: 18%;
    left: 9%;
    width: 16%;
    height: 32%;
    background: var(--ax-glow);
  }

  .card .line {
    left: 32%;
    width: 56%;
    top: 26%;
  }

  .card .line.short {
    top: 58%;
    left: 9%;
    width: 60%;
  }
</style>
