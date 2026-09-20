<script lang="ts">
  /**
   * CP-K2-Design — WEGWERFANSICHT. Kein Produktivcode.
   *
   * Zweck: die Kartenform am Bild entscheiden statt sie zu beschreiben. Feste
   * Fixtures, keine Datenbank, keine Tauri-Befehle. Wird nach der Entscheidung
   * gelöscht; das Gewinnerbild wird in CP-K2a sauber neu gebaut.
   *
   * Umgesetzt sind die schon entschiedenen Regeln, damit sie im Bild geprüft
   * werden können: D1 Spalten stauchen bis Mindestbreite, dann scrollen (nie
   * verstecken) · D2 nur Labels tragen Farbe · D3 Spaltenkörper als Tönung
   * ohne Rahmen · D4 Kopf = Name + Zahl, Menü erst bei Hover/Fokus · Q14
   * Kartenvorderseite hängt an der Breite, nicht an einer Einstellung.
   */
  import type { ModuleContext } from "../core/types";

  // Die Wegwerfansicht liest nichts aus dem Kontext — sie zeigt nur Fixtures.
  let { ctx: _ctx }: { ctx: ModuleContext } = $props();

  type Shape = "auto" | "flach" | "kante" | "schwebend";
  type Scene = "normal" | "voll" | "leer";

  let shape = $state<Shape>("auto");
  let scene = $state<Scene>("normal");
  let boardWidth = $state(0);

  /** D1: bis hierher wird gestaucht, danach scrollt das Brett waagerecht. */
  const MIN_COL_PX = 140;

  // In CP-K2a werden daraus --ax-label-* Tokens (dann auch pro Thema
  // stimmbar). Hier bewusst lokal, damit die Wegwerfansicht nichts Geteiltes
  // anfasst.
  const LABEL_COLORS = ["#d08a5a", "#6f9bd1", "#79ad83", "#a884b8", "#c0a13f", "#5aa8a8"];

  function labelColor(text: string): string {
    let h = 0;
    for (let i = 0; i < text.length; i += 1) h = (h * 31 + text.charCodeAt(i)) >>> 0;
    return LABEL_COLORS[h % LABEL_COLORS.length];
  }

  interface Card {
    id: number;
    title: string;
    labels: string[];
    dueInDays?: number;
    assignee?: string;
    body?: string;
  }

  const BASE: { name: string; cards: Card[] }[] = [
    {
      name: "Offen",
      cards: [
        { id: 1, title: "Spaltenbreite auf 21:9 prüfen", labels: ["design"], dueInDays: 3 },
        { id: 2, title: "Vault-Spiegel: Dateiname festzurren", labels: ["kanban", "vault"] },
        {
          id: 3,
          title: "Migration 0008 gegen bestehende DB testen",
          labels: ["rust"],
          dueInDays: -2,
          body: "Auf leerer und auf gewachsener Datenbank, plus der Test in db/mod.rs.",
        },
      ],
    },
    {
      name: "In Arbeit",
      cards: [
        {
          id: 4,
          title: "Drag-and-Drop ohne HTML5-API",
          labels: ["frontend", "design"],
          assignee: "agent:claude-1",
          body: "canvas/drag.ts als Plumbing, Momentaufnahme bei Drag-Beginn.",
        },
        { id: 5, title: "Tastaturbedienung für Kartenumzug", labels: ["a11y"], dueInDays: 0 },
      ],
    },
    {
      name: "Fertig",
      cards: [
        { id: 6, title: "WAL und busy_timeout entschieden", labels: ["rust"] },
        { id: 7, title: "Zwei-Parteien-Regel als DB-Invariante", labels: ["rust", "agenten"] },
      ],
    },
  ];

  type Column = { name: string; cards: Card[] };

  const columns = $derived.by((): Column[] => {
    if (scene === "leer") return BASE.map((c) => ({ ...c, cards: [] }));
    if (scene === "voll") {
      const stacked: Card[] = Array.from({ length: 40 }, (_, n) => ({
        id: 100 + n,
        title: `Aufgestaute Aufgabe ${n + 1}`,
        labels: n % 3 === 0 ? ["kanban"] : [],
      }));
      return BASE.map((c, i) => (i === 0 ? { ...c, cards: stacked } : c));
    }
    return BASE;
  });

  const boardEmpty = $derived(columns.every((c) => c.cards.length === 0));

  function dueLabel(days: number): string {
    if (days === 0) return "heute";
    if (days < 0) return `${Math.abs(days)} T überfällig`;
    return `in ${days} T`;
  }

  const SHAPES: Shape[] = ["auto", "flach", "kante", "schwebend"];
  const SCENES: { id: Scene; label: string }[] = [
    { id: "normal", label: "normal" },
    { id: "voll", label: "volle Spalte" },
    { id: "leer", label: "leeres Brett" },
  ];
