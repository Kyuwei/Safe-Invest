using System.Text.Json;
using SafeInvest.Core.Abstractions;
using SafeInvest.Core.Engine;
using SafeInvest.Core.Models;
using SafeInvest.Core.Storage;
using SafeInvest.MarketData;

namespace SafeInvest.Mcp;

/// <summary>
/// Everything the tools need in one place: which game is open, how to price it, and how
/// to serialise an answer. The "current game" is the same pointer the WinUI app reads, so
/// when the AI opens a game the human's screen follows it.
/// </summary>
internal sealed class SafeInvestContext(
    IGameStore store,
    IMarketDataService marketData,
    PortfolioEngine engine,
    TimeProvider timeProvider)
{
    public IGameStore Store => store;

    public IMarketDataService MarketData => marketData;

    public PortfolioEngine Engine => engine;

    public DateTimeOffset Now => timeProvider.GetUtcNow();

    /// <summary>Loads the requested game, or the currently open one when no id is given.</summary>
    public async Task<GameSession> RequireGameAsync(string? gameId, CancellationToken cancellationToken)
    {
        Guid? id = ParseId(gameId);

        if (id is null)
        {
            id = await store.GetCurrentGameIdAsync(cancellationToken).ConfigureAwait(false);

            if (id is null)
            {
                throw new SafeInvestToolException(
                    "Aucune partie ouverte.",
                    "Appelez create_game pour en démarrer une, ou open_game avec un identifiant de list_games.");
            }
        }

        return await store.LoadAsync(id.Value, cancellationToken).ConfigureAwait(false)
            ?? throw new SafeInvestToolException(
                $"Partie {id} introuvable.",
                "Utilisez list_games pour voir les parties disponibles.");
    }

    public static Guid? ParseId(string? gameId)
    {
        if (string.IsNullOrWhiteSpace(gameId))
        {
            return null;
        }

        return Guid.TryParse(gameId, out Guid parsed)
            ? parsed
            : throw new SafeInvestToolException(
                $"« {gameId} » n'est pas un identifiant de partie valide.",
                "Les identifiants viennent de list_games ou de create_game.");
    }

    /// <summary>Prices the holdings and builds the snapshot the dashboard also uses.</summary>
    public async Task<PortfolioSnapshot> SnapshotAsync(GameSession session, CancellationToken cancellationToken)
    {
        IReadOnlyList<Asset> held = [.. session.Holdings.Select(h => h.Asset)];

        IReadOnlyDictionary<string, Quote> quotes = held.Count == 0
            ? new Dictionary<string, Quote>()
            : await marketData.GetQuotesAsync(held, session.Currency, cancellationToken).ConfigureAwait(false);

        return ValuationService.Create(session, quotes, Now);
    }

    public GoalProgress? Goal(GameSession session, PortfolioSnapshot snapshot) =>
        GoalTracker.Evaluate(session, snapshot, Now);

    /// <summary>
    /// Turns a ticker into a full asset: the built-in catalog first (instant, no quota
    /// spent), then a provider search when the symbol is unknown.
    /// </summary>
    public async Task<Asset> ResolveAssetAsync(
        string symbol,
        AssetKind kind,
        CancellationToken cancellationToken)
    {
        if (string.IsNullOrWhiteSpace(symbol))
        {
            throw new SafeInvestToolException("Le symbole est obligatoire.", "Par exemple « BTC » ou « MSFT ».");
        }

        string ticker = Asset.Normalize(symbol);

        if (AssetCatalog.Find(kind, ticker) is { } known)
        {
            return known;
        }

        IReadOnlyList<Asset> found = await marketData
            .SearchAsync(ticker, kind, limit: 10, cancellationToken)
            .ConfigureAwait(false);

        Asset? match = found.FirstOrDefault(a => Asset.Normalize(a.Symbol) == ticker && a.Kind == kind)
                       ?? found.FirstOrDefault(a => a.Kind == kind);

        // Unknown to every source: still usable, the quote step will decide.
        return match ?? new Asset { Symbol = ticker, Name = ticker, Kind = kind };
    }

    public async Task<Quote> RequireQuoteAsync(Asset asset, string currency, CancellationToken cancellationToken)
    {
        Quote? quote = await marketData.GetQuoteAsync(asset, currency, cancellationToken).ConfigureAwait(false);

        return quote is null or { Price: <= 0m }
            ? throw new SafeInvestToolException(
                $"Aucun cours disponible pour {asset.Symbol}.",
                "Vérifiez le symbole avec search_assets, ou consultez get_market_sources.")
            : quote;
    }

    public static AssetKind ParseKind(string? kind, AssetKind fallback = AssetKind.Crypto)
    {
        if (string.IsNullOrWhiteSpace(kind))
        {
            return fallback;
        }

        return kind.Trim().ToLowerInvariant() switch
        {
            "crypto" or "cryptocurrency" or "cryptomonnaie" => AssetKind.Crypto,
            "stock" or "share" or "equity" or "action" => AssetKind.Stock,
            "etf" or "fund" or "fonds" => AssetKind.Etf,
            _ => throw new SafeInvestToolException(
                $"Type d'actif inconnu : « {kind} ».",
                "Valeurs acceptées : crypto, stock, etf."),
        };
    }

    public static PlayerKind ParsePlayerKind(string? kind)
    {
        if (string.IsNullOrWhiteSpace(kind))
        {
            return PlayerKind.Ai;
        }

        return kind.Trim().ToLowerInvariant() switch
        {
            "ai" or "ia" or "bot" or "agent" => PlayerKind.Ai,
            "human" or "humain" or "person" or "personne" => PlayerKind.Human,
            _ => throw new SafeInvestToolException(
                $"Type de joueur inconnu : « {kind} ».",
                "Valeurs acceptées : ai, human."),
        };
    }

    /// <summary>Every tool answers with JSON produced by these options, enums included.</summary>
    public static string Serialize<T>(T value) =>
        JsonSerializer.Serialize(value, SafeInvestJson.Wire);

    /// <summary>
    /// Runs a tool body and turns the failures we expect — a rejected trade, a missing
    /// game, an unreachable price — into a structured answer instead of a raw exception,
    /// so the model gets something it can actually act on.
    /// </summary>
    public static async Task<string> GuardAsync(Func<Task<object>> body)
    {
        try
        {
            return Serialize(await body().ConfigureAwait(false));
        }
        catch (SafeInvestToolException ex)
        {
            return Serialize(new Contracts.ErrorResponse { Error = ex.Message, Hint = ex.Hint });
        }
        catch (TradeValidationException ex)
        {
            return Serialize(new Contracts.ErrorResponse
            {
                Error = ex.Message,
                Hint = "Ajustez la quantité ou le montant, puis réessayez.",
            });
        }
        catch (ArgumentException ex)
        {
            return Serialize(new Contracts.ErrorResponse { Error = ex.Message });
        }
        catch (QuoteProviderException ex)
        {
            return Serialize(new Contracts.ErrorResponse
            {
                Error = ex.Message,
                Hint = "Consultez get_market_sources pour l'état des sources de données.",
            });
        }
        catch (FileNotFoundException ex)
        {
            return Serialize(new Contracts.ErrorResponse
            {
                Error = ex.Message,
                Hint = "Utilisez list_games pour retrouver l'identifiant de la partie.",
            });
        }
    }
}

/// <summary>A failure worth explaining to the caller, with a suggested next step.</summary>
internal sealed class SafeInvestToolException(string message, string? hint = null) : Exception(message)
{
    public string? Hint { get; } = hint;
}
