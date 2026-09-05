/**
 * Wiring: which screen is showing, what refreshes it, and what the buttons do.
 */

import { AppError, api, onGameChanged } from "./api.js";
import { $, $$, reportError, toast } from "./ui.js";
import * as screens from "./screens.js";

const state = {
  screen: "home",
  tab: "dashboard",
  dashboard: null,
  marketKind: "all",
  refreshSeconds: 60,
  timer: 0,
};

/* --------------------------------------------------------------- boot */

async function boot() {
  bindNavigation();
  bindNewGameForm();
  bindMarket();
  bindTradeDialog();
  bindSettings();

  const info = await api.appInfo().catch(() => null);
  if (info) {
    $("#version-line").textContent =
      `Safe Invest ${info.version} · données dans ${info.dataDir}` +
      (info.demoMode ? " · mode démonstration" : "");
  }

  await applyDisplaySettings();
  await goHome();

  // An AI trading in the other process must show up here without a refresh.
  await onGameChanged(() => {
    if (state.screen === "game") refreshGame({ quiet: true });
    if (state.screen === "home") loadGames();
  });
}

function bindNavigation() {
  for (const button of $$("[data-goto]")) {
    button.addEventListener("click", () => {
      const target = button.dataset.goto;
      if (target === "home") goHome();
      else screens.showScreen(target);
    });
  }

  $("#btn-new-game").addEventListener("click", () => {
    screens.showScreen("new");
    state.screen = "new";
  });

  $("#btn-settings-home").addEventListener("click", () => openSettings());
  $("#btn-how").addEventListener("click", () => $("#how-dialog").showModal());
  $("#how-close").addEventListener("click", () => $("#how-dialog").close());

  for (const tab of $$(".tab")) {
    tab.addEventListener("click", () => {
      state.tab = tab.dataset.tab;
      screens.showTab(state.tab);
      if (state.tab === "market") loadMarket();
      if (state.tab === "history") loadHistory();
    });
  }
}

/* --------------------------------------------------------------- home */

async function goHome() {
  stopRefresh();
  state.screen = "home";
  screens.showScreen("home");
  await loadGames();
}

async function loadGames() {
  try {
    const games = await api.listGames();
    screens.renderGames(games, {
      onOpen: async (id) => {
        await api.openGame(id);
        await enterGame();
      },
      onDelete: async (game) => {
        const ok = confirm(`Supprimer définitivement la partie de ${game.playerName} ?`);
        if (!ok) return;
        await api.deleteGame(game.id).catch(reportError);
        await loadGames();
      },
    });
  } catch (error) {
    reportError(error);
  }
}

/* ---------------------------------------------------------- new game */

function bindNewGameForm() {
  const form = $("#new-game-form");
  const goalField = $("#goal-field");

  for (const radio of form.elements.playerKind) {
    radio.addEventListener("change", () => {
      goalField.hidden = form.elements.playerKind.value !== "ai";
      updateGoalPreview();
    });
  }

  for (const preset of $$("#cash-presets .preset")) {
    preset.addEventListener("click", () => {
      for (const other of $$("#cash-presets .preset")) other.classList.remove("is-active");
      preset.classList.add("is-active");
      form.elements.startingCash.value = preset.dataset.amount;
      updateGoalPreview();
    });
  }

  for (const name of ["startingCash", "targetAmount", "deadline"]) {
    form.elements[name].addEventListener("input", updateGoalPreview);
  }

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const error = $("#new-game-error");
    error.hidden = true;

    const data = Object.fromEntries(new FormData(form).entries());
    try {
      await api.createGame({
        playerName: data.playerName,
        playerKind: data.playerKind,
        startingCash: data.startingCash,
        currency: data.currency,
        feePercent: data.feePercent,
        targetAmount: data.playerKind === "ai" ? data.targetAmount : null,
        deadline: data.playerKind === "ai" ? data.deadline : null,
      });
      form.reset();
      await enterGame();
    } catch (raw) {
      const failure = raw instanceof AppError ? raw : new AppError(String(raw));
      error.textContent = failure.hint ? `${failure.message} ${failure.hint}` : failure.message;
      error.hidden = false;
    }
  });
}

/**
 * Says out loud what the goal actually demands, before the game starts.
 *
 * "+180 %/an" is a sentence a beginner can weigh; "15 000 € en 2027" is not.
 */
function updateGoalPreview() {
  const form = $("#new-game-form");
  const preview = $("#goal-preview");
  preview.textContent = "";

  if (form.elements.playerKind.value !== "ai") return;

  const start = Number(String(form.elements.startingCash.value).replace(",", "."));
  const target = Number(String(form.elements.targetAmount.value).replace(",", "."));
  const deadline = form.elements.deadline.value;
  if (!(start > 0) || !(target > start) || !deadline) return;

  const years = (new Date(deadline) - Date.now()) / (365.25 * 24 * 3600 * 1000);
  if (!(years > 0.01)) {
    preview.textContent = "Cette date est trop proche pour que l'objectif ait un sens.";
    return;
  }

  const rate = ((target / start) ** (1 / years) - 1) * 100;
  const rounded = rate.toFixed(1).replace(".", ",");

  let verdict = "c'est ambitieux mais jouable";
  if (rate < 8) verdict = "c'est raisonnable, proche de ce que fait un marché actions sur longue durée";
  else if (rate > 40) verdict = "c'est très ambitieux : aucun placement ne tient ce rythme durablement";
  else if (rate > 100) verdict = "aucun placement réel ne fait ça";

  preview.textContent = `Il faudrait environ +${rounded} % par an — ${verdict}.`;
}

