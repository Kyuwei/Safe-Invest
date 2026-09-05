/**
 * Turning the program's answers into what is on screen.
 *
 * Nothing here computes money. Every figure arrives already formatted from
 * Rust, and every colour comes from a `direction` the engine decided, so the
 * window and an MCP answer can never disagree about what a number says.
 */

import { $, $$, appendAll, clear, directionClass, el } from "./ui.js";
import { areaPath, direction as curveDirection, linePath } from "./sparkline.js";

/** Maps a direction to the class that colours it. Exported for the dialogs. */
export function toneClass(way) {
  return directionClass(way);
}

/**
 * The square that stands in for an asset.
 *
 * Coloured by asset class, not by brand: inventing an orange for Bitcoin means
 * inventing a colour for every other symbol too, and the first one we get
 * wrong looks like a mistake about the asset rather than about the palette.
 */
function assetMark(symbol, kind, size = "") {
  return el("span", {
    class: `asset-chip kind-${kind}${size ? ` asset-chip-${size}` : ""}`,
    text: symbol.slice(0, 4),
    "aria-hidden": "true",
  });
}

function kindBadge(kind, label) {
  return el("span", { class: `badge kind-badge kind-${kind}`, text: label });
}

const KIND_LABELS = { crypto: "Crypto", stock: "Action", etf: "ETF", cash: "Liquidités" };

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
      el("div", { class: "game-card" }, [
        el("button", { class: "game-card-open", type: "button", onClick: () => onOpen(game.id) }, [
          el("span", { class: "game-card-top" }, [
            el("span", { class: "game-card-name", text: game.playerName }),
            el("span", {
              class: game.byAi ? "badge badge-ai" : "badge",
              text: game.byAi ? "IA" : "Personne",
            }),
          ]),
          el("span", { class: "game-card-meta", text: `${game.cash} disponible` }),
          el("span", {
            class: "game-card-meta",
            text:
              `${game.holdingCount} position(s) · ${game.tradeCount} opération(s)` +
              ` · ${game.updatedAt}`,
          }),
        ]),
        el("button", {
          class: "game-card-delete",
          type: "button",
          title: "Supprimer cette partie",
          "aria-label": `Supprimer la partie de ${game.playerName}`,
          text: "✕",
          onClick: () => onDelete(game),
        }),
      ])
    );
  }
}

/* ------------------------------------------------------------ dashboard */

export function renderDashboard(view, { onSell, onOpen }) {
  $("#dash-greeting").textContent = view.observerMode
    ? `Partie pilotée par ${view.playerName}`
    : `Bonjour ${view.playerName}`;

  $("#total-value").textContent = view.totalValue;

  const delta = $("#total-delta");
  delta.textContent = `${view.totalPnl} (${view.totalPnlPercent})`;
  delta.className = `value-delta ${directionClass(view.direction)}`;

  // In an AI game the human watches; a buy button would be a lie.
  $("#observer-pill").hidden = !view.observerMode;
  $("#dash-actions").hidden = view.observerMode;
  $("#nav-market").disabled = view.observerMode;

  renderPlayerCard(view);
  renderStats(view);
  renderMission(view.goal);
  renderCurve(view.valueHistory, view.valueHistoryLabel, view.currency);
  renderAllocation(view.allocation);
  fitDashSplit();
  renderPositions(view, { onSell, onOpen });
  renderSourceNote(view);
}

function renderPlayerCard(view) {
  const card = $("#player-card");
  card.hidden = false;
  card.className = `player-card ${view.observerMode ? "is-ai" : "is-human"}`;
  $("#player-card-kind").textContent = view.observerMode ? "JOUEUR IA" : "JOUEUR HUMAIN";
  $("#player-card-name").textContent =
    `${view.playerName} · ${view.positions.length} position(s)`;
  $("#nav-dashboard-label").textContent = view.observerMode ? "Observer l'IA" : "Portefeuille";
}