</script>

<div class="preview">
  <div class="bar">
    <span class="group">
      {#each SHAPES as s}
        <button class:on={shape === s} onclick={() => (shape = s)}>{s}</button>
      {/each}
    </span>
    <span class="group">
      {#each SCENES as s}
        <button class:on={scene === s.id} onclick={() => (scene = s.id)}>{s.label}</button>
      {/each}
    </span>
    <span class="meta">Brett {boardWidth}px · Spalte ab 200px weit</span>
  </div>

  <div class="board {shape}" bind:clientWidth={boardWidth} style="--min-col: {MIN_COL_PX}px">
    {#each columns as col (col.name)}
      <section class="col">
        <header>
          <span class="name">{col.name}</span>
          <span class="count">{col.cards.length}</span>
          <button class="menu" aria-label="Spaltenmenü" tabindex="0">⋯</button>
        </header>
        <div class="cards">
          {#each col.cards as card (card.id)}
            <article class="card">
              <p class="title">{card.title}</p>

              {#if card.body}
                <p class="body">{card.body}</p>
              {/if}

              {#if card.labels.length > 0 || card.dueInDays !== undefined}
                <div class="foot">
                  <!-- Beide Formen stehen im DOM; welche sichtbar ist,
                       entscheidet die Container-Abfrage auf der Spalte. -->
                  <span class="dots">
                    {#each card.labels as l (l)}
                      <span class="dot" style="background: {labelColor(l)}"></span>
                    {/each}
                  </span>
                  <span class="chips">
                    {#each card.labels as l (l)}
                      <span class="chip" style="border-color: {labelColor(l)}; color: {labelColor(l)}">
                        {l}
                      </span>
                    {/each}
                  </span>

                  {#if card.dueInDays !== undefined}
                    <span class="due" class:over={card.dueInDays < 0}>{dueLabel(card.dueInDays)}</span>
                  {/if}
                </div>
              {/if}

              <!-- Q15: nur sichtbar, wenn gesetzt und nicht human:owner -->
              {#if card.assignee}
                <p class="who">{card.assignee}</p>
              {/if}
            </article>
          {:else}
            <p class="empty">Keine Karten</p>
          {/each}
        </div>
        <button class="add">+ Karte</button>
      </section>
    {/each}
  </div>

  {#if boardEmpty}
    <p class="board-empty">Noch nichts hier — leg die erste Karte an.</p>
  {/if}
</div>

<style>
  .preview {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    gap: var(--ax-space-2);
    padding: var(--ax-space-2);
    font-family: var(--ax-font-sans);
    color: var(--ax-text);
  }

  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--ax-space-2);
    font-size: var(--ax-font-size-xs);
  }
  .group {
    display: inline-flex;
    gap: var(--ax-space-1);
  }
  .bar button {
    padding: 2px var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    background: transparent;
    color: var(--ax-text-muted);
    font: inherit;
    cursor: pointer;
  }
  .bar button.on {
    border-color: var(--ax-accent);
    color: var(--ax-accent);
    background: var(--ax-accent-muted);
  }
  .meta {
    margin-left: auto;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
  }

  /* D1: stauchen bis --min-col, danach waagerecht scrollen. Nie verstecken. */
  .board {
    display: flex;
    gap: var(--ax-space-2);
    flex: 1 1 auto;
    min-height: 0;
    overflow-x: auto;
    overflow-y: hidden;
  }
  .col {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1 1 0;
    min-width: var(--min-col);
    /* D3: Tönung als Körper, kein Rahmen — die Fläche ist das Ablageziel.
       --ax-bg als "Tischplatte", Karten als "Blatt darauf": die einzige
       Paarung, die in hellen UND dunklen Themen dieselbe Richtung behält.
       (surface-2/3 taugen nicht: paper kehrt den Verlauf um.) */
    background: var(--ax-bg);
    border-radius: var(--ax-radius-md);
    padding: var(--ax-space-2);
    container: col / inline-size;
  }

  header {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: 0 var(--ax-space-1) var(--ax-space-2);
  }
  .name {
    font-size: var(--ax-font-size-sm);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .count {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    font-family: var(--ax-font-mono);
  }
  /* D4: Menü erst bei Hover oder Tastaturfokus. */
  .menu {
    margin-left: auto;
    border: 0;
    background: transparent;
    color: var(--ax-text-muted);
    cursor: pointer;
    opacity: 0;
    transition: opacity var(--ax-dur-fast) var(--ax-ease);
  }
  .col:hover .menu,
  .menu:focus-visible {
    opacity: 1;
  }

  .cards {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    overflow-y: auto;
    min-height: 0;
    flex: 1 1 auto;
  }

  /* Die Karte fragt nur noch die Tokens — welche Behandlung dabei
     herauskommt, entscheidet das Thema (Vorgabe) oder die Übersteuerung
     unten. Keine Literale hier, Hausregel eingehalten. */
  .card {
    background: var(--ax-card-bg);
    border: 1px solid var(--ax-card-border);
    box-shadow: var(--ax-card-shadow);
    border-radius: var(--ax-radius-md);
    padding: var(--ax-space-2);
  }

  /* Übersteuerung: setzt dieselben Tokens lokal auf dem Brett. In dunklen
     Themen bleibt "schwebend" naturgemäss zurückhaltend — genau deshalb ist
     "auto" die Vorgabe. */
  .board.flach {
    --ax-card-border: transparent;
    --ax-card-shadow: none;
  }
  .board.kante {
    --ax-card-border: var(--ax-border-strong);
    --ax-card-shadow: none;
  }
  .board.schwebend {
    --ax-card-border: transparent;
    --ax-card-shadow: var(--ax-shadow-tile);
  }

  .title {
    margin: 0;
    font-size: var(--ax-font-size-sm);
    line-height: var(--ax-line-height);
  }
  .body {
    margin: var(--ax-space-1) 0 0;
    font-size: var(--ax-font-size-xs);
    color: var(--ax-text-muted);
    line-height: var(--ax-line-height);
  }

  .foot {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--ax-space-1) var(--ax-space-2);
    margin-top: var(--ax-space-2);
    font-size: var(--ax-font-size-xs);
  }
  .dots {
    display: none;
    gap: 3px;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: var(--ax-radius-pill);
  }
  .chips {
    display: inline-flex;
    gap: var(--ax-space-1);
    flex-wrap: wrap;
  }
  /* D2: Farbe tragen ausschliesslich die Labels. */
  .chip {
    padding: 0 6px;
    border: 1px solid;
    border-radius: var(--ax-radius-pill);
    line-height: 1.6;
  }
  .due {
    margin-left: auto;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
    white-space: nowrap;
  }
  .due.over {
    color: var(--ax-danger);
  }
  .who {
    margin: var(--ax-space-1) 0 0;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Q14 korrigiert: die Dichte haengt an der SPALTEN-, nicht an der
     Brettbreite — ein breites Brett mit acht Spalten hat schmale Spalten. */
  @container col (max-width: 200px) {
    .chips,
    .body,
    .who {
      display: none;
    }
    .dots {
      display: inline-flex;
    }
  }

  /* D4: leise Zeile statt leerer Fläche. */
  .empty {
    margin: var(--ax-space-2) var(--ax-space-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
  .add {
    margin-top: var(--ax-space-2);
    border: 0;
    background: transparent;
    color: var(--ax-text-muted);
    font: inherit;
    font-size: var(--ax-font-size-xs);
    text-align: left;
    padding: var(--ax-space-1);
    border-radius: var(--ax-radius-sm);
    cursor: pointer;
  }
  .add:hover {
    color: var(--ax-accent);
    background: var(--ax-accent-muted);
  }
  .board-empty {
    margin: 0;
    text-align: center;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
</style>
