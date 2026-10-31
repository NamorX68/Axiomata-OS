# Settings and appearance

What is in the settings and how to adjust the look: theme, typeface, language, motion, glass with a background image, UI size and keyboard.

## What it is

The “Settings” window has five tabs: **“Appearance”** (everything about the look), **“Workspace”** (vault path and
tags), **“AI providers”** (see [Providers](provider.md)), **“Storage”** (see [Storage](storage.md)) and
**“About”** (owner, workspace, version). Changes under “Appearance” apply immediately.

Since the switch to glass, all surfaces lie as translucent glass over a still, once-blurred
**ground** — the theme's own or a picture of your own. There is no switch for it; three sliders set it.

## Where the buttons are

The gear in the Dock or ⌘, (Windows/Linux: Ctrl+,) opens **Settings**. Under “Appearance”:

- **“Theme”:** Graphite, Paper, Steampunk, Forest, Ocean, five Catppuccin variants, Tokyo Night, GitHub Dark and
  GitHub Light. The same quickly through “Theme” in the Dock.
- **“Typeface”:** Geist, Inter or IBM Plex Sans (not for the editor and terminal).
- **“Language” / “Sprache”:** “System” (the first macOS language the app knows), “Deutsch” or “English”;
  applies immediately. Names such as Studio, Canvas, Flow or Skills stay the same in both languages, and the chat
  answers in the chosen language. Not every view is translated yet — what is missing stays in English for now.
- **“Motion” → “Movement”:** “Full”, “Calm” or “Off”. macOS “Reduce motion” sets it to “Off”.
- **“Light”:** “Light under the pointer”, “Time-of-day light”, “Start ignition”.
- **“Sound”:** “Sounds” (only while Axiomata is in the background), with previews.
- **“Canvas”:** “Show dot grid”, “Magnetic edges”.
- **“Orbit & Second Brain”:** “Speed”, “Labels”, “File names”, “Shimmer”, “Layout”.
- **“UI size”:** “Auto” (follows the display's pixel density) or the slider “Text and controls” in percent.
- **“Keyboard” → “Shortcuts follow”:** “Auto”, “Mac” or “Windows & Linux”.
- **“Glass”:** “Transparency” (0 % solid, 100 % clear, 50 % the theme's look), “Blur” (how soft the ground is),
  “Background image” with “Choose image…” / “Remove” and, with a picture, “Image dim” (how strongly the theme darkens
  or lightens the picture so that text stays readable).
- **“Custom CSS”:** “Reload custom CSS” and “Copy template” for your own `~/.axiomata/theme.css`.

Under “Workspace”: “Vault” → “Path” → “Save & restart”; below it “Tags” (see [Tags and links](tags-links.md)).

## Common tasks

1. **Your own background image:** Appearance → “Glass” → “Choose image…” → choose a picture; make it readable with
   “Image dim” and “Blur”.
2. **Work more calmly:** “Movement” to “Calm” (the glow stands still, the Orbit keeps turning) or “Off”.
3. **Everything bigger:** “UI size” → deselect “Auto” → drag “Text and controls” up (taken over when you let go).

## Traps

- If macOS “Reduce transparency” is on, everything is solid and the glass sliders are locked.
- Menus and text over other surfaces always stay solid, even at 100 % “Transparency”.
- A new vault path applies only after the restart (“Save & restart”).
- “UI size” does not change the font of the editor and terminal; they have settings of their own.
- `theme.css` may contain only `:root { --ax-…: … }`; anything else is refused and shown as an error under
  “Custom CSS”.

## See also

[Providers](provider.md) · [Storage](storage.md) · [Orbit](orbit.md) · [Editor](editor.md) · [Terminal](terminal.md)
