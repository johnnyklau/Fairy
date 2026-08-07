const EYE_COLOR = "#378ADD";

export function createPopupElement(): HTMLDivElement {
  const popup = document.createElement("div");
  popup.className = "popup";
  popup.innerHTML = `
    <div class="popup-badge">
      <svg viewBox="0 0 40 40">
        <circle cx="20" cy="20" r="16" fill="${EYE_COLOR}" />
        <circle cx="24" cy="16" r="3" fill="#ffffff" />
      </svg>
    </div>
    <p class="popup-text"></p>
  `;
  return popup;
}

export function setPopupText(popup: HTMLDivElement, text: string): void {
  const textEl = popup.querySelector<HTMLParagraphElement>(".popup-text");
  if (textEl) textEl.textContent = text;
}

export function showPopup(popup: HTMLDivElement): void {
  popup.classList.add("visible");
}

export function hidePopup(popup: HTMLDivElement): void {
  popup.classList.remove("visible");
}
