/** Small DOM helpers. Nothing here knows what Safe Invest is. */

export const $ = (selector, root = document) => root.querySelector(selector);
export const $$ = (selector, root = document) => [...root.querySelectorAll(selector)];

/**
 * Builds an element.
 *
 * Text always goes in through `textContent`, never `innerHTML`: an asset name
 * comes from a market API, and a page that pastes remote text into markup is
 * one bad API response away from executing it.
 */
export function el(tag, attributes = {}, children = []) {
  const node = document.createElement(tag);

  for (const [key, value] of Object.entries(attributes)) {
    if (value === null || value === undefined || value === false) continue;
    if (key === "class") node.className = value;
    else if (key === "text") node.textContent = value;
    else if (key === "html") throw new Error("el(): pas de HTML brut, utilisez text");
    else if (key.startsWith("on")) node.addEventListener(key.slice(2).toLowerCase(), value);
    else if (key === "dataset") Object.assign(node.dataset, value);
    else node.setAttribute(key, value === true ? "" : String(value));
  }

  for (const child of [children].flat()) {
    if (child === null || child === undefined || child === false) continue;
    node.append(typeof child === "string" ? document.createTextNode(child) : child);
  }
  return node;
}

export function clear(node) {
  node.replaceChildren();
  return node;
}

/**
 * Appends children, skipping the absent ones.
 *
 * `Node.append(null)` inserts the *text* "null", which is how the word once
 * appeared under a goal that had no achieved return yet. This is the only way
 * children get appended outside `el`.
 */
export function appendAll(node, ...children) {
  for (const child of children.flat()) {
    if (child === null || child === undefined || child === false) continue;
    node.append(child);
  }
  return node;
}

/** Maps the engine's direction (+1, -1, 0) to a class name. */
export function directionClass(direction) {
  if (direction > 0) return "up";
  if (direction < 0) return "down";
  return "flat";
}

let toastTimer = 0;

export function toast(message, kind = "info", hint = null) {
  const stack = $("#toast-stack");
  const node = el("div", { class: `toast ${kind}` }, [
    message,
    hint ? el("span", { class: "toast-hint", text: hint }) : null,
  ]);

  stack.append(node);
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => node.remove(), kind === "error" ? 7000 : 3500);
}

/** Shows an error the way the user should see it: plainly, with the hint. */
export function reportError(error) {
  toast(error?.message ?? String(error), "error", error?.hint ?? null);
}
