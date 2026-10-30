# Einstellungen und Darstellung

Was in den Einstellungen steht und wie man das Aussehen anpasst: Theme, Schrift, Sprache, Bewegung, Glas mit Hintergrundbild, UI-Größe und Tastatur.

## Was es ist

Das Fenster „Settings“ hat fünf Reiter: **„Appearance“** (alles zum Aussehen), **„Workspace“** (Vault-Pfad und
Tags), **„AI providers“** (siehe [Provider](provider.md)), **„Storage“** (siehe [Storage](storage.md)) und
**„About“** (Owner, Workspace, Version). Änderungen unter „Appearance“ gelten sofort.

Seit der Glas-Umstellung liegen alle Flächen als halbdurchsichtiges Glas über einem ruhigen, einmal verwischten
**Grund** — dem des Themes oder einem eigenen Bild. Es gibt keinen Schalter dafür; drei Regler stellen es ein.

## Wo die Knöpfe sind

**Settings** öffnet das Zahnrad im Dock oder ⌘, (Windows/Linux: Ctrl+,). Unter „Appearance“:

- **„Theme“:** Graphite, Paper, Steampunk, Forest, Ocean, fünf Catppuccin-Varianten, Tokyo Night, GitHub Dark und
  GitHub Light. Dasselbe schnell über „Theme“ im Dock.
- **„Typeface“:** Geist, Inter oder IBM Plex Sans (nicht für Editor und Terminal).
- **„Language“ / „Sprache“:** „System“ (die erste Sprache von macOS, die die App kann), „Deutsch“ oder „English“;
  gilt sofort. Namen wie Studio, Canvas, Flow oder Skills bleiben in beiden Sprachen gleich, und der Chat antwortet
  in der gewählten Sprache. Noch nicht jede Ansicht ist übersetzt — was fehlt, steht vorerst auf Englisch.
- **„Motion“ → „Movement“:** „Full“, „Calm“ oder „Off“. macOS „Bewegung reduzieren“ stellt auf „Off“.
- **„Light“:** „Light under the pointer“, „Time-of-day light“, „Start ignition“.
- **„Sound“:** „Sounds“ (nur, solange Axiomata im Hintergrund ist), mit Hörproben.
- **„Canvas“:** „Show dot grid“, „Magnetic edges“.
- **„Orbit & Second Brain“:** „Speed“, „Labels“, „File names“, „Shimmer“, „Layout“.
- **„UI size“:** „Auto“ (folgt der Pixeldichte des Bildschirms) oder der Regler „Text and controls“ in Prozent.
- **„Keyboard“ → „Shortcuts follow“:** „Auto“, „Mac“ oder „Windows & Linux“.
- **„Glass“:** „Transparency“ (0 % dicht, 100 % klar, 50 % der Look des Themes), „Blur“ (wie weich der Grund ist),
  „Background image“ mit „Choose image…“ / „Remove“ und, mit Bild, „Image dim“ (wie stark das Theme das Bild
  abdunkelt bzw. aufhellt, damit Text lesbar bleibt).
- **„Custom CSS“:** „Reload custom CSS“ und „Copy template“ für eine eigene `~/.axiomata/theme.css`.

Unter „Workspace“: „Vault“ → „Path“ → „Save & restart“; darunter „Tags“ (siehe [Tags und Links](tags-links.md)).

## Häufige Handgriffe

1. **Eigenes Hintergrundbild:** Appearance → „Glass“ → „Choose image…“ → Bild wählen; mit „Image dim“ und „Blur“
   lesbar machen.
2. **Ruhiger arbeiten:** „Movement“ auf „Calm“ (Glimmen steht still, der Orbit dreht weiter) oder „Off“.
3. **Alles größer:** „UI size“ → „Auto“ abwählen → „Text and controls“ hochziehen (wird beim Loslassen übernommen).

## Fallen

- Ist macOS „Transparenz reduzieren“ an, ist alles dicht und die Glasregler sind gesperrt.
- Menüs und Text über anderen Flächen bleiben immer dicht, auch bei 100 % „Transparency“.
- Ein neuer Vault-Pfad gilt erst nach dem Neustart („Save & restart“).
- „UI size“ ändert nicht die Schrift von Editor und Terminal; die haben eigene Einstellungen.
- `theme.css` darf nur `:root { --ax-…: … }` enthalten; anderes wird abgelehnt und unter „Custom CSS“ als Fehler
  gezeigt.

## Siehe auch

[Provider](provider.md) · [Storage](storage.md) · [Orbit](orbit.md) · [Editor](editor.md) · [Terminal](terminal.md)
