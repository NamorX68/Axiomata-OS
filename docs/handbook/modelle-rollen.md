# Modelle, Stufen und Rollen

Wie das Studio Modelle einschaltet und in die Stufen Light, Medium und Heavy einordnet, welche Engine eine Stufe startet, was eine Rolle ist und was die einmalige Umstellung auf Rollen mit Stufen gemacht hat.

## Was es ist

Ein **Agent** im Studio ist immer **Rolle + Stufe + Engine** — ausgerechnet, nicht gespeichert:

- Die **Rolle** sagt, wofür: Arbeitsanweisung, Rechte, Grenzen, Art der Arbeit und eine **Standard-Stufe**. Sie nennt
  **keine Engine**. Mitgeliefert sind sieben: allrounder (Medium), implementer (Medium), documenter (Light), planner
  (Heavy), reviewer (Medium), tester (Medium), grill (Heavy); eigene kommen dazu (`~/.axiomata/agents/<id>/AGENT.md`).
- Die **Stufe** (Light · Medium · Heavy) sagt, wie stark: Light für kleine, klar umrissene Änderungen, Medium für den
  Normalfall, Heavy für Großes oder Heikles.
- Die **Engine** ist Harness + Modell. Für jedes **eingeschaltete** Modell gibt es von selbst eine Engine je Harness,
  der es kann: Claude Code für Anthropic (nur das neueste Modell jeder Familie), Glut für OpenRouter und Ollama. Diese
  **automatischen Engines** sind nicht editierbar; verschwindet ein Modell aus dem Katalog, verschwindet seine Engine
  aus der Auswahl, laufende Sitzungen bleiben.

**Woher die Stufe eines Modells kommt**, in dieser Reihenfolge: deine eigene Wahl → der Vorschlag der Recherche → der
Vorschlag aus dem Preis → keine („Unrated“, dann ist das Modell noch kein Agent). Der **Preisvorschlag** misst den
Ausgabepreis je Million Tokens: unter 1 $ Light, 1–10 $ Medium, über 10 $ Heavy. Die **Recherche** ist der
mitgelieferte Skill `model-tiering`: er schlägt auf Bewertungsseiten nach (Artificial Analysis, LMArena,
OpenRouter-Ranglisten) und legt je Modell eine Stufe mit einem Satz Begründung und Quelle als Vorschlag ab — nie als
deine Wahl. Er startet von selbst, wenn nach einem Katalog-Refresh ein eingeschaltetes Modell ohne Stufe auftaucht
(höchstens einmal am Tag je Modell), und auf Knopfdruck. Ollama-Modelle haben keinen Preis, für sie gilt nur die
Recherche.

**Welche Engine eine Stufe startet:** deine **Standard-Engine** der Stufe, ohne Wahl die billigste der Stufe. Kann sie
nicht laufen (Schlüssel fehlt, nicht erreichbar), nimmt das Studio die nächste derselben Stufe.

Eine kleine Modellliste eines Providers (bis 20 Modelle) ist ganz eingeschaltet; bei einer großen ist alles aus, und du
kreuzt an, was du willst.

## Wo die Knöpfe sind

Studio → Leiste „Agents“ → **„Models & roles…“** öffnet ein großes Pane mit zwei Reitern:

- **„Models“**
  - **„Default engine per tier“**: je Stufe eine Auswahl; „Cheapest: …“ heißt ohne eigene Wahl.
  - **„Price limits“**: „Light below“, „Heavy above“, „Save limits“.
  - **„Models“** mit „Propose all again“; darunter je Provider die Modelle mit Filterfeld („Filter … models…“), je
    Modell ein Schalter („Switch on“ / „Switch off“), Preis und Kontext, die Stufen-Knöpfe Light · Medium · Heavy (ein
    Klick auf deine Wahl nimmt sie zurück: „Your tier — click to drop it“), die Herkunft („Yours“, „Proposal · not
    reviewed“, „From the price“, „Unrated · no agent until it has a tier“), bei offenem Vorschlag die Marke „Waits for
    your review“ und „Accept“, die Begründung mit Quelle und „Propose again for …“. Läuft die Recherche, steht oben
    „Researching model tiers…“.
  - **„Own engines“** ganz unten: „Add engine…“ mit „Label“, „Harness“, „Model“, „Tier“ („The tier of its model“ ohne
    Wahl) und unter „Advanced“ „Command (empty = the harness default)“, „Environment“ und „Billing“.
