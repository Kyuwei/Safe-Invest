/**
 * Wiring: which screen is showing, what refreshes it, and what the buttons do.
 */

import { AppError, api, onGameChanged } from "./api.js";
import { $, $$, reportError, toast } from "./ui.js";
import * as screens from "./screens.js";
import { goalPreview } from "./goal.js";
import { areaPath, direction as curveDirection, linePath } from "./sparkline.js";

const state = {
  screen: "home",
  tab: "dashboard",
  /** False when the shell is open with no game — the settings-only case. */
  inGame: false,
  dashboard: null,
  marketKind: "all",
  asset: null,
  history: { trades: [], summary: "", side: "all", search: "" },
  info: null,
  refreshSeconds: 60,
  timer: 0,
};

/* --------------------------------------------------------------- boot */

async function boot() {
  bindNavigation();
  bindNewGameForm();
  bindMarket();
  bindHistoryFilters();
  bindTradeDialog();
  bindAssetDialog();
  bindSettings();

  state.info = await api.appInfo().catch(() => null);
  if (state.info) {
    $("#version-line").textContent =
      `Safe Invest ${state.info.version} · données dans ${state.info.dataDir}` +
      (state.info.demoMode ? " · mode démonstration" : "");
    $("#mcp-config").textContent = mcpConfig(state.info);
    screens.renderMcpTools(state.info.mcpTools);
  }

  await applyDisplaySettings();
  await goHome();

  // An AI trading in the other process must show up here without a refresh.
  await onGameChanged(() => {
    if (state.screen === "shell" && state.inGame) refreshGame({ quiet: true });
    if (state.screen === "home") loadGames();
  });
}

/**
 * The block to paste into an MCP client.
 *
 * Built from this executable's own path rather than a placeholder, because a
 * placeholder is one more thing to get wrong before anything works at all.
 */
function mcpConfig(info) {
  const command = info.exePath ?? "C:\\chemin\\vers\\safe-invest.exe";
  return JSON.stringify(
    { mcpServers: { "safe-invest": { command, args: ["mcp"] } } },
    null,
    2
  );
}

function bindNavigation() {
  $("#btn-settings-home").addEventListener("click", () => enterShell("settings", false));
  $("#btn-how").addEventListener("click", () => $("#how-dialog").showModal());
  $("#how-close").addEventListener("click", () => $("#how-dialog").close());
  $("#nav-quit").addEventListener("click", () => goHome());
  $("#btn-dash-buy").addEventListener("click", () => selectTab("market"));
  $("#btn-end-game").addEventListener("click", endGame);
  $("#btn-summary-new").addEventListener("click", () => goHome());
  $("#btn-summary-history").addEventListener("click", () => selectTab("history"));

  for (const item of $$(".nav-item[data-tab]")) {
    item.addEventListener("click", () => selectTab(item.dataset.tab));
  }
}

function selectTab(name) {
  state.tab = name;
  screens.showTab(name);
  if (name === "market") loadMarket();
  if (name === "history") loadHistory();
  if (name === "settings") loadSettings();
  if (name === "summary") loadSummary();
}

/* --------------------------------------------------------------- home */

