# Engines, Rollen, Agenten — neu gedacht

Stand: 2026-10-30, **gegrillt** (Q1–Q21, Owner), Go zum Bau ohne Zwischenhalt (Owner: „Baue ruhig alles fertig“; ein
Review-Agent gibt je Schritt grünes Licht). Die Inspektor-Reiter (Q20) schaut der Owner danach live an.

## 1. Heute

- Eine **Rolle** (`~/.axiomata/agents/<name>/AGENT.md`, `axiomata-roster::role`) ist zweierlei: die Aufgabe und eine
  fest eingebaute Engine (`engine`, `fallback_engines`). Darum 22 Rollen-Dateien statt 7 — Varianten wie
  `planner-haiku`, `reviewer-heavy`, `allrounder-glut`; ihre `tier` hängt an der Rolle, nicht am Modell
  (`reviewer-heavy` läuft auf Sonnet).
- **Engines** (`config.agents.engines`, `axiomata-roster::engine`, 17 Stück) sind von Hand angelegt, 12 heißen noch
  `opencode-…` und laufen auf Glut.
- Der **Modellkatalog** (`model_catalog.rs`, `~/.axiomata/model-catalog.json`) kennt schon alles Nötige: OpenRouter
  über `models/user` (nur die Modelle, die der Owner dort zulässt), Ollama über `/api/tags`, Anthropic als feste Liste;
  je Modell Preis, Kontext und Denkstufen.
- Die **Stufe einer Karte** zählt nur bei der Wahl des Reviewers (`card_session::needed_tier`, `reviewers_for`); der
  Bearbeiter kommt aus `card.agent` → `choose_engine` (Rolle, dann ihre Fallbacks).
- Der Studio-Reiter „Agents“ zeigt Rollen als Chip-Menüs mit Dateiendungen als Varianten und eine Liste aller
  Sitzungen bis zurück zum 7. Oktober.

## 2. Entscheidungen (Owner, 2026-10-29/30)

- **Q1** Drei Stufen: `light`, `medium`, `heavy` (wie die Karten).
- **Q2** Die App schlägt die Stufe eines Modells vor — aus dem Preis und einer kurzen Recherche über Bewertungsseiten;
  der Owner kann sie jederzeit ändern.
- **Q3** Eine kleine Modellliste (≤ 20) ist ganz eingeschaltet; eine große ist aus, der Owner kreuzt an.
- **Q4** Einschalten und Einstufen im Studio, Inspektor-Reiter „Models“ (das Studio wird ein eigenes Programm; Chat
  und Skills wählen ihr Modell weiter in den Einstellungen).
- **Q5** 7 feste Rollen: allrounder, implementer, documenter, planner, reviewer, tester, grill; eigene kommen dazu.
- **Q6** Eine Rolle enthält Anweisung, Rechte, Grenzen, Art der Arbeit und eine **Standard-Stufe** — keine Engine.
- **Q7** Agenten = Rolle + Stufe + Engine, alle **ausgerechnet**, nicht gespeichert; Favoriten per Stern.
- **Q8** Name „‹Harness› ‹Stufe› ‹Rolle› (‹Modell›)“, z. B. „Glut Medium Allrounder (Sonnet)“, angezeigt in zwei
  Zeilen (oben Rolle · Stufe, darunter Harness · Modell); eine Sitzung hängt `#n` an. Keine Kartennummer im Namen.
- **Q9** Automatische Engines: je eingeschaltetes Modell eine je Harness, der es kann. Claude Code: von Anthropic nur
  das **neueste Modell jeder Familie** (Fable, Opus, Sonnet, Haiku — Sonnet 5 fällt weg). Glut: die OpenRouter- und
  Ollama-Modelle. Ein verschwundenes Modell verschwindet aus der Auswahl, laufende Sitzungen bleiben. Automatische
  Engines sind nicht editierbar; eigene Engines („Own engines“) bleiben möglich, mit eigener Stufe.
- **Q10** Denkstufe: die Standard-Denkstufe des Modells, änderbar in der laufenden Pane; nicht Teil der Engine.
- **Q11** Recherche als mitgelieferter Skill `model-tiering` (Skill-Harness, Websuche): läuft, sobald ein Modell ohne
  Stufe auftaucht, und auf „Neu vorschlagen“; Quellen Artificial Analysis, LMArena, OpenRouter-Ranglisten, Preis;
  Ergebnis je Modell Stufe + ein Satz Begründung mit Quelle. Bis dahin gilt der Preisvorschlag; Ollama nur Recherche.
