import { initRenderer, renderState } from "./renderer";
import { initState } from "./state";

window.addEventListener("DOMContentLoaded", () => {
  initRenderer();
  initState((state) => {
    renderState(state);
  });
});