function renderStats(view) {
  $("#stat-invested").textContent = view.invested;
  $("#stat-invested-note").textContent = `${view.positions.length} position(s)`;

  $("#stat-cash").textContent = view.cash;
  $("#stat-cash-note").textContent = `${percentText(view.cashPercent)} du total`;

  const pnl = $("#stat-pnl");
  pnl.textContent = view.totalPnl;
  pnl.className = `stat-value ${directionClass(view.direction)}`;
  const pnlNote = $("#stat-pnl-note");
  pnlNote.textContent = `${view.totalPnlPercent} depuis le départ`;
  pnlNote.className = `stat-note ${directionClass(view.direction)}`;

  const best = view.bestPosition;
  $("#stat-best").textContent = best ? best.name : "—";
  const bestNote = $("#stat-best-note");
  bestNote.textContent = best ? `${best.symbol} · ${best.pnlPercent}` : "rien en portefeuille";
  bestNote.className = `stat-note ${best ? directionClass(best.direction) : ""}`;
}

/** "22,8 %" — a share of the portfolio, not money, so it is safe to format here. */
function percentText(value) {
  return `${Number(value ?? 0).toLocaleString("fr-FR", { maximumFractionDigits: 1 })} %`;
}

/**
 * The mission banner: the target, how far along, and how long is left.
 *
 * Shown for any game that has a goal, not only an AI one — a person who set
 * themselves a target deserves the same running total.
 */
function renderMission(goal) {
  const banner = $("#mission");

  if (!goal) {
    banner.hidden = true;
    return;
  }

  banner.hidden = false;
  const percent = Math.max(0, Math.min(100, goal.progressPercent));

  $("#mission-line").textContent = `Atteindre ${goal.targetAmount} avant le ${goal.deadline}`;
  $("#mission-status").textContent = goal.statusLabel;
  $("#mission-fill").style.width = `${percent}%`;

  appendAll(
    clear($("#mission-facts")),
    el("span", {}, [
      el("strong", { text: percentText(percent) }),
      ` de l'objectif`,
    ]),
    el("span", {
      text:
        goal.daysRemaining > 0
          ? `${goal.daysRemaining} jour(s) restants`
          : "la date limite est passée",
    }),
    el("span", { text: `reste ${goal.amountRemaining} à gagner` }),
    goal.requiredReturn
      ? el("span", { text: `rythme requis : ${goal.requiredReturn}` })
      : null,
    goal.achievedReturn
      ? el("span", { text: `obtenu jusqu'ici : ${goal.achievedReturn}` })
      : null
  );
}

/**
 * Draws the portfolio's value over time.
 *
 * Hidden until there are at least two readings. A game opened five minutes ago
 * has one point, and a curve drawn through one point would be a decoration
 * pretending to be information.
 */
function renderCurve(values, label, currency) {
  const card = $("#value-curve");
  const points = Array.isArray(values) ? values : [];

  if (points.length < 2) {
    card.hidden = true;
    return;
  }

  card.hidden = false;
  const way = curveDirection(points);
  card.classList.toggle("is-up", way > 0);
  card.classList.toggle("is-down", way < 0);

  $("#curve-line").setAttribute("d", linePath(points, 660, 200));
  $("#curve-area").setAttribute("d", areaPath(points, 660, 200));
  $("#curve-caption").textContent = `${label || "depuis le début"}, en ${currency}`;
}

/** The allocation bar: one band per asset class, plus the cash left over. */
function renderAllocation(slices) {
  const card = $("#alloc-card");
  const bands = Array.isArray(slices) ? slices : [];

  if (bands.length === 0) {
    card.hidden = true;
    return;
  }

  card.hidden = false;
  const bar = clear($("#alloc-bar"));
  const legend = clear($("#alloc-legend"));

  for (const slice of bands) {
    const band = el("span", { class: `alloc-band kind-${slice.kind}` });
    band.style.flexGrow = String(Math.max(slice.percent, 0.5));
    bar.append(band);

    legend.append(
      el("li", {}, [
        el("span", { class: `alloc-dot kind-${slice.kind}` }),
        el("span", { class: "alloc-label", text: slice.label }),
        el("span", { class: "alloc-percent", text: percentText(slice.percent) }),
      ])
    );
  }
}

