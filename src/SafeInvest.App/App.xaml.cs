using Microsoft.UI.Xaml;
using SafeInvest.App.Services;

namespace SafeInvest.App;

/// <summary>
/// Entry point. Builds the service container once, then opens the main window on the
/// start menu.
/// </summary>
public partial class App : Application
{
    private Window? _window;

    public App()
    {
        InitializeComponent();
        UnhandledException += OnUnhandledException;
    }

    /// <summary>
    /// The window everything is hosted in. Named RootWindow rather than MainWindow so the
    /// property does not shadow the MainWindow type inside this class.
    /// </summary>
    public static Window? RootWindow { get; private set; }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        AppServices.Initialize();

        _window = new MainWindow();
        RootWindow = _window;
        _window.Activate();
    }

    /// <summary>
    /// A crash in a background refresh must not take the whole game down with it — the
    /// user could lose an unsaved decision. Log it and keep going.
    /// </summary>
    private void OnUnhandledException(object sender, Microsoft.UI.Xaml.UnhandledExceptionEventArgs e)
    {
        System.Diagnostics.Debug.WriteLine($"[Safe Invest] Exception non gérée : {e.Exception}");
        e.Handled = true;
    }
}
