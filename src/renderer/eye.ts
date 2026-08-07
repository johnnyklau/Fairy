const EYE_COLOR = "#378ADD";
const SVG_NS = "http://www.w3.org/2000/svg";

export function createEyeElement(): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, "svg") as SVGSVGElement;
  svg.setAttribute("viewBox", "0 0 120 120");
  svg.classList.add("eye");
  svg.innerHTML = `
    <defs>
      <filter id="eye-glow" x="-100%" y="-100%" width="300%" height="300%">
        <feGaussianBlur stdDeviation="6" result="blur" />
      </filter>
    </defs>
    <circle class="eye-glow-ring" cx="60" cy="60" r="46" fill="${EYE_COLOR}" filter="url(#eye-glow)" />
    <circle class="eye-white-ring" cx="60" cy="60" r="38" fill="none" stroke="#ffffff" stroke-width="4" />
    <circle class="eye-color-ring" cx="60" cy="60" r="30" fill="none" stroke="${EYE_COLOR}" stroke-width="6" />
    <circle class="eye-pupil" cx="60" cy="60" r="20" fill="${EYE_COLOR}" />
    <circle class="eye-highlight" cx="67" cy="52" r="4" fill="#ffffff" />
  `;
  return svg;
}

export function setEyeGlowHigh(eye: SVGSVGElement, high: boolean): void {
  eye.classList.toggle("glow-high", high);
}