/**
 * Lets a lone card have the whole row.
 *
 * The split is two tracks wide, and a hidden card still occupies one of them:
 * without this, a game too young to have a curve showed its allocation
 * squeezed into the column meant for the chart beside it.
 */
function fitDashSplit() {
  const split = $(".dash-split");
  const showing = [$("#value-curve"), $("#alloc-card")].filter((card) => !card.hidden);
  split.classList.toggle("is-single", showing.length === 1);
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

/* ------------------------------------------------------------- tables */

function tableHead(columns) {
  return el(
    "div",
    { class: "table-head" },
    columns.map(([label, align]) =>
      el("span", { class: align ? `col-${align}` : null, text: label })
    )
  );
}

function renderPositions(view, { onSell, onOpen }) {
  const table = clear($("#position-table"));

  if (view.positions.length === 0) {
    table.append(
      el("p", {
        class: "empty",
        text: view.observerMode
          ? "L'IA n'a encore rien acheté."
          : "Rien en portefeuille. Passez par Marché pour votre premier achat.",
      })
    );
    return;
  }

  table.append(
    tableHead([
      ["Actif"],
      ["Quantité", "right"],
      ["Prix actuel", "right"],
      ["Valeur", "right"],
      ["24 h", "right"],
      ["Gain / perte", "right"],
      [""],
    ])
  );

  for (const position of view.positions) {
    const tone = directionClass(position.direction);

    table.append(
      el("div", { class: "table-row" }, [
        el("span", { class: "cell-asset" }, [
          assetMark(position.symbol, position.kind),
          el("span", {}, [
            el("span", { class: "cell-name", text: position.name }),
            el("span", {
              class: "cell-sub",
              text: KIND_LABELS[position.kind] ?? position.kind,
            }),
          ]),
        ]),
        el("span", { class: "col-right", text: position.quantity }),
        el("span", { class: "col-right" }, [
          el("span", { text: position.price ?? "—" }),
          position.isSimulated ? el("span", { class: "sim-flag", text: "simulé" }) : null,
        ]),
        el("span", {
          class: "col-right cell-strong",
          text: position.marketValue ?? "cours indisponible",
        }),
        el("span", {
          class: `col-right ${directionClass(position.changeDirection)}`,
          text: position.changePercent24h ?? "—",
        }),
        el("span", { class: `col-right ${tone}` }, [
          el("span", { class: "cell-strong", text: position.pnl ?? "—" }),
          position.pnlPercent ? el("span", { class: "cell-sub", text: position.pnlPercent }) : null,
        ]),
        // Two actions, not three. The sheet is where buying belongs anyway:
        // it shows the price, the month behind it and what the asset even is,
        // which is the whole argument for opening it before spending.
        el("span", { class: "cell-actions" }, [
          el("button", {
            type: "button",
            class: "row-button",
            text: "Fiche",
            onClick: () => onOpen(position),
          }),
          view.observerMode
            ? null
            : el("button", {
                type: "button",
                class: "row-button",
                text: "Vendre",
                onClick: () => onSell(position),
              }),
        ]),
      ])
    );
  }
}

/* --------------------------------------------------------------- market */

export function renderMarket(rows, { onOpen, onBuy, observerMode }) {
  const table = clear($("#market-list"));

  if (rows.length === 0) {
    table.append(el("p", { class: "empty", text: "Aucun actif ne correspond." }));
    return;
  }

  table.append(
    tableHead([["Nom"], ["Classe"], ["Prix", "right"], ["24 h", "right"], [""]])
  );

  for (const row of rows) {
    table.append(
      el("div", { class: "table-row" }, [
        el("span", { class: "cell-asset" }, [
          assetMark(row.symbol, row.kind),
          el("span", {}, [
            el("span", { class: "cell-name", text: row.name }),
            el("span", { class: "cell-sub", text: row.symbol }),
          ]),
        ]),
        el("span", {}, [kindBadge(row.kind, KIND_LABELS[row.kind] ?? row.kind)]),
        el("span", { class: "col-right cell-strong" }, [
          el("span", { text: row.price ?? "—" }),
          row.isSimulated ? el("span", { class: "sim-flag", text: "simulé" }) : null,
        ]),
        el("span", {
          class: `col-right ${directionClass(row.direction)}`,
          text: row.changePercent24h ?? "—",
        }),
        el("span", { class: "cell-actions" }, [
          el("button", {
            type: "button",
            class: "row-button",
            text: "Fiche",
            onClick: () => onOpen(row),
          }),
          observerMode
            ? null
            : el("button", {
                type: "button",
                class: "market-buy",
                text: "Acheter",
                onClick: () => onBuy(row),
              }),
        ]),
      ])
    );
  }
}

/* -------------------------------------------------------------- history */

export function renderHistory(trades, summary) {
  const table = clear($("#history-list"));

  $("#history-summary").textContent = summary ?? "";

  if (trades.length === 0) {
    table.append(el("p", { class: "empty", text: "Aucune opération à afficher." }));
    return;
  }

  table.append(
    tableHead([
      ["Date"],
      ["Type"],
      ["Actif"],
      ["Qté", "right"],
      ["Prix unit.", "right"],
      ["Montant", "right"],
      ["Auteur", "center"],
      ["Justification"],
    ])
  );

  for (const trade of trades) {
    table.append(historyRow(trade));
  }
}

function historyRow(trade) {
  const tone = directionClass(trade.direction);

  return el("div", { class: "table-row" }, [
    el("span", { class: "cell-when", text: trade.timestamp }),
    el("span", {}, [
      el("span", { class: `trade-side ${trade.side}`, text: trade.sideLabel }),
    ]),
    el("span", { class: "cell-asset" }, [
      assetMark(trade.symbol, trade.kind ?? "cash"),
      el("span", {}, [
        el("span", { class: "cell-name", text: trade.name }),
        el("span", { class: "cell-sub", text: trade.symbol }),
      ]),
    ]),
    el("span", { class: "col-right", text: trade.quantity }),
    el("span", { class: "col-right", text: trade.unitPrice }),
    el("span", { class: "col-right" }, [
      el("span", { class: "cell-strong", text: trade.total }),
      trade.realizedPnl
        ? el("span", { class: `cell-sub ${tone}`, text: trade.realizedPnl })
        : null,
    ]),
    el("span", { class: "col-center" }, [
      el("span", {
        class: trade.byAi ? "badge badge-ai" : "badge",
        text: trade.byAi ? "IA" : "Vous",
      }),
    ]),
    // The whole reason AI mode exists: the history reads as decisions, not rows.
    el("span", { class: "cell-rationale" }, [
      trade.rationale
        ? el("span", { class: "rationale", text: trade.rationale })
        : el("span", { class: "cell-sub", text: sourceLine(trade) }),
    ]),
  ]);
}

function sourceLine(trade) {
  if (!trade.sourceId) return "";
  return trade.wasSimulated ? "cours simulé au moment de l'opération" : `cours relevé chez ${trade.sourceId}`;
}

/** The AI's last few moves, in the shape of a log rather than a table. */
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
    list.append(
      el("li", { class: "feed-row" }, [
        el("span", { class: "feed-when", text: trade.timestamp }),
        el("span", { class: `trade-side ${trade.side}`, text: trade.sideLabel }),
        el("span", { class: "feed-body" }, [
          el("span", { class: "feed-headline" }, [
            el("strong", { text: `${trade.quantity} ${trade.symbol}` }),
            ` à ${trade.unitPrice} · ${trade.total}`,
          ]),
          trade.rationale ? el("span", { class: "rationale", text: trade.rationale }) : null,
        ]),
        trade.realizedPnl
          ? el("span", {
              class: `feed-result ${directionClass(trade.direction)}`,
              text: trade.realizedPnl,
            })
          : null,
      ])
    );
  }
}

