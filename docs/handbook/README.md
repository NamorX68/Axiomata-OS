# Das Handbuch von Axiomata-OS

Dieser Ordner ist die **Quelle** des Handbuchs: eine Seite je Bereich der App, auf Deutsch, mit den echten (deutschen)
Namen der Knöpfe, wie die App sie zeigt; die englischen Seiten in `en/` nennen die englischen. Der Chat in der Leiste
antwortet nur aus diesen Seiten und aus den Live-Daten der App; was hier nicht steht, weiß er nicht.

- **Hier bearbeiten, nirgends sonst.** Die App liefert das Handbuch mit jedem Build aus und legt es beim Start nach
  `~/.axiomata/handbook/`. Passt die Kopie dort nicht zum installierten Build, ersetzt die App sie — eine Änderung in
  `~/.axiomata/handbook/` geht also beim nächsten Start verloren.
- **Eigene Notizen zur App** (Gewohnheiten, Ideen, offene Fragen) gehören in den Vault, nicht hierher.
- **Wer eine sichtbare Funktion baut oder ändert, pflegt ihre Seite** im selben Zug; ohne das ist die Runde nicht
  fertig.

Form jeder Seite: `# Titel`, darunter **ein** Satz Zusammenfassung (er wird das Inhaltsverzeichnis des Chats), dann
„Was es ist“, „Wo die Knöpfe sind“, „Häufige Handgriffe“, „Fallen“ und bei Bedarf „Siehe auch“. Etwa eine
Bildschirmseite, ohne Screenshots, ohne Schlüssel oder andere Geheimnisse. Der Einstieg ist [index.md](index.md).
