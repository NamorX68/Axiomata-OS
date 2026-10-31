# Notizen anlegen, ablegen und die Inbox

Wie man eine neue Notiz schreibt, wie der Agent sie beim Speichern in den passenden Bereich des Vaults ablegt und was mit der Inbox geschieht.

## Was es ist

Der **Vault** ist der Ordner mit deinen Markdown-Notizen (Pfad unter Einstellungen → Workspace → „Vault“). Jeder oberste
Ordner ist ein **Bereich**; in ihm und im Vault selbst liegt eine `AGENTS.md` mit einer erzeugten Karte der Dateien
(der „Router“), damit Agenten sich zurechtfinden. Die Kachel **„Gedächtnis“** zeigt, ob diese Karten aktuell sind
(„aktuell“ / „veraltet“), und „Synchronisieren“ schreibt sie neu.

Eine **neue Notiz** schreibst du, ohne einen Ordner zu wählen: beim ersten Speichern sucht ein Agenten-Zug den
passenden Bereich, einen Dateinamen, Tags aus der Tag-Liste und verwandte Notizen. Passt kein vorhandener Bereich, darf
er einen neuen vorschlagen; nur wenn gar nichts passt, landet sie in `Inbox/`. Die **Inbox** ist der Platz für noch
nicht Einsortiertes; der Skill `inbox-sort` räumt sie auf.

## Wo die Knöpfe sind

- **Neue Notiz:** Dock „Notiz“, ⌘K „Neue Notiz“ oder im Studio ⌘N. Es öffnet sich das schwebende Dateifenster „Neue
  Notiz“ zum Schreiben; unten steht „Neue Notiz — ⌘S legt sie ab“.
- **Ablegen:** ⌘S. Während der Agent arbeitet: „Die Notiz wird abgelegt…“; danach öffnet sich die Notiz lesend an ihrem
  neuen Platz.
- **Schließen, ohne abzulegen:** die Frage „Diese Notiz ist noch nicht abgelegt.“ mit „Ablegen“ / „Verwerfen“ /
  „Abbrechen“.
- **Inbox sortieren:** im Skills Deck `inbox-sort` ▶ (oder als Routine).
- **Leere Datei an festem Ort:** `/newfile <pfad>` im Chat.

## Häufige Handgriffe

1. **Schnell etwas festhalten:** Dock „Notiz“ → schreiben → ⌘S. Der Agent legt ab.
2. **Inbox leeren:** `inbox-sort` starten; er verschiebt nur klare Fälle, versieht sie mit Tags und bis zu drei
   verwandten Links und listet unter „Fragen“, was er nicht zuordnen konnte.
3. **Router auffrischen:** in der Kachel „Gedächtnis“ auf „Synchronisieren“, wenn sie „veraltet“ zeigt.

## Fallen

- Eine neue Notiz wird **nie** automatisch gespeichert; erst ⌘S legt sie ab. Das Ablegen ist ein Agenten-Zug auf dem
  Harness und Provider des Chats und zählt gegen die Tagesgrenze.
- Der Agent überschreibt nie eine vorhandene Datei. Ist der Vault-Pfad falsch, landet nichts dort, wo du suchst.
- `inbox-sort` legt **keine neuen Bereiche** an und verschiebt nur in vorhandene Ordner; Unklares bleibt in der
  Inbox.
- In den Ordner `Kanban/` (Spiegel des Boards) gehören keine eigenen Notizen — die App überschreibt ihn.
- Der Router zwischen den Markierungen `AXIOMATA-ROUTER` in `AGENTS.md` wird erzeugt; eigener Text gehört außerhalb.

## Siehe auch

[Editor](editor.md) · [Tags und Links](tags-links.md) · [Second Brain](second-brain.md) · [Skills](skills.md)