/* ---------------------------------------------------------- asset sheet */

/** The market facts: where the price came from and when. */
export function renderAssetFacts(view) {
  const list = clear($("#asset-facts"));

  const facts = [
    ["Type", view.kindLabel],
    // Say it once: "simulated (simulé)" reads like a stutter.
    ["Source du cours", view.isSimulated ? "marché simulé" : view.sourceId],
    ["Relevé à", view.quotedAt],
    ["Disponible", view.cash],
    ["Frais par opération", view.feePercent],
  ];

  for (const [label, value] of facts) {
    if (!value) continue;
    list.append(el("div", {}, [el("dt", { text: label }), el("dd", { text: value })]));
  }
}

/** What is already held, if anything is. */
export function renderAssetHolding(view) {
  const panel = $("#asset-holding");
  const list = clear($("#asset-holding-facts"));

  const facts = [
    ["Quantité", view.heldQuantity],
    ["Valeur", view.heldValue],
    ["Coût moyen", view.heldAverageCost],
  ].filter(([, value]) => Boolean(value));

  panel.hidden = facts.length === 0;

  for (const [label, value] of facts) {
    list.append(el("div", {}, [el("dt", { text: label }), el("dd", { text: value })]));
  }
}

/* ------------------------------------------------------------- settings */

export function renderSources(sources) {
  const list = clear($("#source-list"));

  for (const source of sources) {
    const state = source.healthy === null ? "" : source.healthy ? "ok" : "ko";
    const status = source.healthy === null ? "jamais appelée" : source.healthy ? "Répond" : "Muette";

    list.append(
      el("div", { class: "source" }, [
        el("div", { class: "source-body" }, [
          el("div", { class: "source-name" }, [
            el("span", { text: source.label }),
            el("span", { class: "badge", text: source.id }),
          ]),
          el("div", {
            class: "source-detail",
            text:
              (source.configured ? "" : "clé absente · ") +
              (source.isSimulated ? "cours inventés · " : "") +
              source.kinds.join(", ") +
              (source.detail ? ` · ${source.detail}` : "") +
              (source.lastUsed ? ` · vu à ${source.lastUsed}` : ""),
          }),
        ]),
        el("span", { class: `source-status ${state}` }, [
          el("span", { class: `dot ${state}` }),
          el("span", { text: status }),
        ]),
      ])
    );
  }
}

