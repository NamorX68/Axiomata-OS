# Plan: JEV-Einsatz (TypeSafe AI System-One-Modell)

Status (2026-09-21): **PLAN — keine Umsetzung.** Reine Bewertung (Für/Wider)
plus Umsetzungsfahrplan für den Einsatz von Jev, TypeSafe AIs
System-One-Evaluierungsmodell, in Mail-Triage und Cleanup-Skill sowie weiteren
Stellen. Owner-Vorgabe: erst dieser Plan, Umsetzung nur auf explizite
Freigabe — beginnend mit Phase 0 (Zugang & Spike), deren Go/No-Go über alles
Weitere entscheidet.

Update (2026-09-21, abends): Zugang via **OpenRouter** bestätigt
(`typesafe/jev-1.13`, keine Waitlist nötig) + Open-Source-Recherche (§6 neu).
OpenRouter ist in `~/.config/opencode/opencode.json` und
`~/.axiomata/config.toml` bereits referenziert — voraussichtlich kein neues
Secret nötig (in Phase 0 verifizieren).

## 1. Was Jev ist (Recherche, Stand 2026-09-21)

- **TypeSafe AIs „System One"-Evaluierungsmodell**, Launch 15.09.2026,
  **Early Access (Waitlist)**. Kein generatives LLM: es erzeugt **keinen
  Text**, sondern beantwortet **typisierte Fragen** zu einem `state`
  (Text/JSON) mit kalibrierten Wahrscheinlichkeiten (Training: RLCD —
  Reinforcement Learning for Calibrated Decisions).
- **Drei Frage-Primitives:** `Noul` (Ja/Nein-Wahrscheinlichkeit 0–1),
  `Choice` (eine von bis zu 255 Optionen + Verteilung + Confidence 0–1),
  `Score` (Position auf einer selbst definierten 2–10-stufigen Skala,
  Antwort = wahrscheinlichkeitsgewichteter Mittelwert + Verteilung +
  Confidence).