- **Q12** Preisgrenzen (Ausgabe, $ je Million Tokens): light < 1, medium 1–10, heavy > 10; änderbar im Reiter.
- **Q13** Je Stufe eine **Standard-Engine** des Owners, ohne Festlegung die billigste; fällt eine aus (Schlüssel fehlt,
  nicht erreichbar), die nächste derselben Stufe. Ersetzt `fallback_engines`.
- **Q14** Einmalige Umstellung beim ersten Start: 22 Rollen → 7 (Stufe aus der Grundrolle; Varianten mit eigenem Text
  werden dem Owner gezeigt), alte Dateien in einen Sicherungsordner (gelöscht erst auf sein Wort); alte Sitzungen
  behalten Name und Modell, werden auf neue Rolle + passende automatische Engine umgehängt; die 17 Engines gehen in
  den automatischen auf.
- **Q15** Flow: Bearbeiter = Rolle der Karte + Stufe der Karte (sonst Standard-Stufe der Rolle) → Standard-Engine der
  Stufe; Reviewer mindestens dieselbe Stufe; nach zwei Rückgaben dieselbe Rolle eine Stufe höher.
- **Q16** Reiter „Agents“: Master-Kacheln · Favoriten (ein Klick startet) · „Start an agent“ (Rolle → light · medium ·
  heavy → Engine, Standard markiert → Start; kürzester Weg zwei Klicks) · Sitzungen. Dieselbe Auswahl im Code-Modus
  beim Öffnen einer Pane. Gebaut aus dem App-Design (Hex, `ui/`, Akzentregel); die Chip-Menüs fallen weg.
- **Q17** Die Liste zeigt nur laufende, wartende und beendete Sitzungen mit nicht übernommener Arbeit („Wartet auf
  dich: übernehmen oder verwerfen“). Beendete ohne eigene Arbeit verschwinden aus der Liste; die wöchentliche Wartung
  räumt sie nach erneuter Prüfung weg (Worktree + leerer Zweig) und nennt sie im Storage-Reiter. Arbeit wird nie von
  selbst gelöscht.
- **Q18** Favoriten per Stern im Start-Bereich und an jeder Sitzung, anfangs keine; ein Favorit ist Rolle + Stufe +
  Engine oder Rolle + Stufe + „Standard-Engine“ (zieht dann mit).
- **Q19** Projektrollen: gleicher Name ersetzt die feste Rolle im Projekt, neuer Name ist eine weitere Rolle; keine
  Engine mehr — eine genannte wird ignoriert und beim Bestätigen gesagt.
- **Q20** Inspektor „Models“ (Modelle je Harness mit Schalter, Stufe, Vorschlag + Begründung, Preis; Standard-Engine je
  Stufe; Preisgrenzen; „Own engines“) und „Roles“ (Anweisung, Rechte, Grenzen, Standard-Stufe, „Add role…“); „Engines
  & roles“ fällt weg. Der Owner will es **live sehen**.
- **Q21** Chat und Skills bleiben bei ihrer Modellwahl in den Einstellungen; Skills nach Stufe später.

## 3. Bau

- **AM1 Datenmodell:** Stufe je Modell (Owner-Wert, Vorschlag, Begründung, Quelle) und eingeschaltet/aus; Preisgrenzen;
  automatische Engines aus Katalog + Einschaltung (Anthropic: neuestes je Familie); eigene Engines mit Stufe;
  Standard-Engine je Stufe und Auflösung „Stufe → Engine“ mit Ausweichen; Rollen ohne Engine (Feld wird ignoriert);
  Agent = Rolle + Stufe + Engine mit Namen (Q8). CLI zum Lesen/Setzen.
- **AM2 Recherche:** mitgelieferter Skill `model-tiering`, Auslöser bei Modellen ohne Stufe und auf Knopfdruck; das
  Ergebnis landet als Vorschlag, nie als Owner-Wert.
- **AM3 Inspektor:** Reiter „Models“ und „Roles“ statt „Engines & roles“.
- **AM4 Start:** Reiter „Agents“ neu (Favoriten, „Start an agent“, aufgeräumte Liste), dieselbe Auswahl im Code-Modus;
  Wartung räumt beendete Sitzungen ohne Arbeit.
- **AM5 Flow:** Bearbeiter, Reviewer, Eskalation nach Stufe.
- **AM6 Umstellung:** Rollen, Engines, Sitzungen (Q14), mit Sicherungsordner.
- **AM7 Handbuch:** `studio.md`, `flow.md`, neue Abschnitte; `docs/architecture.md`.

Prüfungen: der volle Gate je Schritt; ein Review-Agent je Schritt (grünes Licht vor dem nächsten); `security-auditor`
für AM1/AM2/AM6 (Konfiguration, Skill-Rechte, Dateien verschieben); Sicht im Browser-Mock.