export function renderKeyForm(configured, { onSave }) {
  const form = clear($("#key-form"));

  const providers = [
    ["coingecko", "CoinGecko", "Clé Demo gratuite : plus de requêtes par minute."],
    ["coinmarketcap", "CoinMarketCap", "Clé gratuite : 15 000 crédits par mois."],
    ["finnhub", "Finnhub", "Clé gratuite : actions américaines."],
  ];

  for (const [id, label, note] of providers) {
    const input = el("input", {
      type: "password",
      autocomplete: "off",
      placeholder: configured.includes(id) ? "•••••••• (enregistrée)" : "coller la clé ici",
    });

    form.append(
      el("div", { class: "key-card" }, [
        el("div", { class: "key-card-head" }, [
          el("span", { class: "key-card-name", text: label }),
          configured.includes(id)
            ? el("span", { class: "badge badge-ok", text: "enregistrée" })
            : el("span", { class: "badge", text: "absente" }),
        ]),
        el("p", { class: "field-note", text: note }),
        el("div", { class: "key-row" }, [
          input,
          el("button", {
            class: "ghost",
            type: "button",
            text: "Enregistrer",
            onClick: () => {
              onSave(id, input.value);
              input.value = "";
            },
          }),
        ]),
      ])
    );
  }
}

/** The tool names an AI would be handed, read from the program, never retyped. */
export function renderMcpTools(names) {
  const list = clear($("#mcp-tools"));
  for (const name of names ?? []) {
    list.append(el("code", { class: "tool-chip", text: name }));
  }
}

/* ------------------------------------------------------------ navigation */

export function showScreen(name) {
  for (const screen of $$(".screen")) {
    screen.hidden = screen.dataset.screen !== name;
  }
  window.scrollTo({ top: 0 });
}

export function showTab(name) {
  for (const item of $$(".nav-item[data-tab]")) {
    item.classList.toggle("is-active", item.dataset.tab === name);
  }
  for (const panel of $$(".tab-panel")) {
    panel.hidden = panel.id !== `panel-${name}`;
  }
  $(".workspace")?.scrollTo({ top: 0 });
}