- **„Roles“**: die Rollen; „Add role…“ und je Rolle „Name“, „Description“, „Kind of work“, „Default tier“, „Working
  instructions (Markdown)“, „Rights (harness rules, one per line)“, unter „Advanced“ die Grenzen („Step limit“, „Token
  limit“, „Cost limit (USD)“) und „Creates cards without asking“; „Save role“.
- **Umstellungs-Hinweis** im Reiter „Roles“ unter der Kopfzeile (nur nach der Umstellung): eine Zeile, was geschah,
  „OK“; je Rolle, die anders ist als ihre Grundrolle, „Side by side“, „Keep as own role“ und „Fold into ‹Grundrolle›“.

Im Terminal: `axiomata-cli ide models list|set|limits|default`, `ide engines list`, `ide roles list|show`,
`ide migrate-agents --dry-run`.

## Häufige Handgriffe

1. **Ein Modell nutzbar machen:** „Models“ → Modell suchen → einschalten; hat es keine Stufe, wartet es auf die
   Recherche oder du klickst selbst eine Stufe.
2. **Einen Vorschlag annehmen oder ändern:** beim Modell „Accept“ — oder eine andere Stufe klicken; deine Wahl gilt
   immer vor jedem Vorschlag.
3. **Festlegen, worauf eine Stufe läuft:** „Default engine per tier“ → je Stufe die Engine wählen.
4. **Eine Rolle schärfer machen:** „Roles“ → Rolle → Anweisung, Rechte oder „Default tier“ ändern → „Save role“.
5. **Eine eigene Engine** (eigener Befehl, eigene Umgebung, eigene Stufe): „Own engines“ → „Add engine…“.

## Die Umstellung beim ersten Start

Früher trug jede Rolle eine Engine, daher Kopien wie `planner-haiku` oder `reviewer-heavy`. Beim ersten Start mit dem
neuen Modell stellt die App einmalig um:

- Die sieben Grundrollen werden angelegt, falls sie fehlen. Eine Kopie, die dasselbe sagt wie ihre Grundrolle, wandert
  in die Sicherung; eine mit eigenem Text bleibt als eigene Rolle und erscheint im Umstellungs-Hinweis.
- Eigene Engines, die nur eine automatische sind, gehen in ihr auf (ihr Modell wird eingeschaltet). Eine eigene Engine
  mit eigenem Befehl, eigener Umgebung, eigener Stufe oder eigener Abrechnung bleibt.
- Je Stufe ohne deine Wahl wird die Standard-Engine gesetzt, die die alten Rollen dieser Stufe am meisten nutzten.
  Standard-Engines, Favoriten und Sitzungen, die auf eine entfernte Engine zeigten, ziehen auf die automatische um.
  Namen, Worktrees und Branches der Sitzungen bleiben.
- **Gelöscht wird nichts:** Jeder Lauf legt einen eigenen Sicherungsordner `~/.axiomata/agents-backup-<Datum>` an
  (beim zweiten Lauf am selben Tag mit `-2` …) mit den Originalen der Rollen und den entfernten Engines
  (`engines.toml`). Er bleibt, bis du ihn unter Settings → Storage löschst ([Storage](storage.md)).

## Fallen

- Ein Modell ohne Stufe ist **kein Agent**: es taucht unter „Start an agent“ nicht auf.
- Die Recherche ist ein bezahlter Skill-Lauf auf dem Skill-Harness ([Kosten](kosten.md)).
- Eine Rolle nennt keine Engine; ein `engine:` in einer Rollendatei wird ignoriert — auch in den Rollen eines Projekts
  (`.axiomata/agents/`), das sagt die App beim Bestätigen.
- Chat und Skills wählen ihr Modell weiter unter Settings → „AI providers“ ([Provider](provider.md)), nicht hier.
- Eine Standard-Engine, die nicht zur Stufe passt, steht als „— not of this tier“ in der Auswahl; besser neu wählen.
- „Fold into ‹Grundrolle›“ legt die Kopie in die Sicherung, ihre Sitzungen spielen danach die Grundrolle.

## Siehe auch

[Studio](studio.md) · [Flow](flow.md) · [Storage](storage.md) · [Kosten](kosten.md) · [Provider](provider.md)