async function goHome() {
  stopRefresh();
  state.screen = "home";
  state.inGame = false;
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
      $("#goal-field").hidden = true;
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
 * The arithmetic and the wording live in `goal.js`, where they are tested.
 */
function updateGoalPreview() {
  const form = $("#new-game-form");
  const preview = $("#goal-preview");

  if (form.elements.playerKind.value !== "ai") {
    preview.textContent = "";
    return;
  }

  const number = (value) => Number(String(value).replace(",", "."));
  const deadline = form.elements.deadline.value;

  preview.textContent = goalPreview(
    number(form.elements.startingCash.value),
    number(form.elements.targetAmount.value),
    deadline ? Date.parse(deadline) : Number.NaN
  );
}

/* -------------------------------------------------------------- shell */

/**
 * Opens the sidebar shell.
 *
 * `inGame` is false when the settings are reached from the menu: the shell is
 * the only place the settings live, but the sections that need a portfolio are
 * switched off rather than shown empty.
 */
function enterShell(tab, inGame) {
  state.screen = "shell";
  state.inGame = inGame;
  state.tab = tab;

  for (const id of ["#nav-dashboard", "#nav-market", "#nav-history"]) {
    $(id).disabled = !inGame;
  }
  $("#player-card").hidden = !inGame;

  screens.showScreen("shell");
  screens.showTab(tab);
}

async function enterGame() {
  enterShell("dashboard", true);
  await refreshGame();

  // A finished game opens on its summary: the portfolio behind it can no
  // longer change, so the result is the thing worth showing first.
  if (state.dashboard?.finished) {
    selectTab("summary");
    return;
  }
  startRefresh();
}

/**
 * Stops the game for good, at the value it has right now.
 *
 * Irreversible, and it fixes the number the summary will quote for ever, so it
 * asks first.
 */
async function endGame() {
  const ok = confirm(
    "Terminer la partie maintenant ?\n\n" +
      "Le résultat sera figé à la valeur actuelle du portefeuille et plus aucun ordre " +
      "ne pourra être passé."
  );
  if (!ok) return;

  try {
    state.dashboard = await api.endGame();
    stopRefresh();
    await refreshGame();
    selectTab("summary");
    toast("Partie terminée. Voici le bilan.", "ok");
  } catch (error) {
    reportError(error);
  }
}

async function loadSummary() {
  if (!state.inGame) return;
  try {
    screens.renderSummary(await api.summary());
  } catch (error) {
    reportError(error);
  }
}

async function refreshGame({ quiet = false } = {}) {
  try {
    const view = await api.dashboard();
    state.dashboard = view;
    screens.renderDashboard(view, {
      onSell: openSell,
      onOpen: (position) => openAsset(position.symbol, position.kind),
    });

    if (view.observerMode) {
      const recent = await api.history(5);
      screens.renderAiFeed(recent.trades, true);
    } else {
      screens.renderAiFeed([], false);
    }

    if (state.tab === "history") await loadHistory();
    if (state.tab === "summary") await loadSummary();
  } catch (error) {
    if (!quiet) reportError(error);
  }
}

function startRefresh() {
  stopRefresh();
  state.timer = setInterval(() => {
    if (state.screen === "shell" && state.inGame) refreshGame({ quiet: true });
  }, state.refreshSeconds * 1000);
}

function stopRefresh() {
  clearInterval(state.timer);
  state.timer = 0;
}

/* ------------------------------------------------------------- history */

function bindHistoryFilters() {
  $("#history-search").addEventListener("input", (event) => {
    state.history.search = event.target.value.trim().toLowerCase();
    drawHistory();
  });

  for (const button of $$("#history-side button")) {
    button.addEventListener("click", () => {
      for (const other of $$("#history-side button")) other.classList.remove("is-active");
      button.classList.add("is-active");
      state.history.side = button.dataset.side;
      drawHistory();
    });
  }
}

async function loadHistory() {
  if (!state.inGame) return;
  try {
    const view = await api.history(null);
    state.history.trades = view.trades;
    state.history.summary = view.count
      ? `${view.count} opération(s) · volume échangé ${view.volume}` +
        (view.since ? ` · depuis le ${view.since}` : "")
      : "Aucune opération pour l'instant.";
    drawHistory();
  } catch (error) {
    reportError(error);
  }
}

/**
 * Applies the filters to the rows already fetched.
 *
 * The filtering is over strings the engine formatted, never over money: a
 * filter narrows what is shown, it never recomputes what a row says.
 */
function drawHistory() {
  const { trades, side, search, summary } = state.history;

  const shown = trades.filter((trade) => {
    if (side !== "all" && trade.side !== side) return false;
    if (!search) return true;
    return (
      trade.symbol.toLowerCase().includes(search) || trade.name.toLowerCase().includes(search)
    );
  });

  const suffix =
    shown.length === trades.length ? "" : ` · ${shown.length} affichée(s) après filtrage`;
  screens.renderHistory(shown, summary + suffix);
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
  if (!state.inGame) return;
  try {
    const rows = await api.market($("#market-search").value, state.marketKind);
    screens.renderMarket(rows, {
      observerMode: Boolean(state.dashboard?.observerMode),
      onOpen: (row) => openAsset(row.symbol, row.kind),
      onBuy: (row) =>
        openTrade("buy", {
          symbol: row.symbol,
          name: row.name,
          kind: row.kind,
          price: row.price,
          priceRaw: row.priceRaw,
        }),
    });
  } catch (error) {
    reportError(error);
  }
}

/* ------------------------------------------------------- trade dialog */

const trade = { side: "buy", mode: "amount", asset: null };

/* ---------------------------------------------------------- asset sheet */

/**
 * Opens one asset's page: its price, the shape of the last month, what is
 * already held, and a sentence on what this kind of asset even is.
 */
async function openAsset(symbol, kind) {
  const dialog = $("#asset-dialog");
  try {
    const view = await api.asset(symbol, kind, 30);
    state.asset = view;

    $("#asset-symbol").textContent = view.symbol;
    $("#asset-name").textContent = view.name;
    $("#asset-price").textContent = view.price ?? "cours indisponible";

    const mark = $("#asset-mark");
    mark.className = `asset-chip asset-chip-large kind-${view.kind}`;
    mark.textContent = view.symbol.slice(0, 4);

    $("#asset-kind").textContent = view.kindLabel;
    $("#asset-kind").className = `badge kind-badge kind-${view.kind}`;

    $("#asset-source").textContent = view.quotedAt
      ? `${view.isSimulated ? "Marché simulé" : view.sourceId ?? "source inconnue"} · relevé à ${view.quotedAt}`
      : "";

    const change = $("#asset-change");
    change.textContent = view.changePercent24h ?? "";
    change.className = `value-delta ${screens.toneClass(view.direction)}`;
    change.hidden = !view.changePercent24h;

    drawAssetCurve(view);

    $("#asset-primer").textContent = view.primer;
    screens.renderAssetFacts(view);
    screens.renderAssetHolding(view);

    // In an AI game the window watches, and a finished game trades no more;
    // offering the buttons in either case would be a lie.
    const readOnly = view.observerMode || view.finished;
    $("#asset-buy").hidden = readOnly;
    $("#asset-sell").hidden = readOnly || !view.heldQuantity;

    dialog.showModal();
  } catch (error) {
    reportError(error);
  }
}

function drawAssetCurve(view) {
  const points = Array.isArray(view.history) ? view.history : [];
  const figure = $("#asset-curve");
  const way = curveDirection(points);

  figure.classList.toggle("is-up", way > 0);
  figure.classList.toggle("is-down", way < 0);
  $("#asset-line").setAttribute("d", linePath(points, 620, 200));
  $("#asset-area").setAttribute("d", areaPath(points, 620, 200));

  $("#asset-caption").textContent =
    points.length < 2
      ? "Pas d'historique disponible pour cet actif."
      : `Cours sur ${view.historyDays} jours` +
        (view.periodChange ? ` · ${view.periodChange} sur la période.` : ".");
}

function bindAssetDialog() {
  $("#asset-close").addEventListener("click", () => $("#asset-dialog").close());

  $("#asset-buy").addEventListener("click", () => {
    const view = state.asset;
    $("#asset-dialog").close();
    openTrade("buy", {
      symbol: view.symbol,
      name: view.name,
      kind: view.kind,
      price: view.price,
      priceRaw: view.priceRaw,
    });
  });

  $("#asset-sell").addEventListener("click", () => {
    const view = state.asset;
    $("#asset-dialog").close();
    openTrade("sell", {
      symbol: view.symbol,
      name: view.name,
      kind: view.kind,
      price: view.price,
      priceRaw: view.priceRaw,
      quantity: view.heldQuantity,
    });
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

async function loadSettings() {
  try {
    const { settings, configuredKeys, demoForced } = await api.getSettings();

    $("#opt-colourblind").checked = settings.colourBlindPalette;
    syncRefreshButtons(settings.refreshIntervalSeconds);

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
        await loadSettings();
      },
    });

    screens.renderSources(await api.marketSources());
  } catch (error) {
    reportError(error);
  }
}

function syncRefreshButtons(seconds) {
  for (const button of $$("#opt-refresh button")) {
    button.classList.toggle("is-active", Number(button.dataset.seconds) === seconds);
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

  for (const button of $$("#opt-refresh button")) {
    button.addEventListener("click", () => {
      const seconds = Number(button.dataset.seconds);
      syncRefreshButtons(seconds);
      persist({ refreshIntervalSeconds: seconds });
    });
  }

  $("#btn-open-data").addEventListener("click", () => api.openDataDir().catch(reportError));

  $("#btn-copy-mcp").addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText($("#mcp-config").textContent);
      toast("Configuration copiée. Collez-la dans votre client MCP.", "ok");
    } catch {
      toast("Copie refusée par le système — sélectionnez le bloc à la main.", "error");
    }
  });
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
