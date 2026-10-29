# Kosten, Preise und Tagesgrenzen

Was Agenten-Läufe kosten, woher die Preise kommen und welche Grenzen Chat, Skills und das Studio stoppen.

## Was es ist

Axiomata misst Geld nur bei **bezahlten Providern** (OpenRouter; Ollama zählt nichts). Anthropic läuft über das Abo
und den Claude-Code-Login — dafür gibt es keine Kostenzählung, die Zeile heißt dann „billed through the subscription
— no cost tracking“. Gemessen wird nicht die Schätzung des Werkzeugs, sondern **Tokens × Preis je Modell**; der
Teil der Eingabe, der aus dem Cache kommt, wird zum günstigeren Cache-Preis gezählt, wenn einer bekannt ist.

**Die Preise** kommen aus OpenRouters Preisliste: bei jedem Auffrischen des Modell-Katalogs (beim App-Start, einmal
am Tag, mit dem ↻ am Modellfeld in den Einstellungen oder `axiomata-cli models refresh`) schreibt die App die
Listenpreise der benutzten Modelle in ihre Preistabelle. Ein bezahlter Preis wird nie durch einen kostenlosen
ersetzt. Beim Start rechnet die App schon erfasste Läufe gegen die Tabelle nach.

**Die Grenzen** (Vorgaben):

| Grenze | gilt für | Vorgabe |
|---|---|---|
| „Daily limit (USD, paid providers)“ | Chat und Skills/Routinen zusammen | 2 $ am Tag |
| Grenze eines Plans | alle Sitzungen eines Studio-Plans | 15 $ und 6 Mio. Tokens |
| Grenze eines Karten-Plans | ein mit „Start“ gestarteter Karten-Plan | 5 $ |
| Tagesgrenze des Studios | alle Plan- und Kartensitzungen eines Tages | 20 $ |
| Grenzen einer Sitzung | eine Karten-Sitzung, nach Stufe der Rolle | leicht 0,50 $, mittel 2 $, schwer 6 $ (dazu Tokens, Schritte) |

## Wo die Knöpfe sind

- **Settings → AI providers**, unten im Abschnitt „Harness & provider assignment“: je Rolle die Zeile „Spend chat →
  …“ mit heute / Grenze / diesen Monat, darunter das Feld „Daily limit (USD, paid providers)“ (leer = keine Grenze),
  dann „Save assignment“.
- **Studio → Flow**, im Plan: das Feld „Cost limit (USD)“ über dem Ziel (leer = keine eigene Grenze) und die Zeile
  „Verbrauch: …“ mit dem Tagesstand des Studios.
- **Studio → Agents-Bereich → „Engines & roles…“ → „Roles“**, eine Rolle öffnen → „Advanced: fallback engines,
  rights, limits“: „Cost limit (USD)“, „Token
  limit“, „Step limit“.
- Im Terminal: `axiomata-cli board usage` zeigt Schritte, Tokens und Geld einer Sitzung gegen ihre Grenzen.

## Häufige Handgriffe

1. **Tagesgrenze für Chat und Skills ändern:** Settings → AI providers → „Daily limit (USD, paid providers)“ →
   „Save assignment“.
2. **Ein pausierter Plan soll weiterlaufen:** im Flow beim Plan „Continue“ — das setzt seine Grenze auf den
   bisherigen Verbrauch plus einen neuen Spielraum.
3. **Prüfen, was heute ausgegeben wurde:** Settings → AI providers, die „Spend …“-Zeilen; für das Studio die
   „Verbrauch“-Zeile im Flow.

## Fallen

- Ist die Tagesgrenze erreicht, wird der **nächste** Aufruf über einen bezahlten Provider abgelehnt — auch der
  Refresh einer Kalender- oder Mail-Kachel, denn jeder Refresh ist ein bezahlter Agenten-Zug.
- Die Tagesgrenze des Studios (20 $) ist von der Grenze für Chat und Skills getrennt und hat **kein Feld in der
  Oberfläche**; sie steht als `studio_daily_usd_cap` in der Config. Ist sie erreicht, startet bis morgen nichts Neues
  und laufende bezahlte Sitzungen werden gestoppt.
- Ein Modell ohne bekannten Preis zählt kein Geld; dann greift nur die Token- oder Schrittgrenze.
- Gluts Websuche hat eigene Grenzen (Suchen pro Tag, Tavily pro Monat) — siehe [Provider](provider.md).

## Siehe auch

[Provider](provider.md) · [Flow](flow.md) · [Studio](studio.md) · [Verbindungen](verbindungen.md)
