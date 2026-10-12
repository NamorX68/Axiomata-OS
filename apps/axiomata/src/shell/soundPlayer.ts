/**
 * Plays the cues of `core/sounds.ts` with WebAudio — made on the spot, no sound files (`docs/plans/ui-ux-round.md`
 * §5). The webview lets an audio context run only once the owner did something in the window, so `unlockAudio` is
 * called from a click or key press; until then a cue stays silent rather than queueing up.
 */

import { CUE_VOICES, MASTER_GAIN, type Cue, type Voice } from "../core/sounds";

/** How fast a voice comes in, in seconds: soft, but without a click. */
const ATTACK_S = 0.008;
/** The quietest an exponential ramp may go (it cannot reach zero). */
const SILENT = 0.0001;

let context: AudioContext | null = null;

/** Makes (or wakes) the audio context. Call it from a user gesture; it settles once the context runs (or cannot). */
export async function unlockAudio(): Promise<void> {
  try {
    context ??= new AudioContext();
    if (context.state === "suspended") await context.resume();
  } catch {
    // No audio in this webview: the sounds simply stay silent.
  }
}

/** Lets the audio context go (the sounds were switched off), so it does not keep the audio device awake. */
export function closeAudio(): void {
  const ctx = context;
  context = null;
  if (ctx) void ctx.close().catch(() => {});
}

/**
 * Plays `cue`, if the audio context is allowed to run. WebKit may suspend or interrupt a context while the window is
 * hidden — the very case the sounds are for —, so a context that is not running is asked once to resume first.
 */
export function playCue(cue: Cue): void {
  const ctx = context;
  if (!ctx) return;
  if (ctx.state === "running") {
    voice(ctx, cue);
    return;
  }
  void ctx
    .resume()
    .then(() => {
      if (ctx.state === "running") voice(ctx, cue);
    })
    .catch(() => {});
}

/** Schedules every voice of `cue` on a running context. */
function voice(ctx: AudioContext, cue: Cue): void {
  const master = ctx.createGain();
  master.gain.value = MASTER_GAIN;
  master.connect(ctx.destination);
  const start = ctx.currentTime + 0.02;
  for (const voice of CUE_VOICES[cue]) {
    if (voice.kind === "tone") tone(ctx, master, voice, start);
    else spark(ctx, master, voice, start);
  }
}

/** The envelope every voice shares: a quick rise, then an exponential ring-out over `dur`. */
function envelope(ctx: AudioContext, out: AudioNode, voice: Voice, start: number): GainNode {
  const env = ctx.createGain();
  const t = start + voice.at;
  env.gain.setValueAtTime(SILENT, t);
  env.gain.linearRampToValueAtTime(voice.gain, t + ATTACK_S);
  env.gain.exponentialRampToValueAtTime(SILENT, t + voice.dur);
  env.connect(out);
  return env;
}

/** A sine with a faint octave above it, which makes it a soft bell rather than a test tone. */
function tone(ctx: AudioContext, out: AudioNode, voice: Voice, start: number): void {
  const t = start + voice.at;
  const env = envelope(ctx, out, voice, start);
  for (const [ratio, level] of [
    [1, 1],
    [2, 0.12],
  ] as const) {
    const osc = ctx.createOscillator();
    osc.type = "sine";
    osc.frequency.value = voice.freq * ratio;
    const mix = ctx.createGain();
    mix.gain.value = level;
    osc.connect(mix).connect(env);
    osc.start(t);
    osc.stop(t + voice.dur + 0.05);
  }
}

/** White noise through a band filter: the crackle of a fuse catching. */
function spark(ctx: AudioContext, out: AudioNode, voice: Voice, start: number): void {
  const t = start + voice.at;
  const length = Math.ceil(ctx.sampleRate * voice.dur);
  const buffer = ctx.createBuffer(1, length, ctx.sampleRate);
  const data = buffer.getChannelData(0);
  for (let i = 0; i < length; i++) data[i] = Math.random() * 2 - 1;
  const noise = ctx.createBufferSource();
  noise.buffer = buffer;
  const band = ctx.createBiquadFilter();
  band.type = "bandpass";
  band.frequency.value = voice.freq;
  band.Q.value = 1.2;
  noise.connect(band).connect(envelope(ctx, out, voice, start));
  noise.start(t);
  noise.stop(t + voice.dur);
}
