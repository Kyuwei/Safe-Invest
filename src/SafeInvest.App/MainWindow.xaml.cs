using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media.Animation;
using SafeInvest.App.Views;

namespace SafeInvest.App;

/// <summary>
/// The single window. It hosts a frame that swaps between the start menu, the new-game
/// form and the in-game shell.
/// </summary>
public sealed partial class MainWindow : Window
{
    public MainWindow()
    {
        InitializeComponent();

        Title = "Safe Invest — investir sans risque, pour de vrai";
        ExtendsContentIntoTitleBar = false;

        AppWindow.Resize(new Windows.Graphics.SizeInt32(1360, 900));

        RootFrame.Navigate(typeof(StartPage), null, new EntranceNavigationTransitionInfo());
    }

    /// <summary>Lets pages navigate without each of them reaching for the visual tree.</summary>
    public Frame Navigation => RootFrame;
}
