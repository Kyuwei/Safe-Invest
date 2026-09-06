/**
 * The only way this page talks to the program.
 *
 * Everything goes through Tauri's command channel — there is no `fetch` here,
 * and the capability file grants the window nothing but these commands. A
 * rejected command comes back as `{ message, hint }`, the shape `CommandError`
 * serialises to, so callers can show a sentence and a suggestion.
 */

const bridge = globalThis.__TAURI__;

/** A failure with something useful to say about it. */
export class AppError extends Error {
  constructor(message, hint) {
    super(message);
    this.name = "AppError";
    this.hint = hint ?? null;
  }
}

function normalise(raw) {
  if (raw instanceof AppError) return raw;
  if (raw && typeof raw === "object" && typeof raw.message === "string") {
    return new AppError(raw.message, raw.hint);
  }
  return new AppError(String(raw ?? "Erreur inconnue"));
}

async function call(command, args) {
  if (!bridge) {
    throw new AppError(
      "Cette page doit être ouverte depuis l'application Safe Invest.",
      "Lancez safe-invest.exe plutôt que d'ouvrir le fichier HTML."
    );
  }
  try {
    return await bridge.core.invoke(command, args);
  } catch (raw) {
    throw normalise(raw);
  }
}

export const api = {
  appInfo: () => call("app_info"),

  listGames: () => call("list_games"),
  createGame: (args) => call("create_game", { args }),
  openGame: (gameId) => call("open_game", { gameId }),
  deleteGame: (gameId) => call("delete_game", { gameId }),
  setGoal: (targetAmount, deadline) => call("set_goal", { targetAmount, deadline }),

  dashboard: () => call("dashboard"),
  endGame: () => call("end_game"),
  summary: () => call("summary"),
  history: (limit) => call("history", { limit: limit ?? null }),
  market: (query, kind) => call("market", { query, kind }),
  asset: (symbol, kind, days) => call("asset", { symbol, kind, days: days ?? null }),
  priceHistory: (symbol, kind, days) => call("price_history", { symbol, kind, days }),

  buy: (args) => call("buy", { args }),
  sell: (args) => call("sell", { args }),

  getSettings: () => call("get_settings"),
  saveSettings: (settings) => call("save_settings", { settings }),
  setApiKey: (providerId, key) => call("set_api_key", { providerId, key }),
  marketSources: () => call("market_sources"),
  openDataDir: () => call("open_data_dir"),
};

/**
 * Runs `handler` whenever another process writes to a game file.
 *
 * This is what makes AI mode live: the MCP server trades, the file changes,
 * and the open window redraws without polling anything.
 */
export async function onGameChanged(handler) {
  if (!bridge) return () => {};
  return bridge.event.listen("safe-invest://game-changed", handler);
}
