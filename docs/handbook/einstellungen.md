# Einstellungen und Darstellung

Was in den Einstellungen steht und wie man das Aussehen anpasst: Theme, Schrift, Sprache, Bewegung, Glas mit Hintergrundbild, UI-Größe und Tastatur.

## Was es ist

Das Fenster „Einstellungen“ hat fünf Reiter: **„Darstellung“** (alles zum Aussehen), **„Workspace“** (Vault-Pfad und
Tags), **„KI-Anbieter“** (siehe [Provider](provider.md)), **„Speicher“** (siehe [Storage](storage.md)) und
**„Über“** (Owner, Workspace, Version). Änderungen unter „Darstellung“ gelten sofort.

Seit der Glas-Umstellung liegen alle Flächen als halbdurchsichtiges Glas über einem ruhigen, einmal verwischten
**Grund** — dem des Themes oder einem eigenen Bild. Es gibt keinen Schalter dafür; drei Regler stellen es ein.

## Wo die Knöpfe sind

**Einstellungen** öffnet das Zahnrad im Dock oder ⌘, (Windows/Linux: Ctrl+,). Unter „Darstellung“:

- **„Theme“:** Graphite, Paper, Steampunk, Forest, Ocean, fünf Catppuccin-Varianten, Tokyo Night, GitHub Dark und
  GitHub Light. Dasselbe schnell über „Theme“ im Dock.
- **„Schrift“:** Geist, Inter oder IBM Plex Sans (nicht für Editor und Terminal).
- **„Sprache“:** „System“ (die erste Sprache von macOS, die die App kann), „Deutsch“ oder „English“;
  gilt sofort. Namen wie Studio, Canvas, Flow oder Skills bleiben in beiden Sprachen gleich, und der Chat antwortet
  in der gewählten Sprache. Noch nicht jede Ansicht ist übersetzt — was fehlt, steht vorerst auf Englisch.
- **„Bewegung“:** „Voll“, „Ruhig“ oder „Aus“. macOS „Bewegung reduzieren“ stellt auf „Aus“.
- **„Licht“:** „Licht unter dem Zeiger“, „Tageszeit-Licht“, „Zündung beim Start“.
- **„Ton“:** „Töne“ (nur, solange Axiomata im Hintergrund ist), mit Hörproben.
- **„Canvas“:** „Punktraster zeigen“, „Magnetische Kanten“.
- **„Orbit & Second Brain“:** „Tempo“, „Beschriftungen“, „Dateinamen“, „Schimmer“, „Anordnung“.
- **„UI-Größe“:** „Auto“ (folgt der Pixeldichte des Bildschirms) oder der Regler „Text und Bedienelemente“ in Prozent.
- **„Tastatur“ → „Tastenkürzel nach“:** „Auto“, „Mac“ oder „Windows & Linux“.
- **„Glas“:** „Transparenz“ (0 % dicht, 100 % klar, 50 % der Look des Themes), „Unschärfe“ (wie weich der Grund ist),
  „Hintergrundbild“ mit „Bild wählen…“ / „Entfernen“ und, mit Bild, „Bild abdunkeln“ (wie stark das Theme das Bild
  abdunkelt bzw. aufhellt, damit Text lesbar bleibt).
- **„Eigenes CSS“:** „Eigenes CSS neu laden“ und „Vorlage kopieren“ für eine eigene `~/.axiomata/theme.css`.

Unter „Workspace“: „Vault“ → „Pfad“ → „Speichern & neu starten“; darunter „Tags“ (siehe [Tags und
Links](tags-links.md)).

## Häufige Handgriffe

1. **Eigenes Hintergrundbild:** „Darstellung“ → „Glas“ → „Bild wählen…“ → Bild wählen; mit „Bild abdunkeln“ und
   „Unschärfe“ lesbar machen.
2. **Ruhiger arbeiten:** „Bewegung“ auf „Ruhig“ (Glimmen steht still, der Orbit dreht weiter) oder „Aus“.
3. **Alles größer:** „UI-Größe“ → „Auto“ abwählen → „Text und Bedienelemente“ hochziehen (wird beim Loslassen
   übernommen).

## Fallen

- Ist macOS „Transparenz reduzieren“ an, ist alles dicht und die Glasregler sind gesperrt.
- Menüs und Text über anderen Flächen bleiben immer dicht, auch bei 100 % „Transparenz“.
- Ein neuer Vault-Pfad gilt erst nach dem Neustart („Speichern & neu starten“).
- „UI-Größe“ ändert nicht die Schrift von Editor und Terminal; die haben eigene Einstellungen.
- `theme.css` darf nur `:root { --ax-…: … }` enthalten; anderes wird abgelehnt und unter „Eigenes CSS“ als Fehler
  gezeigt.

## Siehe auch

[Provider](provider.md) · [Storage](storage.md) · [Orbit](orbit.md) · [Editor](editor.md) · [Terminal](terminal.md)
