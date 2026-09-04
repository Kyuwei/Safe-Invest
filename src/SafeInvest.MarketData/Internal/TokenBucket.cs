namespace SafeInvest.MarketData.Internal;

/// <summary>
/// Keeps us inside a provider's free-tier rate limit. CoinGecko's keyless tier allows a
/// handful of calls a minute, and going over earns a 429 for everyone using the app.
/// </summary>
internal sealed class TokenBucket(int capacity, TimeSpan refillWindow, TimeProvider? timeProvider = null)
{
    private readonly TimeProvider _clock = timeProvider ?? TimeProvider.System;
    private readonly Lock _gate = new();
    private readonly Queue<DateTimeOffset> _issued = new();

    /// <summary>Takes a token if one is free. Returns false rather than blocking.</summary>
    public bool TryTake()
    {
        DateTimeOffset now = _clock.GetUtcNow();

        lock (_gate)
        {
            while (_issued.Count > 0 && now - _issued.Peek() >= refillWindow)
            {
                _issued.Dequeue();
            }

            if (_issued.Count >= capacity)
            {
                return false;
            }

            _issued.Enqueue(now);
            return true;
        }
    }

    /// <summary>How long until a token frees up. <see cref="TimeSpan.Zero"/> when one is available.</summary>
    public TimeSpan TimeUntilNextToken()
    {
        DateTimeOffset now = _clock.GetUtcNow();

        lock (_gate)
        {
            if (_issued.Count < capacity)
            {
                return TimeSpan.Zero;
            }

            TimeSpan wait = refillWindow - (now - _issued.Peek());
            return wait > TimeSpan.Zero ? wait : TimeSpan.Zero;
        }
    }
}
