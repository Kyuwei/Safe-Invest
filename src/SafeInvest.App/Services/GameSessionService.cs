using Microsoft.UI.Dispatching;
using SafeInvest.Core.Engine;
using SafeInvest.Core.Models;
using SafeInvest.Core.Storage;
using SafeInvest.MarketData;

namespace SafeInvest.App.Services;

/// <summary>
/// Holds the game currently on screen and keeps it fresh.
///
/// Two things can move the game on: this app, and the MCP server an AI drives. The store
/// is the meeting point, so as well as refreshing prices on a timer, this service watches
/// the save folder — when the AI trades, the dashboard updates within a moment without
/// the user doing anything. That live view is the whole point of the AI mode.
/// </summary>
internal sealed class GameSessionService : IDisposable
{
    private readonly IGameStore _store;
    private readonly PortfolioEngine _engine;
    private readonly AppSettings _settings;
    private readonly TimeProvider _timeProvider;
    private readonly SemaphoreSlim _refreshGate = new(1, 1);

    private IMarketDataService _marketData;
    private DispatcherQueue? _dispatcher;
    private GameStoreWatcher? _watcher;
    private DispatcherQueueTimer? _timer;
    private DateTimeOffset _lastWriteByThisApp = DateTimeOffset.MinValue;

    public GameSessionService(
        IGameStore store,
        IMarketDataService marketData,
        PortfolioEngine engine,
        AppSettings settings,
        TimeProvider timeProvider)
    {
        _store = store;
        _marketData = marketData;
        _engine = engine;
        _settings = settings;
        _timeProvider = timeProvider;
    }

    /// <summary>Raised on the UI thread whenever the game or its valuation changed.</summary>
    public event EventHandler? Changed;

    /// <summary>Raised when a trade arrives that this app did not make — an AI move.</summary>
    public event EventHandler<Trade>? ExternalTradeObserved;

    public GameSession? Session { get; private set; }

    public PortfolioSnapshot? Snapshot { get; private set; }

    public GoalProgress? Goal { get; private set; }

    public bool IsRefreshing { get; private set; }

    public string? LastError { get; private set; }

    public bool IsAiGame => Session?.PlayerKind == PlayerKind.Ai;

    public string Currency => Session?.Currency ?? _settings.DefaultCurrency;

    public IMarketDataService MarketData => _marketData;

    public IGameStore Store => _store;

    /// <summary>Called after the settings screen rebuilds the provider chain.</summary>
    public void UseMarketData(IMarketDataService marketData) => _marketData = marketData;

    /// <summary>Binds the service to the UI thread and starts watching for outside changes.</summary>
    public void Attach(DispatcherQueue dispatcher)
    {
        _dispatcher = dispatcher;

        _watcher ??= CreateWatcher();

        if (_timer is null)
        {
            _timer = dispatcher.CreateTimer();
            _timer.Interval = TimeSpan.FromSeconds(Math.Clamp(_settings.RefreshIntervalSeconds, 15, 3600));
            _timer.IsRepeating = true;
            _timer.Tick += (_, _) => _ = RefreshAsync();
        }
    }

    public void StartAutoRefresh() => _timer?.Start();

    public void StopAutoRefresh() => _timer?.Stop();

    public async Task<GameSession> CreateAsync(
        string playerName,
        PlayerKind playerKind,
        decimal startingCash,
        string currency,
        decimal feePercent,
        decimal? goalAmount,
        DateTimeOffset? goalDeadline,
        CancellationToken cancellationToken = default)
    {
        GameSession session = GameFactory.Create(
            playerName,
            playerKind,
            startingCash,
            currency,
            feePercent,
            goalAmount,
            goalDeadline,
            _timeProvider);

        await _store.SaveAsync(session, cancellationToken).ConfigureAwait(false);
        await _store.SetCurrentGameAsync(session.Id, cancellationToken).ConfigureAwait(false);

        Session = session;
        _lastWriteByThisApp = _timeProvider.GetUtcNow();

        await RefreshAsync(cancellationToken).ConfigureAwait(false);
        return session;
    }

    public async Task<bool> OpenAsync(Guid id, CancellationToken cancellationToken = default)
    {
        GameSession? session = await _store.LoadAsync(id, cancellationToken).ConfigureAwait(false);
        if (session is null)
        {
            return false;
        }

        Session = session;
        await _store.SetCurrentGameAsync(id, cancellationToken).ConfigureAwait(false);
        await RefreshAsync(cancellationToken).ConfigureAwait(false);
        return true;
    }

    /// <summary>Reopens whatever game was last in play, including one an AI started.</summary>
    public async Task<bool> ResumeCurrentAsync(CancellationToken cancellationToken = default)
    {
        Guid? id = await _store.GetCurrentGameIdAsync(cancellationToken).ConfigureAwait(false);
        return id is not null && await OpenAsync(id.Value, cancellationToken).ConfigureAwait(false);
    }

    public void Close()
    {
        Session = null;
        Snapshot = null;
        Goal = null;
        StopAutoRefresh();
        Notify();
    }

