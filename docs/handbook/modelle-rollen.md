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
Vorschlag aus dem Preis → keine („ohne Stufe“, dann ist das Modell noch kein Agent). Der **Preisvorschlag** misst den
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

Studio → Leiste „Agenten“ → **„Modelle & Rollen…“** öffnet ein großes Pane mit zwei Reitern:

- **„Modelle“**
  - **„Standard-Engine je Stufe“**: je Stufe eine Auswahl; „Günstigste: …“ heißt ohne eigene Wahl.
  - **„Preisgrenzen“**: „Light unter“, „Heavy über“, „Grenzen speichern“.
  - **„Modelle“** mit „Alle neu vorschlagen“; darunter je Provider die Modelle mit Filterfeld („… Modelle filtern…“), je
    Modell ein Schalter („Einschalten“ / „Ausschalten“), Preis und Kontext, die Stufen-Knöpfe Light · Medium · Heavy
    (ein Klick auf deine Wahl nimmt sie zurück: „Deine Stufe — klicke, um sie zu entfernen“), die Herkunft („Deine“,
    „Vorschlag · nicht geprüft“, „Aus dem Preis“, „Ohne Stufe · kein Agent, bis es eine Stufe hat“), bei offenem
    Vorschlag die Marke „Wartet auf deine Prüfung“ und „Annehmen“, die Begründung mit Quelle und „Für … neu
    vorschlagen“. Läuft die Recherche, steht oben „Stufen der Modelle werden recherchiert…“.
  - **„Eigene Engines“** ganz unten: „Engine hinzufügen…“ mit „Bezeichnung“, „Harness“, „Modell“, „Stufe“ („Die Stufe
    seines Modells“ ohne Wahl) und unter „Erweitert“ „Befehl (leer = Standard des Harness)“, „Umgebung“ und
    „Abrechnung“.
- **„Rollen“**: die Rollen; „Rolle hinzufügen…“ und je Rolle „Name“, „Beschreibung“, „Art der Arbeit“, „Standardstufe“,
  „Arbeitsanweisungen (Markdown)“, „Rechte (Harness-Regeln, eine pro Zeile)“, unter „Erweitert“ die Grenzen
  („Schrittgrenze“, „Token-Grenze“, „Kostengrenze (USD)“) und „Legt Karten an, ohne zu fragen“; „Rolle speichern“.
- **Umstellungs-Hinweis** im Reiter „Rollen“ unter der Kopfzeile (nur nach der Umstellung): eine Zeile, was geschah,
  wo die Sicherung liegt, „OK“.

Im Terminal: `axiomata-cli ide models list|set|limits|default`, `ide engines list`, `ide roles list|show`,
`ide migrate-agents --dry-run`.

## Häufige Handgriffe

1. **Ein Modell nutzbar machen:** „Modelle“ → Modell suchen → einschalten; hat es keine Stufe, wartet es auf die
   Recherche oder du klickst selbst eine Stufe.
2. **Einen Vorschlag annehmen oder ändern:** beim Modell „Annehmen“ — oder eine andere Stufe klicken; deine Wahl gilt
   immer vor jedem Vorschlag.
3. **Festlegen, worauf eine Stufe läuft:** „Standard-Engine je Stufe“ → je Stufe die Engine wählen.
4. **Eine Rolle schärfer machen:** „Rollen“ → Rolle → Anweisung, Rechte oder „Standardstufe“ ändern → „Rolle speichern“.
5. **Eine eigene Engine** (eigener Befehl, eigene Umgebung, eigene Stufe): „Eigene Engines“ → „Engine hinzufügen…“.

## Die Umstellung beim ersten Start

Früher trug jede Rolle eine Engine, daher Kopien wie `planner-haiku` oder `reviewer-heavy`. Beim ersten Start mit dem
neuen Modell stellt die App einmalig um:

- Die sieben Grundrollen werden angelegt, falls sie fehlen. **Jede Kopie** (`planner-haiku`, `reviewer-heavy` …)
  wandert in die Sicherung, auch eine mit ein, zwei eigenen Zeilen: Wie stark ein Agent ist, sagt seine Stufe, nicht
  eine Kopie der Rolle. Ihre Sitzungen und Favoriten spielen danach die Grundrolle. Was `reviewer-light` und
  `reviewer-heavy` eigens sagten, sagt jetzt jedem Reviewer seine Startanweisung, nach der Stufe der Karte.
- Eigene Engines, die nur eine automatische sind, gehen in ihr auf (ihr Modell wird eingeschaltet). Eine eigene Engine
  mit eigenem Befehl, eigener Umgebung, eigener Stufe oder eigener Abrechnung bleibt.
- Je Stufe ohne deine Wahl wird die Standard-Engine gesetzt, die die alten Rollen dieser Stufe am meisten nutzten.
  Standard-Engines, Favoriten und Sitzungen, die auf eine entfernte Engine zeigten, ziehen auf die automatische um.
  Namen, Worktrees und Branches der Sitzungen bleiben.
- **Gelöscht wird nichts:** Jeder Lauf legt einen eigenen Sicherungsordner `~/.axiomata/agents-backup-<Datum>` an
  (beim zweiten Lauf am selben Tag mit `-2` …) mit den Originalen der Rollen und den entfernten Engines
  (`engines.toml`). Er bleibt, bis du ihn unter Einstellungen → Speicher löschst ([Storage](storage.md)).

## Fallen

- Ein Modell ohne Stufe ist **kein Agent**: es taucht unter „Agenten starten“ nicht auf.
- Die Recherche ist ein bezahlter Skill-Lauf auf dem Skill-Harness ([Kosten](kosten.md)).
- Eine Rolle nennt keine Engine; ein `engine:` in einer Rollendatei wird ignoriert — auch in den Rollen eines Projekts
  (`.axiomata/agents/`), das sagt die App beim Bestätigen.
- Chat und Skills wählen ihr Modell weiter unter Einstellungen → KI-Anbieter ([Provider](provider.md)), nicht hier.
- Eine Standard-Engine, die nicht zur Stufe passt, steht als „— nicht von dieser Stufe“ in der Auswahl; besser neu
  wählen.

## Siehe auch

[Studio](studio.md) · [Flow](flow.md) · [Storage](storage.md) · [Kosten](kosten.md) · [Provider](provider.md)