- **Alle Fragen eines Requests werden parallel beantwortet** (ca. 70–500 ms,
  $0.042/1M Input-Token, Output gratis). Ein Endpoint:
  `POST https://api.typesafe.ai/v1/systemone`, Modell `jev-latest` (löst
  aktuell zu `jev-1.13.0` auf — **Version pinnen, nicht Alias**, weil
  Confidence-Thresholds gegen genau eine Version getunt werden und TypeSafe
  pro Version eine Schwächenliste („jaggedness") publiziert).
- **Zwei recherchierte Warnungen, die diesen Plan direkt prägen:**
  1. Publizierte Schwächen von v1.13.0 u. a.: **Datumsvergleiche**,
     wörtliches Lesen, Zählen/Rechnen → **Datumslogik darf nie an Jev
     delegiert werden**, sie bleibt im Code (§4, §6).
  2. „Cannot hallucinate" heißt nur: keine Schema-Verletzung. **Falsche,
     aber valide Antworten bleiben möglich** → Confidence-Threshold +
     Fallback aufs bisherige Agent-Urteil ist Pflicht, kein Nice-to-have.
- Zugangspfade: **OpenRouter (empfohlen)** — `typesafe/jev-1.13` (gepinnt)
  bzw. `~typesafe/jev-latest` (Alias), **keine Waitlist**, gleiche Preise
  ($0.042/1M in, Output gratis), aber **eigener Alpha-Endpoint**
  `POST https://openrouter.ai/api/alpha/decisions` (nicht Chat-Completions),
  Auth per `OPENROUTER_API_KEY`, Credits nötig (kein Free Tier), 32k Kontext,
  Antwort = gleiche `answers` plus `id` (`gen-dec-*`), `model` (datierte
  Version), `provider`, `usage` mit Kosten. Wichtig: Namensform
  `typesafe/jev-1.13` gilt nur auf OpenRouter (TypeSafe direkt: `jev-1.13.0`).
  Alternative: TypeSafe direkt (Waitlist + `TYPESAFE_API_KEY`, 64k) oder
  Gateways (`typesafe-ai/jev` Vercel, `typesafe/jev` Cloudflare). SDKs:
  offiziell Python/TS, OpenRouter-SDKs mit `Alpha.Decisions`-Sektion,
  LangChain via `langchain-typesafe` (`TypeSafeClassifier`),
  Community-Clients (u. a. Go: `anilsenay/jev` als Vorlage für typisierte
  Clients).

## 2. Architektur-Befund (verifiziert im Repo, 2026-09-21)

Der Owner-Vorschlag („eine Funktion ruft den MCP auf, gibt Mails an Jev,
gibt nur Wichtige zurück, Agent fasst zusammen") ist richtig gedacht, aber
die Funktion kann nicht dort leben, wo man sie zuerst vermutet:

- **`axiomata-core` ruft kein MCP auf — nur der opencode-Agent tut das.**
  Einziger MCP-Konsument ist der gespawnte `opencode run`-Child
  (`crates/axiomata-core/src/agents/opencode.rs:246-354`); Core selbst ist
  kein MCP-Client und hat nicht einmal einen HTTPS-Client im Workspace
  (`Cargo.toml:29-32` sieht `rustls` ausdrücklich erst für den Fall vor,
  „if a remote HTTPS ever needed").
- `allowed_tools` im `SKILL.md`-Frontmatter ist **rein dokumentarisch** und
  wird von keinem Backend enforced
  (`agents/mod.rs:490-493`, `opencode.rs:217-220`); reale Gates sind `--auto`
  bzw. Prompt-Disziplin.
- Secrets-Konvention: Plaintext in `~/.axiomata/config.toml` (0600)
  (`config.rs:144-171,574-634`); opencode-MCP-Server lesen dagegen ihren
  eigenen Store (`~/.config/opencode/opencode.json`).
- Verbraucher-Verträge, die stabil bleiben müssen: `mail-digest` emittiert
  exakt ein JSON-Objekt `{"emails": [{id, sender, subject, date, reason,
  topic, summary}]}` (`resources/mail-digest/SKILL.md:76-78`), das
  `parseMailDigest`/`mail.svelte` (`apps/dashboard/src/modules/`) parsen und
  nach `reason === "important"|"topic"` filtern. **Nur additiv erweitern**
  (z. B. optionales `confidence`-Feld), Parser-Toleranz vorab verifizieren.
- `cleanup` läuft auf Backend `claude-code` (löst zu `Opencode` auf,
  `agents/mod.rs:262-289`), rechnet den Cutoff per `date -v-3d` im Prompt
  und löscht per `Bash(rm:*)` (`resources/cleanup/SKILL.md`).

**Konsequenz:** „Die Funktion" gehört in V1 in die **opencode-Schicht**
(neues MCP-Tool, z. B. Server `jev` mit `triage_mail`/`check_cleanup`, Key
in `opencode.json`), das der Skill-Agent wie jede andere Funktion aufruft —
**null Rust-Änderungen**. Ein natives `axiomata-core::jev`-Modul
(reqwest+rustls, Key in `config.toml`, CLI-/Tauri-Kommandos) ist eine valide
V2, erst wenn V1 Mehrwert beweist.

## 3. Mail-Triage — Für/Wider

**Heute:** `mail-digest` baut den Kandidatenpool via apple-mail-MCP (Inbox,
needs-response, je eine Topic-Suche; Budget ~6 Calls),
**klassifiziert per Agent-Judgement** (`SKILL.md` Schritt 3: „by judgement,
not string matching"), cappt bei 12 Mails.

**Mit Jev:** Pool-Bau wie bisher → **ein** `triage_mail`-Call mit allen
Kandidaten als States + gebündelten Fragen in einem Request (parallel,
nahezu keine Mehrkosten): `Noul` für die Important-Definition aus dem Skill
(Antwortbedarf/Frage/Deadline/Rechnung/Einladung; Newsletter ausgeschlossen),
`Choice` für das Topic aus `Mail/.topics.md` (+ Option `none`), optional
`Score` Wichtigkeit 1–5 für Sortierung statt harter 12er-Cap. Filter per
Confidence-Threshold (z. B. ≥ 0.8 auto, darunter Fallback aufs bisherige
Agent-Urteil); zurück kommen nur Relevante inkl. gekürztem Body → Agent
fasst zusammen.

- **Dafür:** ~6 langsame Mail.app-Roundtrips + teure LLM-Judgement-Calls
  werden zu einem Jev-Call (ms-Bereich, Bruchteile von Cent);
  deterministische Klassen statt Prompt-Stimmung; parallele Fragen kosten
  kaum extra. Exakt Jevs Kernkompetenz (ein Drittanbieter-Beispiel beschreibt
  wörtlich diesen Fall: Jev triagiert, LLM fasst zusammen).
- **Dagegen:** Topic-Erkennung „by judgement" (Fotografie ↔ Kamera-Ausrüstung
  ohne Wortmatch) ist semantisch anspruchsvoll — ob Jev das auf echten
  Postfächern so gut kann wie das Frontier-Modell, ist **unbewiesen**
  (Vendor-Evals sind selbst-referenziert, keine unabhängige Reproduktion).
  → A/B gegen bestehende Runs vor Umstellung (Phase 1).

## 4. Cleanup-Gate — Für/Wider

**Heute:** Cutoff- und Löschlogik (`file_date < cutoff` als String-Vergleich,
`## Done`-Zeilen mit `(done: …)`-Stempel) stehen deterministisch im Prompt.

**Mit Jev, zwingend zweistufig** (wegen §1, Warnung 1):

1. **Code/Prompt berechnet deterministische Fakten** (Pattern-Match,
   Datumsvergleich — exakt wie heute, unverändert).
2. **Jev-`Noul`-Gate nur für die Urteilsfrage:** „Erfüllt dieser Kandidat
   (mit vorberechneten Fakten) die Löschregel?" — fängt die Fälle, wo reine
   String-Logik blind ist (unbekannte Dateiformate, undatierte Done-Einträge,
   `Mail/.topics.md`-Verwechslungen). Löschen nur ≥ Threshold; darunter →
   behalten + im Report als `unsicher` listen.

- **Dafür:** Die gefährlichste Operation im System (`rm` per Agent) bekommt
  einen kalibrierten Zweitprüfer mit messbarer Confidence — das dokumentierte
  „AutoModeMiddleware"-Muster (Jev prüft Tool-Calls vor Ausführung).
- **Dagegen:** Bei rein datumsbasierten Regeln (Job 1+2 heute) ist der
  Mehrwert **gering** — die String-Logik ist bereits deterministisch und
  korrekt. Jev lohnt hier erst, sobald Regeln semantisch werden („lösche,
  was erkennbar obsolet ist") oder neue Jobs mit Ermessensanteil dazukommen.

## 5. Weitere Einsatzideen (priorisiert)

1. **Backend-Routing** (stärkster Hebel nach Mail): `Choice`-Frage „welches
   Backend für diesen Task?" — billig/lokal (Ollama) vs. fähig
   (Opencode-Modell) pro Skill-/Routine-Run. Kosteninfrastruktur existiert
   bereits (`spend::role_spend_summaries`, Spend-Caps); Jev wäre der fehlende
   Router davor. Entspricht dem dokumentierten „Model-routing
   middleware"-Pattern.
2. **Risk-Gate vor destruktiven Aktionen** (Board-Delete, Routine-Delete,
   `cleanup`-rm): `Noul` „entspricht diese Aktion der genannten Absicht?"
   als Code-enforced Block unter Threshold.
3. **Score statt binär im Digest:** Wichtigkeits-Score fürs Mail-Dashboard
   (Sortierung/Filter statt starrer Cap), später übertragbar auf
   calendar-/reminders-digest (`Noul` „braucht jetzt Aufmerksamkeit?").
4. **Routine-Dedup:** Beim Anlegen `Choice` „Duplikat welcher bestehenden
   Routine?" — billig, klarer Vertrag.
5. **Import-Klassifikation:** Neue Notizen per `Choice` auf Areas mappen
   (stützt den Obsidian-Importer, `import_obsidian` in
   `crates/axiomata-cli/src/main.rs:398-479`).
6. **Eval-Harness:** Per Gateway-Adapter Jev gegen das aktuelle LLM-Urteil
   auf gelabelten eigenen Mails antreten lassen, bevor irgendwas umgestellt
   wird.

## 6. Open-Source-Alternativen (Recherche 2026-09-21)

Fazit vorweg: **Kein offenes Projekt ist Jev** (keine Weights, kein
Self-Hosting seitens TypeSafe) — aber die Lücke „typisierte Entscheidungen,
lokal und privat" ist aktiv besetzt. Für uns interessant aus genau einem
Grund: **Mail-Inhalte verlassen das Gerät nicht.**

| Projekt | Ansatz | Reife / Hinweis |
|---|---|---|
| OpenDecision (deepanwadhwa, Apache-2.0) | ModernBERT-large Zero-Shot-NLI (~400M), Choice/Noul/Score (+ Relation), Python/FastAPI, **`POST /v1/systemone`-kompatibel** → Backend-swappable mit Jev | v0.1.1, kleine Community (~31 Stars), CPU-lauffähig (ca. 200–460 ms/Frage CPU, ~35 ms T4); Confidence = Konzentration, **nicht** Korrektheit → Thresholds neu tunen |
| open-jev-deberta-v3-large (HuggingFace, kotoba-lang) | DeBERTa-v3-large, Jev-Form (bis 255 Optionen), ein Forward-Pass, 0 % Strukturfehler by construction, temperatur-kalibriert; gemessen auf Public-Gold-Labels (banking77, SST-5, BoolQ) | Unabhängige Reproduktion, Zahlen nicht mit TypeSafe vergleichbar; prüfungswürdig als lokaler Kandidat |
| OpenJevPro (PyPI, openjev.pro) | Open-Weight-LLMs (Qwen3, Gemma, gpt-oss …) + Constrained Decoding (vLLM/SGLang) + Kalibrierung + Abstention-Layer (`UNKNOWN`/`HUMAN_REVIEW`) | Benchmarks selbst berichtet (Parität mit Jev auf eigenem JevBench) → Skepsis; Edge-Tier (Qwen3-4B, < 35 ms) wäre lokal denkbar, Server-Tier braucht Datacenter-GPU |
| Apple-Silicon-/Consumer-GPU-Ökosystem | mini-jev (~8,5 GB, Apple Silicon — passt zu unserem macOS), jevmlx, NanoJev/SemIf (CUDA), localjev (Bun), Laya | Fragmentiert, Kleinst-Communities; nur bei Bedarf einzeln evaluieren |
| daf-jev (Zenodo) | Toolkit + CLI + Skill + **MCP-Server** für die Jev-API | Als Vorlage für unser V1-MCP-Tool prüfen statt neu bauen |

Bewertung für unsere Fälle:

- **Stärkstes Argument lokal: Datenschutz.** Mail-Triage mit Bodies über
  Drittanbieter-API vs. alles on-device — OpenDecisions Request-Form ist zur
  Jev-Form kompatibel, d. h. **ein Backend-Interface, zwei Implementierungen**
  (OpenRouter-Jev / lokal). Ein Wechsel bleibt ohne Prompt-Umbau möglich.
- **Genauigkeit:** Jev (Frontier-Niveau in Vendor-Evals) > Open-Weight-Pipelines
  (selbst berichtet) > kleine Zero-Shot-NLI (ModernBERT). Unsere härteste
  Frage (Topic-by-judgement) ist der natürliche Prüfstein — gehört in den
  Phase-0-Spike.
- **Betrieb:** Jev via OpenRouter = null Ops, Mikrobezahlung; lokal = ein
  Prozess (OpenDecision-Server) oder der vorhandene Ollama-Daemon (Qwen3.8,
  Gemma4, Granite4 sind lokal bereits vorhanden — Klassifikator via
  strukturiertem JSON sofort testbar, ohne Download).
- **Empfehlung:** V1 gegen OpenRouter-Jev bauen (§3/§4), Interface
  backend-agnostisch halten, **lokal als Backlog-Item in Phase 3**
  (OpenDecision zuerst — kompatibelste Form, kleinste Hardware-Hürde), nur
  bei < 10 % Accuracy-Abstand übernehmen.

## 7. Risiken & offene Punkte

- **Zugang:** Gelöst via OpenRouter (`typesafe/jev-1.13`, Alpha-Endpoint,
  Credits nötig, kein Free Tier) — Waitlist entfällt; in Phase 0
  Key-/Credits-Verfügbarkeit verifizieren.
- **Confidence-Semantik je Backend:** Jev = kalibrierte Korrektheit,
  OpenDecision = Konzentration, Ollama-JSON = unkalibriert → Thresholds pro
  Backend neu tunen, nie übernehmen.
- **Datenschutz:** Mail-Inhalte gehen an eine US-API → Minimierung
  (Subject/Sender/Snippet, Bodies kürzen/trunkieren) oder
  Nur-Metadaten-Variante (Jev sieht nur Metadaten, Bodies fasst der Agent
  lokal via MCP zusammen). Haltung vor Phase 1 festlegen und dokumentieren.
- **Datums-Schwäche:** Datumslogik bleibt im Code; Jev nur für
  Regel-Erfüllung — harte Regel, in jeden Prompt schreiben.
- **Latenz aus DE:** 70–500 ms sind US-West-Messungen; vor UI-Versprechen
  selbst messen.
- **Kosten:** Praktisch irrelevant (Größenordnung $0.0004/Fall); kein
  Gegenargument.
- **Modell-Version:** `jev-1.13.0` pinnen, Thresholds versionieren, bei
  Modellwechsel neu tunen.

## 8. Phasen (Ablauf, keine Umsetzung ohne Freigabe)

- **Phase 0 – Spike (Backend-Kandidaten auf eigenen Mails):** (a) Jev via
  OpenRouter (`typesafe/jev-1.13`, Alpha-Decisions-Endpoint — OpenRouter ist
  bereits referenziert, voraussichtlich kein neues Secret); (b) lokal
  OpenDecision-Server (TypeSafe-kompatible Form, Mails bleiben privat);
  (c) Ollama-Klassifikator (Qwen3/Gemma aus lokalem Bestand, strukturiertes
  JSON). Gleiches Mail-Sample, gleiche Fragen, Metrik: Trennung
  important/nicht-important + Topic-Treffer vs. bestehende Runs.
  **Go/No-Go:** Jev trennt bei Confidence ≥ 0.8 sinnvoll; lokal nur weiter
  bei < 10 % Accuracy-Abstand (sonst parken).
- **Phase 1 – V1 Mail:** `triage_mail`-Tool in der opencode-Schicht +
  `mail-digest`-Prompt-Umschreibung (Jev klassifiziert, Agent fasst
  zusammen) + A/B gegen bestehende Runs. Verifikation: `axiomata-cli
  run-skill mail-digest`, Dashboard zeigt unverändertes Schema.
- **Phase 2 – V1 Cleanup:** `check_cleanup`-Gate + Prompt-Umschreibung,
  Threshold-Tuning über Reports mit `unsicher`-Kategorie.
- **Phase 3 – Backlog:** Backend-Routing zuerst (größter Hebel), dann
  Risk-Gates, dann Score-Digests (Reihenfolge aus §5).
- **Phase 4 (optional) – V2 Rust-nativ:** nur bei bewährtem V1-Mehrwert:
  `axiomata-core::jev` (neue Deps `reqwest`+`rustls` — der
  Workspace-Kommentar in `Cargo.toml:29-32` sieht das bereits vor), Key als
  `config.toml`-Feld nach 0600-Konvention, CLI- + Tauri-Kommandos, danach
  `cargo clippy --workspace -- -D warnings`, `cargo fmt --check`,
  `cargo test --workspace` nach Projektstandard.

## 9. Quellen (Recherche 2026-09-21)

- TypeSafe AI, „Introducing System One Models & Jev"
  (https://typesafe.ai/blog/introducing-system-one-models-and-jev)
- LangChain, „What Is Jev? A Guide to TypeSafe AI's System One Model"
  (https://www.langchain.com/blog/building-a-harness-with-jev)
- Flavio Copes, „A deep dive into Jev, TypeSafe's System One model"
  (https://flaviocopes.com/jev/) — u. a. Versions-/Alias-Stand
  (`jev-latest`/`jev-preview` → `jev-1.13.0`), Kontextlimits (~64k),
  Latenz-Hinweis US-West
- JevAPI.dev, „Jev API — Fast, Type-Safe Structured Decisions"
  (https://jevapi.dev/) — Endpoint, Request-/Response-Form,
  Gateway-IDs, Preis
- DEV Community, „How to Use Jev" (https://dev.to/valyuai/...) —
  Scorecard mit Selbsteinschätzungs-Vorbehalten (67.8 % Workflow-Eval,
  selbst durchgeführt, Referenz = Frontier-Modell-Konsens)
- Tom's Hardware, „TypeSafe AI's Jev …" — Monitoring-Beispiel
  (Jev bewertet Ernsthaftigkeit, LLM untersucht)
- KDnuggets, „What Everyone Is Getting Wrong About TypeSafe AI's Jev" —
  Einordnung Kalibrierung (RLCD), 0-%-Type-Error als strukturell, nicht
  empirisch
- Vercel AI Gateway, Modellseite `typesafe-ai/jev`
  (https://vercel.com/ai-gateway/models/jev) — `evaluate`-Aufrufform
- Zenodo, „Jev in Practice" (`daf-jev`-Toolkit + MCP-Server, v0.3.0) —
  Hinweis auf existierenden Community-MCP-Server als Referenz
- OpenRouter, `typesafe/jev-1.13` + `~typesafe/jev-latest`
  (https://openrouter.ai/typesafe) — Preis, Alpha-Decisions-Endpoint, 32k,
  Namensformen; Jev Lab mit Live-Rezepten (https://openrouter.ai/labs/jev)
- Jev AI Guide, „Jev on OpenRouter"
  (https://jevaiguide.com/channels/openrouter/) — Request-/Response-Form,
  Unterschiede zur TypeSafe-API
- PyPI `opendecision` v0.1.1 / deepanwadhwa/OpenDecision + Doku
  (https://deepanwadhwa.github.io/OpenDecision/) — lokale
  `/v1/systemone`-kompatible Engine (ModernBERT-NLI)
- HuggingFace `com-kotobalabs/open-jev-deberta-v3-large` —
  Jev-förmige DeBERTa-Reproduktion mit Messprotokoll
- PyPI `openjevpro` (https://openjev.pro) — Open-Weight-Entscheidungs-Engine
  mit Constrained Decoding (Benchmarks selbst berichtet)
- systemonemodels.org, „Jev alternatives"
  (https://systemonemodels.org/examples/alternatives/) — Überblick +
  Hardware-Einordnung (CPU/GPU/Apple Silicon)
- MrJev, Open-Source-Kategorie
  (https://mrjev.com/projects/category/open-source/) — u. a. localjev,
  Jev Local, mini-jev
