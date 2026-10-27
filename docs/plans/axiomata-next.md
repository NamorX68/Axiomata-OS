# Axiomata — what comes next (owner's ideas, 2026-10-27)

Collected on 2026-10-27, to be grilled one at a time. Each is the owner's own tool, not a product to publish.

## N1 — the morning AI news podcast (private)

Every night, articles about AI are gathered from a short, chosen list of sites, the ten best are picked, and a model
turns them into a spoken episode. In the morning it is waiting, to hear with a coffee. It is only for the owner and
is never published.

- **How it fits what stands:** a Glut plan (T60b, `glut schedule add … --at 03:00`) runs a skill at night. The skill
  fetches the sites (`web_fetch`), ranks and picks the ten, and writes the script. A speech model reads it into an
  audio file, which lands where the owner listens: Studio, the phone, or a folder.
- **Speech:** try both a local model and one over OpenRouter.
  - Locally, Ollama now has small text-to-speech models. One is `phonon`: 31–40M parameters, German via `phonon:de`,
    24 kHz mono, Apple Silicon, Ollama ≥ 0.40.9. The 40M models are CC BY-NC, which is fine for private use.
  - Whether its output can be saved to a file is unverified. Ollama's speech guide (`docs.ollama.com/capabilities/speech`)
    is the place to check.
- **To grill:**
  - the sites, and more topics later;
  - the length and language of an episode;
  - which voice;
  - where the file goes;
  - what happens on a night a site is down;
  - the budget of a run.

## N2 — a voice assistant in Axiomata

In the EU, Apple's new Siri is still waiting. The idea is a quick voice assistant inside Axiomata: ask something
briefly and get an answer, or dictate a note while typing something else.

- **Ideas to grill:**
  - **Speaking:** a global push-to-talk key instead of a wake word, so no microphone listens all the time.
  - **Recognition:** done locally on the Mac (Whisper, Apple's own).
  - **Answers:** through Glut, with web search.
  - **Notes:** a dictated note lands as a card in the Kanban, or in an inbox.
  - **Questions about the owner's own state:** "what runs now", "what did the night run bring".
  - **Voice replies:** read aloud only when asked.
  - **Requirements:** short waits, and private speech.
  - **Voices:** a playful voice of the owner's choosing is the owner's private matter at home.

## N3 — Terminal, Kanban and Studio in pure Rust

Terminal, Kanban and Studio move to pure Rust, without Tauri and Svelte. The owner is sure this makes sense.

- **Before it:** the owner's larger idea, which is the whole voice part: N1 and N2, then N5 with them. After it comes
  pure Rust.
- **Order of building:** N1 and N2 are built as Rust services in the core, not bound to today's Tauri and Svelte
  front end. They then survive the move, and only their surface is drawn anew.

## N5 — a small browser beside the assistant

Kanban card #84 ("Einfacher Browser entwickeln", backlog, 2026-10-12).

A browser that can do little, and is meant to do little:

- an address and search field;
- back and forward;
- a simple list of favourites;
- a button that sends the page to the default browser.

It fits the voice assistant (N2) well. The assistant answers a question ("who played in film X", "how old is the actor
Y"), and the browser shows what it found ("show me pictures of Y"). The owner can then go on from there by hand, or
hand the page to the real browser.

To grill:
- how the assistant and the browser hand each other a page;
- which engine to use (the system's WebKit; and what it can be once the front end is pure Rust, N3);
- that a page never reaches Axiomata's own capabilities;
- where the favourites are kept.

## N4 — the presentation as a film

This belongs to Glut's T77 (`../Axiomata-Glut/docs/plans/glut-tui.md`): Axiomata presented to the owner's department
in about four weeks.

- **Format:** a film that is played, and after it the live demonstration. The film carries the story safely; live
  shows that it is real.
- **Video and sound:** OpenRouter's video and sound models, Qwen's speech model, and a package the owner found.
- **Narrator:** to grill when T77 is grilled.
