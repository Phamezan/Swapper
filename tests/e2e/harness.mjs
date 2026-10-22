import { setTimeout as delay } from "node:timers/promises";

export async function poll(label, check, timeout = 3500, interval = 25) {
  const end = performance.now() + timeout;
  let last;
  do {
    try { const value = await check(); if (value) return value; }
    catch (error) { last = error; }
    await delay(interval);
  } while (performance.now() < end);
  throw new Error(`Timed out waiting for ${label}${last ? `: ${last.message}` : ""}`);
}

// Missing favicon is Vite preview noise; application errors are never allow-listed.
export function allowance(issue) {
  const missing = issue.kind === "resource" && issue.status === 404;
  const missingLog = issue.kind === "log" && issue.text === "Failed to load resource: the server responded with a status of 404 (Not Found)";
  if ((missing || missingLog) && issue.url && new URL(issue.url).pathname === "/favicon.ico") {
    return "Vite previews do not supply a favicon";
  }
  return null;
}

export class Cdp {
  #next = 0;
  #pending = new Map();
  #listeners = new Map();
  #closed = false;

  constructor(socket) {
    this.socket = socket;
    socket.addEventListener("message", event => {
      const message = JSON.parse(event.data);
      if (message.id) {
        const pending = this.#pending.get(message.id);
        if (!pending) return;
        this.#pending.delete(message.id);
        clearTimeout(pending.timer);
        if (message.error) pending.reject(new Error(`${pending.method}: ${message.error.message}`));
        else pending.resolve(message.result);
      } else {
        for (const listener of this.#listeners.get(message.method) ?? []) listener(message.params);
      }
    });
    socket.addEventListener("close", () => {
      this.#closed = true;
      for (const pending of this.#pending.values()) {
        clearTimeout(pending.timer);
        pending.reject(new Error(`CDP connection closed during ${pending.method}`));
      }
      this.#pending.clear();
    });
  }

  static async connect(url) {
    const socket = new WebSocket(url);
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => { socket.close(); reject(new Error("CDP connection timed out")); }, 4000);
      socket.addEventListener("open", () => { clearTimeout(timer); resolve(); }, { once: true });
      socket.addEventListener("error", () => { clearTimeout(timer); reject(new Error(`Cannot connect to ${url}`)); }, { once: true });
    });
    return new Cdp(socket);
  }

  on(method, listener) {
    if (!this.#listeners.has(method)) this.#listeners.set(method, new Set());
    this.#listeners.get(method).add(listener);
  }

  send(method, params = {}, timeout = 4000) {
    if (this.#closed) return Promise.reject(new Error("CDP connection closed"));
    return new Promise((resolve, reject) => {
      const id = ++this.#next;
      const timer = setTimeout(() => {
        this.#pending.delete(id);
        reject(new Error(`${method} timed out`));
      }, timeout);
      this.#pending.set(id, { method, resolve, reject, timer });
      try { this.socket.send(JSON.stringify({ id, method, params })); }
      catch (error) { clearTimeout(timer); this.#pending.delete(id); reject(error); }
    });
  }

  close() { this.socket.close(); }
}

export class Page {
  constructor(cdp) { this.cdp = cdp; }

  async evaluate(expression) {
    const result = await this.cdp.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
    return result.result.value;
  }

  wait(expression, label = expression) { return poll(label, () => this.evaluate(expression)); }

  async visible(selector) {
    return this.wait(`Array.from(document.querySelectorAll(${JSON.stringify(selector)})).some(el => el.checkVisibility() && el.getBoundingClientRect().width > 0)`, selector);
  }

  async text(selector, text) {
    return this.wait(`Array.from(document.querySelectorAll(${JSON.stringify(selector)})).some(el => el.checkVisibility() && el.textContent.includes(${JSON.stringify(text)}))`, `${selector} containing ${text}`);
  }

  async click(selector, text = null) {
    const position = await poll(`clickable ${selector}${text ? ` (${text})` : ""}`, () => this.evaluate(`(() => {
      const el = Array.from(document.querySelectorAll(${JSON.stringify(selector)})).find(el =>
        el.checkVisibility() && !el.disabled && (${JSON.stringify(text)} === null || el.textContent.trim() === ${JSON.stringify(text)}));
      if (!el) return null;
      el.scrollIntoView({ block: 'center', inline: 'nearest', behavior: 'instant' });
      const r = el.getBoundingClientRect();
      const x = r.x + r.width / 2, y = r.y + r.height / 2;
      const hit = document.elementFromPoint(x, y);
      return r.width && r.height && (hit === el || el.contains(hit)) ? { x, y } : null;
    })()`));
    await this.cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", ...position, button: "left", clickCount: 1 });
    await this.cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", ...position, button: "left", clickCount: 1 });
  }

  async type(selector, text) {
    await this.click(selector);
    await this.cdp.send("Input.insertText", { text });
  }

  async settle() {
    await this.evaluate("document.fonts.ready.then(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))");
  }
}

export function horizontalOverflow() {
  const selectors = ["html", "body", "#root", ".fa-frame", ".dashboard-shell", ".sidebar", "main", ".full-app-embed",
    ".ui-page-header", ".dash-v2", ".champion-tools", ".tier-table-wrap", ".selected-champion", ".md-page",
    ".fa-accounts", ".fa-settings-layout", ".fa-draft-page", ".cs-band", ".cs-planning", ".fa-build-section",
    ".account-picker", ".fa-picker-popover", ".shell", ".main-panel", ".ob-card"];
  return selectors.flatMap(selector => Array.from(document.querySelectorAll(selector)).filter(el => el.checkVisibility() && el.clientWidth > 0)
    .filter(el => el.scrollWidth > el.clientWidth + 1)
    .map(el => `${selector}: ${el.scrollWidth}px content in ${el.clientWidth}px container`));
}
