# Flow: Pläne, Agententeam und Grenzen

Wie man im Studio-Modus „Flow“ einen Plan anlegt, vom Planer in Karten zerlegen lässt, freigibt, selbst laufen lässt und am Ende übernimmt — mit Grenzen, Kostenstand, Fragen der Agenten und Eskalation.

## Was es ist

Ein **Plan** ist ein Ziel für ein Projekt. Ein **Planer**-Agent zerlegt es in Karten (Vorschläge) mit Rolle, Stufe
und Abhängigkeiten; du nimmst sie an und gibst den Plan frei. Mit „Automatisch abarbeiten“ starten die Karten von
selbst, sobald ihre Vorgänger fertig sind; jede Karte bekommt einen Arbeiter und danach einen Reviewer, und die
fertige Arbeit sammelt sich auf einer eigenen **Linie des Plans** (einem Integrations-Branch). Am Ende übernimmst du
den ganzen Plan mit einem Klick in den Hauptbranch.

Eine Karte, die zweimal zurückgegeben wird oder abbricht, geht an die nächststärkere Rolle gleicher Art
(**Eskalation**). Passt sie auch dann nicht, gibt das Studio sie auf und wartet auf dich.

## Wo die Knöpfe sind

- **Flow-Bereiche:** „Planning“ (der Plan), „Agents“ (Live-Kacheln des Teams), „Flow view“ (der Graph); „Reset
  layout“ legt sie zurück.
- **Plan anlegen:** „New plan …“ → Name und Ziel → „Anlegen“. Dann „Role of the planner“, „Danach grillen“ (das Ziel
  erst hinterfragen lassen) und „Planer starten“.
- **Vorschläge:** „Proposals (…)“ mit „Accept“, „Edit“, „Discard“; „Add card“ für eigene.
- **Freigeben:** „Release plan“ mit „Automatisch abarbeiten“ und „Auto mode“ (Arbeiter dürfen ohne Rückfrage
  Befehle ausführen und ändern — nur für Karten, die du gelesen hast).
- **Grenzen:** „Cost limit (USD)“ über dem Ziel; die Zeile „Verbrauch: …“; bei Pause „Continue“.
- **Ende:** „Take over plan“ (alles in den Hauptbranch, nichts wird gepusht, ein Konflikt wird zurückgerollt);
  „Close“, „Delete“.
- **Live-Kacheln:** ✉ für ungelesene Post, „Terminal“, bei Glut „Watch“ (folgt der Sitzung nur lesend).
- **Fragen der Agenten:** das Abzeichen „… is waiting for you“ im Dock, im Orbit und auf Karten; Antwort mit
  „Allow“ / „Deny“ / „In terminal“, auch als Streifen oben im Agenten-Pane.

## Häufige Handgriffe

1. **Plan aus einem Ziel:** „New plan …“ → Ziel → „Anlegen“ → „Planer starten“ → Vorschläge annehmen → „Release
   plan“ mit „Automatisch abarbeiten“.
2. **Karte als Plan:** im Kanban auf der Karte „Start …“ → Projekt → „Start as plan“. Die Karte wird das Ziel; der
   Flow öffnet sich mit vorbelegtem „Danach grillen“, der Planer startet erst auf deinen Klick.
3. **Fertigen Plan übernehmen:** steht alles auf der Linie des Plans → „Take over plan“.

## Fallen

- Ein Plan pausiert an seiner Grenze (Vorgabe 15 $ und 6 Mio. Tokens, ein Karten-Plan 5 $); „Continue“ gibt neuen
  Spielraum. Die Tagesgrenze aller Studio-Sitzungen (20 $) stoppt bis morgen alles Neue ([Kosten](kosten.md)).
- Eine Sitzung, die ihre eigene Grenze erreicht, stoppt nach dem laufenden Schritt; die Karte bleibt bei ihr.
- Gibt das Studio eine Karte auf („the Studio gave up“), entscheidest du: „Erneut integrieren“ oder „Neu machen“.
- „Auto mode“ ist kein Sandkasten: Push, die CLI, Datenbank und Config-Dateien bleiben in ihren einfachen Formen
  gesperrt, mehr nicht.
- Selbstlaufende Pläne starten zusammen höchstens vier Kartensitzungen gleichzeitig; der Rest wartet.
- Wartet ein Agent auf dich, steht er still, bis du antwortest.

## Siehe auch

[Studio](studio.md) · [Kanban](kanban.md) · [Glut](glut.md) · [Kosten](kosten.md)