    /// <summary>Reloads the game from disk and re-prices every holding.</summary>
    public async Task RefreshAsync(CancellationToken cancellationToken = default)
    {
        if (Session is null || !await _refreshGate.WaitAsync(0, cancellationToken).ConfigureAwait(false))
        {
            return;
        }

        try
        {
            SetRefreshing(true);

            // Always re-read: the AI may have traded since we last looked.
            GameSession? fresh = await _store.LoadAsync(Session.Id, cancellationToken).ConfigureAwait(false);
            if (fresh is null)
            {
                Close();
                return;
            }

            int knownTrades = Session.Trades.Count;
            Session = fresh;

            IReadOnlyList<Asset> held = [.. fresh.Holdings.Select(h => h.Asset)];
            IReadOnlyDictionary<string, Quote> quotes = held.Count == 0
                ? new Dictionary<string, Quote>()
                : await _marketData.GetQuotesAsync(held, fresh.Currency, cancellationToken).ConfigureAwait(false);

            Snapshot = ValuationService.Create(fresh, quotes, _timeProvider.GetUtcNow());
            Goal = GoalTracker.Evaluate(fresh, Snapshot, _timeProvider.GetUtcNow());
            LastError = null;

            AnnounceNewTrades(fresh, knownTrades);
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            LastError = ex.Message;
        }
        finally
        {
            SetRefreshing(false);
            _refreshGate.Release();
            Notify();
        }
    }

    public async Task<Trade> BuyAsync(
        Asset asset,
        decimal? quantity,
        decimal? amount,
        string? rationale,
        CancellationToken cancellationToken = default)
    {
        GameSession session = RequireSession();
        Quote quote = await RequireQuoteAsync(asset, cancellationToken).ConfigureAwait(false);

        Trade? trade = null;
        await _store.MutateAsync(
            session.Id,
            s => trade = _engine.Buy(s, asset, quote, quantity, amount, rationale),
            cancellationToken).ConfigureAwait(false);

        _lastWriteByThisApp = _timeProvider.GetUtcNow();
        await RefreshAsync(cancellationToken).ConfigureAwait(false);
        return trade!;
    }

    public async Task<Trade> SellAsync(
        Asset asset,
        decimal? quantity,
        decimal? amount,
        bool sellAll,
        string? rationale,
        CancellationToken cancellationToken = default)
    {
        GameSession session = RequireSession();
        Quote quote = await RequireQuoteAsync(asset, cancellationToken).ConfigureAwait(false);

        Trade? trade = null;
        await _store.MutateAsync(
            session.Id,
            s => trade = _engine.Sell(s, asset, quote, quantity, amount, sellAll, rationale),
            cancellationToken).ConfigureAwait(false);

        _lastWriteByThisApp = _timeProvider.GetUtcNow();
        await RefreshAsync(cancellationToken).ConfigureAwait(false);
        return trade!;
    }

    public async Task SetGoalAsync(decimal amount, DateTimeOffset deadline, CancellationToken cancellationToken = default)
    {
        GameSession session = RequireSession();
        decimal reference = Snapshot?.TotalValue ?? session.Cash;

        Goal goal = GameFactory.BuildGoal(amount, deadline, reference, _timeProvider.GetUtcNow());

        await _store.MutateAsync(session.Id, s => s.Goal = goal, cancellationToken).ConfigureAwait(false);
        _lastWriteByThisApp = _timeProvider.GetUtcNow();
        await RefreshAsync(cancellationToken).ConfigureAwait(false);
    }

    public Task<Quote?> QuoteAsync(Asset asset, CancellationToken cancellationToken = default) =>
        _marketData.GetQuoteAsync(asset, Currency, cancellationToken);

    private GameSession RequireSession() =>
        Session ?? throw new InvalidOperationException("Aucune partie n'est ouverte.");

    private async Task<Quote> RequireQuoteAsync(Asset asset, CancellationToken cancellationToken)
    {
        Quote? quote = await _marketData
            .GetQuoteAsync(asset, RequireSession().Currency, cancellationToken)
            .ConfigureAwait(false);

        return quote is null or { Price: <= 0m }
            ? throw new TradeValidationException(
                $"Impossible d'obtenir un cours pour {asset.Symbol} en ce moment. " +
                "Vérifiez l'état des sources dans les Réglages.")
            : quote;
    }

    private GameStoreWatcher CreateWatcher()
    {
        GameStoreWatcher watcher = new(_store.GamesDirectory);
        watcher.GamesChanged += (_, _) =>
        {
            // The watcher fires on a background thread; hop to the UI before touching state.
            if (_dispatcher is not null)
            {
                _dispatcher.TryEnqueue(() => _ = RefreshAsync());
            }
        };
        watcher.Start();
        return watcher;
    }

    /// <summary>
    /// Surfaces trades that appeared without this app making them — that is, moves made by
    /// an AI through MCP — so the dashboard can announce them.
    /// </summary>
    private void AnnounceNewTrades(GameSession fresh, int knownTrades)
    {
        if (fresh.Trades.Count <= knownTrades)
        {
            return;
        }

        // A trade made here was already shown; only report the ones that came from outside.
        bool madeHere = _timeProvider.GetUtcNow() - _lastWriteByThisApp < TimeSpan.FromSeconds(2);
        if (madeHere)
        {
            return;
        }

        foreach (Trade trade in fresh.Trades.Skip(knownTrades))
        {
            ExternalTradeObserved?.Invoke(this, trade);
        }
    }

    private void SetRefreshing(bool refreshing)
    {
        IsRefreshing = refreshing;
        Notify();
    }

    private void Notify()
    {
        if (_dispatcher is null || _dispatcher.HasThreadAccess)
        {
            Changed?.Invoke(this, EventArgs.Empty);
        }
        else
        {
            _dispatcher.TryEnqueue(() => Changed?.Invoke(this, EventArgs.Empty));
        }
    }

    public void Dispose()
    {
        _timer?.Stop();
        _watcher?.Dispose();
        _refreshGate.Dispose();
    }
}