/* --------------------------------------------------------------- game */

async function enterGame() {
  state.screen = "game";
  state.tab = "dashboard";
  screens.showScreen("game");
  screens.showTab("dashboard");
  await refreshGame();
  startRefresh();
}

async function refreshGame({ quiet = false } = {}) {
  try {
    const view = await api.dashboard();
    state.dashboard = view;
    screens.renderDashboard(view, { onBuy: openBuy, onSell: openSell });

    if (view.observerMode) {
      const trades = await api.history(5);
      screens.renderAiFeed(trades, true);
    } else {
      screens.renderAiFeed([], false);
    }

    if (state.tab === "history") await loadHistory();
  } catch (error) {
    if (!quiet) reportError(error);
  }
}

function startRefresh() {
  stopRefresh();
  state.timer = setInterval(() => {
    if (state.screen === "game") refreshGame({ quiet: true });
  }, state.refreshSeconds * 1000);
}

function stopRefresh() {
  clearInterval(state.timer);
  state.timer = 0;
}

async function loadHistory() {
  try {
    screens.renderHistory(await api.history(null));
  } catch (error) {
    reportError(error);
  }
}

/* ------------------------------------------------------------- market */

let marketDebounce = 0;

function bindMarket() {
  $("#market-search").addEventListener("input", () => {
    clearTimeout(marketDebounce);
    // Without this, every keystroke would be one search request per source.
    marketDebounce = setTimeout(loadMarket, 350);
  });

  for (const chip of $$("#kind-chips .chip")) {
    chip.addEventListener("click", () => {
      for (const other of $$("#kind-chips .chip")) other.classList.remove("is-active");
      chip.classList.add("is-active");
      state.marketKind = chip.dataset.kind;
      loadMarket();
    });
  }
}

async function loadMarket() {
  try {
    const rows = await api.market($("#market-search").value, state.marketKind);
    screens.renderMarket(rows, {
      readOnly: Boolean(state.dashboard?.observerMode),
      onBuy: (row) => openTrade("buy", row),
    });
  } catch (error) {
    reportError(error);
  }
}

/* ------------------------------------------------------- trade dialog */

const trade = { side: "buy", mode: "amount", asset: null };

function openBuy(position) {
  openTrade("buy", {
    symbol: position.symbol,
    name: position.name,
    kind: position.kind,
    priceRaw: position.priceRaw,
    price: position.price,
  });
}

function openSell(position) {
  openTrade("sell", {
    symbol: position.symbol,
    name: position.name,
    kind: position.kind,
    priceRaw: position.priceRaw,
    price: position.price,
    quantity: position.quantity,
    quantityRaw: position.quantityRaw,
  });
}

function openTrade(side, asset) {
  trade.side = side;
  trade.asset = asset;
  trade.mode = side === "sell" ? "quantity" : "amount";

  $("#trade-title").textContent = side === "buy" ? "Acheter" : "Vendre";
  $("#trade-asset").textContent =
    `${asset.symbol} — ${asset.name}` + (asset.price ? ` · ${asset.price}` : "");
  $("#trade-input").value = "";
  $("#trade-error").hidden = true;

  // "Tout vendre" only makes sense on a position that exists.
  $("#trade-mode-all").hidden = side !== "sell";

  // A human need not justify a trade; in an AI game the window is read-only,
  // so this field is only ever shown when the game is a human one.
  $("#trade-rationale-field").hidden = true;

  syncTradeMode();
  updateEstimate();
  $("#trade-dialog").showModal();
  $("#trade-input").focus();
}

function syncTradeMode() {
  for (const button of $$("#trade-mode button")) {
    button.classList.toggle("is-active", button.dataset.mode === trade.mode);
  }

  const label = $("#trade-input-label");
  const input = $("#trade-input");

  if (trade.mode === "all") {
    label.textContent = "Toute la position";
    input.value = trade.asset?.quantity ?? "";
    input.disabled = true;
  } else {
    input.disabled = false;
    label.textContent =
      trade.mode === "amount"
        ? trade.side === "buy" ? "Montant à investir" : "Montant à récupérer"
        : "Quantité";
  }
  updateEstimate();
}

/**
 * Says what the typed figure buys, before the order is sent.
 *
 * "3 000 €, c'est 0,049 BTC" is the sentence that makes an order concrete for
 * someone learning. It is a preview only — the engine recomputes everything in
 * decimal, and its answer is what gets recorded.
 */
