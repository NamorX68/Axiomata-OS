<!--
  MiniCalendar — the small month grid top-left in the `calendar` module.
  Pure presentation: the parent owns `selected` / the displayed month and
  the event data; this only renders the 6×7 grid (`core/monthGrid.ts`) and
  reports clicks back via `onSelect` / `onPage`.

  - today: round `--ax-accent` underlay
  - selected: `--ax-accent` ring
  - a day with an event: a small dot
  - adjacent-month filler days: muted, still clickable (selecting one pages
    the month, which the parent handles)
-->
<script lang="ts">
  import { monthDiff, monthGrid, monthOf, weekdayLabels, type YearMonth } from "../core/monthGrid";

  let {
    month,
    selected,
    today,
    eventDays,
    minMonth,
    maxMonth,
    onSelect,
    onPage,
  }: {
    month: YearMonth;
    /** `YYYY-MM-DD` of the selected day. */
    selected: string;
    /** `YYYY-MM-DD` of the local "today". */
    today: string;
    /** Days (`YYYY-MM-DD`) that have at least one event. */
    eventDays: Set<string>;
    /** Earliest / latest month the caller has data for. When set, the nav
     *  buttons stop at these bounds and cells outside them are disabled —
     *  the calendar module only fetches this month + next. */
    minMonth?: YearMonth;
    maxMonth?: YearMonth;
    onSelect: (iso: string) => void;
    /** `delta` is -1 (previous month) or +1 (next). */
    onPage: (delta: number) => void;
  } = $props();

  const grid = $derived(monthGrid(month.year, month.month, today));
  const weekdays = weekdayLabels("short");

  const canPrev = $derived(!minMonth || monthDiff(minMonth, month) > 0);
  const canNext = $derived(!maxMonth || monthDiff(month, maxMonth) > 0);
  const outOfRange = (iso: string) => {
    const m = monthOf(iso);
    return (!!minMonth && monthDiff(minMonth, m) < 0) || (!!maxMonth && monthDiff(m, maxMonth) < 0);
  };
</script>

<div class="mini">
  <div class="bar">
    <button type="button" class="nav" aria-label="Previous month" disabled={!canPrev} onclick={() => onPage(-1)}>‹</button>
    <span class="label">{grid.label}</span>
    <button type="button" class="nav" aria-label="Next month" disabled={!canNext} onclick={() => onPage(1)}>›</button>
  </div>

  <div class="grid" role="grid" aria-label="Month">
    {#each weekdays as wd (wd)}
      <span class="wd" aria-hidden="true">{wd}</span>
    {/each}
    {#each grid.weeks as week (week[0].iso)}
      {#each week as cell (cell.iso)}
        {@const blocked = outOfRange(cell.iso)}
        <button
          type="button"
          class="day"
          class:out={!cell.inMonth}
          class:today={cell.isToday}
          class:selected={cell.iso === selected}
          aria-pressed={cell.iso === selected}
          aria-label={cell.iso}
          disabled={blocked}
          onclick={() => onSelect(cell.iso)}
        >
          <span class="num">{Number(cell.iso.slice(8, 10))}</span>
          {#if eventDays.has(cell.iso)}<span class="dot" aria-hidden="true"></span>{/if}
        </button>
      {/each}
    {/each}
  </div>
</div>

<style>
  .mini {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    flex: 0 0 auto;
    user-select: none;
  }

  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ax-space-1);
  }
  .label {
    font-size: var(--ax-font-size-sm);
    font-weight: 600;
    text-transform: capitalize;
  }
  .nav {
    padding: 0 var(--ax-space-2);
    line-height: 1.4;
    color: var(--ax-text-muted);
  }
  .nav:hover:not(:disabled) {
    color: var(--ax-text);
  }
  .nav:disabled {
    opacity: 0.3;
    cursor: default;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 1px;
  }
  .wd {
    text-align: center;
    font-size: var(--ax-font-size-base);
    color: var(--ax-text-muted);
    padding-bottom: var(--ax-space-1);
  }

  .day {
    position: relative;
    aspect-ratio: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: none;
    background: transparent;
    border-radius: var(--ax-radius-pill);
    font-size: var(--ax-font-size-xs);
    color: var(--ax-text);
    cursor: pointer;
  }
  .day:hover:not(:disabled) {
    background: var(--ax-surface-2);
  }
  .day.out {
    color: var(--ax-text-muted);
    opacity: 0.55;
  }
  .day:disabled {
    color: var(--ax-text-muted);
    opacity: 0.25;
    cursor: default;
  }
  .day.today {
    background: var(--ax-accent);
    color: var(--ax-text-invert);
    font-weight: 700;
  }
  .day.selected {
    box-shadow: inset 0 0 0 2px var(--ax-accent);
  }
  .day.today.selected {
    box-shadow: inset 0 0 0 2px var(--ax-text-invert);
  }
  .day:focus-visible {
    outline: 2px solid var(--ax-focus-ring);
    outline-offset: -2px;
  }

  .num {
    line-height: 1;
  }
  .dot {
    position: absolute;
    bottom: 2px;
    width: 3px;
    height: 3px;
    border-radius: var(--ax-radius-pill);
    background: var(--ax-accent);
  }
  .day.today .dot {
    background: var(--ax-text-invert);
  }
</style>
