/**
 * Turning the program's answers into what is on screen.
 *
 * Nothing here computes money. Every figure arrives already formatted from
 * Rust, and every colour comes from a `direction` the engine decided, so the
 * window and an MCP answer can never disagree about what a number says.
 */

import { $, $$, appendAll, clear, directionClass, el } from "./ui.js";

/* ----------------------------------------------------------------- home */

export function renderGames(games, { onOpen, onDelete }) {
  const list = clear($("#game-list"));

  if (games.length === 0) {
    list.append(
      el("p", {
        class: "empty",
        text: "Aucune partie pour l'instant. Commencez-en une, il n'y a rien à perdre.",
      })
    );
    return;
  }

  for (const game of games) {
    list.append(
      el("button", { class: "game-card", type: "button", onClick: () => onOpen(game.id) }, [
        el("div", { class: "game-card-top" }, [
          el("span", { class: "game-card-name", text: game.playerName }),
          el("span", {
            class: game.byAi ? "badge badge-ai" : "badge",
            text: game.byAi ? "🤖 IA" : "🧑 Personne",
          }),
        ]),
        el("div", { class: "game-card-meta", text: `${game.cash} disponible` }),
        el("div", {
          class: "game-card-meta",
          text: `${game.holdingCount} position(s) · ${game.tradeCount} opération(s) · ${game.updatedAt}`,
        }),
        el("div", { class: "position-actions" }, [
          el("button", {
            type: "button",
            text: "Supprimer",
            onClick: (event) => {
              event.stopPropagation();
              onDelete(game);
            },
          }),
        ]),
      ])
    );
  }
}

/* ------------------------------------------------------------ dashboard */

export function renderDashboard(view, { onBuy, onSell }) {
  $("#total-value").textContent = view.totalValue;

  const delta = $("#total-delta");
  delta.textContent = `${view.totalPnl} (${view.totalPnlPercent})`;
  delta.className = `value-delta ${directionClass(view.direction)}`;

  $("#cash-value").textContent = view.cash;
  $("#invested-value").textContent = view.invested;
  $("#realized-value").textContent = view.realizedPnl;

  $("#observer-pill").hidden = !view.observerMode;
  // In an AI game the human watches; the buy button would be a lie.
  $("#tab-market").disabled = view.observerMode;

  renderSourceNote(view);
  renderGoal(view.goal);
  renderPositions(view, { onBuy, onSell });
}

function renderSourceNote(view) {
  const note = clear($("#source-note"));

  if (view.unpricedSymbols.length > 0) {
    note.append(
      el("span", {
        class: "warn",
        text: `Cours indisponible pour ${view.unpricedSymbols.join(", ")}. `,
      })
    );
  }

  if (view.containsSimulatedPrices) {
    note.append(
      el("span", {
        class: "warn",
        text: "⚠ Certains cours sont simulés — ils ne valent que pour l'exercice. ",
      })
    );
  }

  if (view.sources.length > 0) {
    note.append(document.createTextNode(`Sources : ${view.sources.join(", ")}. `));
  }
  note.append(document.createTextNode(`Mis à jour à ${view.updatedAt}.`));
}

function renderGoal(goal) {
  const ring = $("#goal-ring");
  const detail = $("#goal-detail");

  if (!goal) {
    ring.hidden = true;
    detail.hidden = true;
    return;
  }

  ring.hidden = false;
  detail.hidden = false;

  const circumference = 327;
  const percent = Math.max(0, Math.min(100, goal.progressPercent));
  $("#ring-value").style.strokeDashoffset = String(circumference * (1 - percent / 100));
  $("#goal-percent").textContent = `${Math.round(percent)} %`;
  $("#goal-days").textContent =
    goal.daysRemaining > 0 ? `${goal.daysRemaining} j restants` : "échéance passée";

  appendAll(
    clear(detail),
    el("strong", { text: `${goal.statusLabel} — objectif ${goal.targetAmount}` }),
    document.createTextNode(
      goal.daysRemaining > 0
        ? ` · il reste ${goal.amountRemaining} à gagner en ${goal.daysRemaining} jours.`
        : " · la date limite est passée."
    ),
    goal.requiredReturn
      ? el("div", { text: `Rendement encore nécessaire : ${goal.requiredReturn}.` })
      : null,
    goal.achievedReturn
      ? el("div", { text: `Rendement obtenu jusqu'ici : ${goal.achievedReturn}.` })
      : null
  );
}

function renderPositions(view, { onBuy, onSell }) {
  const grid = clear($("#position-grid"));

  if (view.positions.length === 0) {
    grid.append(
      el("p", {
        class: "empty",
        text: view.observerMode
          ? "L'IA n'a encore rien acheté."
          : "Rien en portefeuille. Passez par l'onglet Marché pour votre premier achat.",
      })
    );
    return;
  }

  for (const position of view.positions) {
    const direction = directionClass(position.direction);

    grid.append(
      el("article", { class: `position is-${direction}` }, [
        el("div", { class: "position-top" }, [
          el("span", { class: "position-symbol", text: position.symbol }),
          el("span", { class: "badge", text: `${Math.round(position.weightPercent)} %` }),
        ]),
        el("div", { class: "position-name", text: position.name }),
        el("div", {
          class: "position-value",
          text: position.marketValue ?? "cours indisponible",
        }),
        position.pnl
          ? el("div", {
              class: `position-meta ${direction}`,
              text: `${position.pnl} (${position.pnlPercent})`,
            })
          : null,
        el("div", {
          class: "position-meta",
          text: `${position.quantity} × ${position.price ?? "—"} · coût moyen ${position.averageCost}`,
        }),
        position.isSimulated
          ? el("div", { class: "sim-flag", text: "cours simulé" })
          : null,
        view.observerMode
          ? null
          : el("div", { class: "position-actions" }, [
              el("button", { type: "button", text: "Acheter", onClick: () => onBuy(position) }),
              el("button", { type: "button", text: "Vendre", onClick: () => onSell(position) }),
            ]),
      ])
    );
  }
}

