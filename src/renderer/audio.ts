/**
 * Decodes a base64 WAV clip and plays it once. Used for both reminder
 * audio (attached to CompanionState) and flavor-line audio (returned
 * directly from synthesizeFlavorLine, since those never touch state).
 *
 * Stops any clip already playing first — without this, clicking the eye
 * again before a previous flavor line finishes would layer a second
 * playback on top instead of replacing it.
 *
 * `onEnded`, if given, fires once playback genuinely finishes (or fails to
 * start) — callers use this to dismiss a popup exactly when its dialogue
 * completes, rather than only on a pre-computed duration estimate.
 */
let currentAudio: HTMLAudioElement | null = null;

export function playBase64Wav(base64Wav: string, onEnded?: () => void): void {
  currentAudio?.pause();

  const binary = atob(base64Wav);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  const url = URL.createObjectURL(new Blob([bytes], { type: "audio/wav" }));
  const audio = new Audio(url);
  currentAudio = audio;

  const finish = () => {
    URL.revokeObjectURL(url);
    if (currentAudio === audio) currentAudio = null;
    onEnded?.();
  };
  audio.addEventListener("ended", finish);
  void audio.play().catch(() => {
    // Autoplay can be blocked in some contexts; the popup text still
    // shows either way, so this is silent-degrade, not an error to surface.
    finish();
  });
}
