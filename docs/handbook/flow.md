# Flow: Pläne, Agententeam und Grenzen

Wie man im Studio-Modus „Flow“ einen Plan anlegt, vom Planer in Karten zerlegen lässt, freigibt, selbst laufen lässt und am Ende übernimmt — mit Grenzen, Kostenstand, Fragen der Agenten und Eskalation.

## Was es ist

Ein **Plan** ist ein Ziel für ein Projekt. Ein **Planer**-Agent zerlegt es in Karten (Vorschläge) mit Rolle, Stufe
und Abhängigkeiten; du nimmst sie an und gibst den Plan frei. Mit „Automatisch abarbeiten“ starten die Karten von
selbst, sobald ihre Vorgänger fertig sind; jede Karte bekommt einen Arbeiter und danach einen Reviewer, und die
fertige Arbeit sammelt sich auf einer eigenen **Linie des Plans** (einem Integrations-Branch). Am Ende übernimmst du
den ganzen Plan mit einem Klick in den Hauptbranch.

**Wer eine Karte bearbeitet:** die Rolle der Karte auf der Stufe der Karte — hat die Karte keine Stufe, die
Standard-Stufe der Rolle — auf der Standard-Engine dieser Stufe; kann die nicht laufen, die nächste Engine derselben
Stufe ([Modelle und Rollen](modelle-rollen.md)). **Der Reviewer** ist die Rolle der Art „review“ auf mindestens der
Stufe des Bearbeiters und läuft immer auf **einer anderen Engine** als der Bearbeiter — ein Modell, das seine eigene
Arbeit prüft, gibt sich recht. Gibt es keine solche Engine, startet kein Reviewer, und die App sagt dir, was fehlt.

Eine Karte, die zweimal zurückgegeben wird, geht an **dieselbe Rolle eine Stufe höher** (**Eskalation**, einmal je
Karte): die Sitzung wechselt an Ort und Stelle auf eine Engine der höheren Stufe, Worktree und Branch bleiben. War die
Karte schon Heavy oder läuft keine Engine der nächsten Stufe, gibt das Studio sie nicht weiter und wartet auf dich.

## Wo die Knöpfe sind

- **Flow-Bereiche:** „Planung“ (der Plan), „Agenten“ (Live-Kacheln des Teams), „Flow-Ansicht“ (der Graph); „Layout
  zurücksetzen“ legt sie zurück.
- **Plan anlegen:** „Neuer Plan…“ → Name und Ziel → „Anlegen“. Dann „Rolle des Planers“, „Danach grillen“ (das Ziel
  erst hinterfragen lassen) und „Planer starten“.
- **Vorschläge:** „Vorschläge (…)“ mit „Annehmen“, „Bearbeiten“, „Verwerfen“; „Karte hinzufügen“ für eigene.
- **Freigeben:** „Plan freigeben“ mit „Automatisch abarbeiten“ und „Automodus“ (Arbeiter dürfen ohne Rückfrage
  Befehle ausführen und ändern — nur für Karten, die du gelesen hast).
- **Grenzen:** „Kostengrenze (USD)“ über dem Ziel; die Zeile „Ausgaben: …“; bei Pause „Weiter“.
- **Ende:** „Plan übernehmen“ (alles in den Hauptbranch, nichts wird gepusht, ein Konflikt wird zurückgerollt);
  „Schließen“, „Löschen“.
- **Live-Kacheln:** ✉ für ungelesene Post, „Terminal“, bei Glut „Zusehen“ (folgt der Sitzung nur lesend).
- **Fragen der Agenten:** das Abzeichen „… wartet auf dich“ im Dock, im Orbit und auf Karten; Antwort mit
  „Erlauben“ / „Ablehnen“ / „Im Terminal“, auch als Streifen oben im Agenten-Pane.

## Häufige Handgriffe

1. **Plan aus einem Ziel:** „Neuer Plan…“ → Ziel → „Anlegen“ → „Planer starten“ → Vorschläge annehmen → „Plan freigeben“
   mit „Automatisch abarbeiten“.
2. **Karte als Plan:** im Kanban auf der Karte „Starten…“ → Projekt → „Als Plan starten“. Die Karte wird das Ziel; der
   Flow öffnet sich mit vorbelegtem „Danach grillen“, der Planer startet erst auf deinen Klick.
3. **Fertigen Plan übernehmen:** steht alles auf der Linie des Plans → „Plan übernehmen“.

## Fallen

- Ein Plan pausiert an seiner Grenze (Vorgabe 15 $ und 6 Mio. Tokens, ein Karten-Plan 5 $); „Weiter“ gibt neuen
  Spielraum. Die Tagesgrenze aller Studio-Sitzungen (20 $) stoppt bis morgen alles Neue ([Kosten](kosten.md)).
- Eine Sitzung, die ihre eigene Grenze erreicht, stoppt nach dem laufenden Schritt; die Karte bleibt bei ihr.
- Gibt das Studio eine Karte auf („das Studio hat aufgegeben“), entscheidest du: „Erneut integrieren“ oder „Neu machen“.
- „Automodus“ ist kein Sandkasten: Push, die CLI, Datenbank und Config-Dateien bleiben in ihren einfachen Formen
  gesperrt, mehr nicht.
- Selbstlaufende Pläne starten zusammen höchstens vier Kartensitzungen gleichzeitig; der Rest wartet.
- Wartet ein Agent auf dich, steht er still, bis du antwortest.
- Hat eine Stufe nur **eine** laufbare Engine, findet der Reviewer auf dieser Stufe keine andere und weicht nach oben
  aus; gibt es auch dort keine, bleibt die Karte ohne Review. Abhilfe: unter „Modelle“ ein zweites Modell der Stufe
  einschalten.
- Eine Karte ohne passende Engine ihrer Stufe startet nicht („Keine Engine der Stufe … kann …“); dann ein Modell dieser
  Stufe einschalten oder eine Standard-Engine setzen.

## Siehe auch

[Studio](studio.md) · [Modelle und Rollen](modelle-rollen.md) · [Kanban](kanban.md) · [Glut](glut.md) ·
[Kosten](kosten.md)