/* --------------------------------------------------------------- market */

export function renderMarket(rows, { onBuy, readOnly }) {
  const list = clear($("#market-list"));

  if (rows.length === 0) {
    list.append(el("p", { class: "empty", text: "Aucun actif ne correspond." }));
    return;
  }

  for (const row of rows) {
    const direction = directionClass(row.direction);

    list.append(
      el("div", { class: "market-row" }, [
        el("div", { class: "market-id" }, [
          el("div", { class: "market-symbol", text: row.symbol }),
          el("div", { class: "market-name", text: row.name }),
        ]),
        el("div", { class: "market-price" }, [
          el("div", { text: row.price ?? "—" }),
          row.changePercent24h
            ? el("div", { class: `market-change ${direction}`, text: row.changePercent24h })
            : null,
          row.isSimulated ? el("div", { class: "sim-flag", text: "simulé" }) : null,
        ]),
        el("button", {
          class: "market-buy",
          type: "button",
          text: "Acheter",
          disabled: readOnly || !row.price,
          onClick: () => onBuy(row),
        }),
      ])
    );
  }
}

/* -------------------------------------------------------------- history */

export function renderHistory(trades) {
  const list = clear($("#history-list"));

  if (trades.length === 0) {
    list.append(el("p", { class: "empty", text: "Aucune opération pour l'instant." }));
    return;
  }

  for (const trade of trades) {
    list.append(tradeCard(trade));
  }
}

export function tradeCard(trade) {
  return el("article", { class: "trade" }, [
    el("div", { class: "trade-top" }, [
      el("span", { class: `trade-side ${trade.side}`, text: trade.sideLabel }),
      el("strong", { text: trade.symbol }),
      el("span", { class: "market-name", text: trade.name }),
      el("span", { class: "trade-when", text: trade.timestamp }),
    ]),
    el("div", {
      class: "trade-detail",
      text: `${trade.quantity} × ${trade.unitPrice} = ${trade.total}` +
        (trade.fees ? ` · frais ${trade.fees}` : ""),
    }),
    trade.realizedPnl
      ? el("div", {
          class: `trade-detail ${directionClass(trade.direction)}`,
          text: `Résultat réalisé : ${trade.realizedPnl}`,
        })
      : null,
    trade.sourceId
      ? el("div", {
          class: "sim-flag",
          text: trade.wasSimulated
            ? "cours simulé au moment de l'opération"
            : `cours relevé chez ${trade.sourceId}`,
        })
      : null,
    // The whole reason AI mode exists: the history reads as decisions, not rows.
    trade.rationale
      ? el("p", { class: "rationale", text: `« ${trade.rationale} »` })
      : null,
  ]);
}

export function renderAiFeed(trades, visible) {
  const panel = $("#ai-feed");
  panel.hidden = !visible;
  if (!visible) return;

  const list = clear($("#feed-list"));
  const recent = trades.slice(0, 5);

  if (recent.length === 0) {
    list.append(el("li", { class: "empty", text: "L'IA n'a encore rien fait." }));
    return;
  }

  for (const trade of recent) {
    list.append(el("li", {}, [tradeCard(trade)]));
  }
}

/* ------------------------------------------------------------- settings */

export function renderSources(sources) {
  const list = clear($("#source-list"));

  for (const source of sources) {
    const state = source.healthy === null ? "" : source.healthy ? "ok" : "ko";
    const kinds = source.kinds.join(", ");

    list.append(
      el("div", { class: "source" }, [
        el("span", { class: `dot ${state}` }),
        el("div", {}, [
          el("div", { text: source.label }),
          el("div", {
            class: "source-detail",
            text:
              (source.configured ? "" : "clé absente · ") +
              (source.isSimulated ? "cours inventés · " : "") +
              kinds +
              (source.detail ? ` · ${source.detail}` : "") +
              (source.lastUsed ? ` · vu à ${source.lastUsed}` : ""),
          }),
        ]),
        el("span", { class: "badge", text: source.id }),
      ])
    );
  }
}

export function renderKeyForm(configured, { onSave }) {
  const form = clear($("#key-form"));

  const providers = [
    ["coingecko", "CoinGecko (clé Demo gratuite, plus de requêtes par minute)"],
    ["coinmarketcap", "CoinMarketCap (clé gratuite, 15 000 crédits par mois)"],
    ["finnhub", "Finnhub (clé gratuite, actions américaines)"],
  ];

  for (const [id, label] of providers) {
    const input = el("input", {
      type: "password",
      autocomplete: "off",
      placeholder: configured.includes(id) ? "•••••••• (enregistrée)" : "coller la clé ici",
    });

    form.append(
      el("div", { class: "key-row" }, [
        el("label", { class: "field" }, [
          el("span", { class: "field-label", text: label }),
          input,
        ]),
        el("button", {
          class: "ghost",
          type: "button",
          text: "Enregistrer",
          onClick: () => {
            onSave(id, input.value);
            input.value = "";
          },
        }),
      ])
    );
  }
}

/* ------------------------------------------------------------ screens */

export function showScreen(name) {
  for (const screen of $$(".screen")) {
    screen.hidden = screen.dataset.screen !== name;
  }
  window.scrollTo({ top: 0 });
}

export function showTab(name) {
  for (const tab of $$(".tab")) {
    tab.classList.toggle("is-active", tab.dataset.tab === name);
  }
  for (const panel of $$(".tab-panel")) {
    panel.hidden = panel.id !== `panel-${name}`;
  }
}
