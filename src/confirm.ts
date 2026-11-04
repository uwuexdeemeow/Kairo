/// How long a destructive button waits for its second press.
export const ARMED_FOR_MS = 5000;

/// Destructive buttons take a second click within a few seconds instead of a confirm dialog: the
/// first says what a second will do. Returns a function that puts the button back.
export function confirmTwice(btn: HTMLButtonElement, armedText: () => string, action: () => Promise<void>): () => void {
  let resting = "";
  let timer = 0;
  const disarm = () => {
    if (!btn.classList.contains("armed")) return;
    clearTimeout(timer);
    btn.classList.remove("armed");
    btn.textContent = resting;
  };
  btn.addEventListener("click", async () => {
    if (!btn.classList.contains("armed")) {
      resting = btn.textContent ?? "";
      btn.classList.add("armed");
      btn.textContent = armedText();
      timer = window.setTimeout(disarm, ARMED_FOR_MS);
      return;
    }
    disarm();
    await action();
  });
  return disarm;
}