function updateEstimate() {
  const estimate = $("#trade-estimate");
  const asset = trade.asset;
  if (!asset) {
    estimate.textContent = "";
    return;
  }

  const held = asset.quantity ? `Vous détenez ${asset.quantity}. ` : "";

  if (trade.mode === "all") {
    estimate.textContent = `${held}Tout sera vendu au cours du moment.`;
    return;
  }

  const typed = Number(String($("#trade-input").value).replace(",", "."));
  const price = asset.priceRaw;
  if (!(typed > 0) || !(price > 0)) {
    estimate.textContent = held;
    return;
  }

  if (trade.mode === "amount") {
    const units = typed / price;
    // Small prices need more decimals to say anything at all.
    const shown = units < 1 ? units.toPrecision(4) : units.toFixed(4);
    estimate.textContent =
      `${held}Soit environ ${shown.replace(".", ",")} ${asset.symbol} au cours actuel.`;
  } else {
    const total = typed * price;
    estimate.textContent =
      `${held}Soit environ ${total.toLocaleString("fr-FR", { maximumFractionDigits: 2 })} ` +
      `au cours actuel, frais en plus.`;
  }
}

function bindTradeDialog() {
  for (const button of $$("#trade-mode button")) {
    button.addEventListener("click", () => {
      trade.mode = button.dataset.mode;
      syncTradeMode();
    });
  }

  $("#trade-input").addEventListener("input", updateEstimate);
  $("#trade-cancel").addEventListener("click", () => $("#trade-dialog").close());

  $("#trade-form").addEventListener("submit", async (event) => {
    event.preventDefault();
    const error = $("#trade-error");
    error.hidden = true;

    const value = $("#trade-input").value.trim();
    const args = {
      symbol: trade.asset.symbol,
      kind: trade.asset.kind,
      quantity: trade.mode === "quantity" ? value : null,
      amount: trade.mode === "amount" ? value : null,
      all: trade.mode === "all",
      rationale: $("#trade-rationale").value.trim() || null,
    };

    try {
      const done = trade.side === "buy" ? await api.buy(args) : await api.sell(args);
      $("#trade-dialog").close();
      toast(`${done.sideLabel} : ${done.quantity} ${done.symbol} pour ${done.total}`, "ok");
      await refreshGame();
      if (state.tab === "market") await loadMarket();
    } catch (raw) {
      const failure = raw instanceof AppError ? raw : new AppError(String(raw));
      error.textContent = failure.hint ? `${failure.message} ${failure.hint}` : failure.message;
      error.hidden = false;
    }
  });
}

/* ----------------------------------------------------------- settings */

async function openSettings() {
  state.screen = "settings";
  screens.showScreen("settings");

  try {
    const { settings, configuredKeys, demoForced } = await api.getSettings();

    $("#opt-colourblind").checked = settings.colourBlindPalette;
    $("#opt-refresh").value = String(settings.refreshIntervalSeconds);

    // `--demo` on the command line wins, but it is the launch that chose it,
    // not the user. Show it as such rather than ticking their box for them.
    const simulated = $("#opt-simulated");
    simulated.checked = demoForced || settings.forceSimulatedMode;
    simulated.disabled = demoForced;
    simulated.closest(".switch").lastElementChild.textContent = demoForced
      ? "Mode démonstration : imposé par l'option --demo au lancement"
      : "Mode démonstration : cours simulés, aucun appel réseau";

    screens.renderKeyForm(configuredKeys, {
      onSave: async (providerId, key) => {
        if (!key.trim()) return;
        await api.setApiKey(providerId, key).catch(reportError);
        toast("Clé enregistrée et chiffrée sur cette machine.", "ok");
        screens.renderSources(await api.marketSources());
      },
    });

    screens.renderSources(await api.marketSources());
  } catch (error) {
    reportError(error);
  }
}

function bindSettings() {
  const persist = async (change) => {
    try {
      const { settings } = await api.getSettings();
      await api.saveSettings({ ...settings, ...change });
      await applyDisplaySettings();
      screens.renderSources(await api.marketSources());
    } catch (error) {
      reportError(error);
    }
  };

  $("#opt-colourblind").addEventListener("change", (event) =>
    persist({ colourBlindPalette: event.target.checked })
  );
  $("#opt-simulated").addEventListener("change", (event) =>
    persist({ forceSimulatedMode: event.target.checked })
  );
  $("#opt-refresh").addEventListener("change", (event) =>
    persist({ refreshIntervalSeconds: Number(event.target.value) })
  );

  $("#btn-open-data").addEventListener("click", () => api.openDataDir().catch(reportError));
}

async function applyDisplaySettings() {
  try {
    const { settings } = await api.getSettings();
    document.body.classList.toggle("colourblind", settings.colourBlindPalette);
    state.refreshSeconds = Math.max(15, settings.refreshIntervalSeconds);
    if (state.timer) startRefresh();
  } catch {
    // Display preferences are a convenience; never block the app on them.
  }
}

boot().catch(reportError);
