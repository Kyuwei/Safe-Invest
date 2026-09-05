using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using SafeInvest.App.Converters;
using Windows.Foundation;

namespace SafeInvest.App.Controls;

/// <summary>
/// A small price curve. Drawn with plain XAML shapes rather than a charting library: the
/// app needs one line and a shaded area, and that is not worth a dependency — nor the
/// risk of one more thing to keep compiling.
///
/// The line takes the app's up/down colour from the overall move, so a glance at the
/// shape and a glance at the colour say the same thing.
/// </summary>
public sealed class SparklineControl : UserControl
{
    private readonly Grid _root = new();
    private readonly Polygon _area = new() { Opacity = 0.18 };
    private readonly Polyline _line = new() { StrokeThickness = 2, StrokeLineJoin = PenLineJoin.Round };

    private IReadOnlyList<decimal> _values = [];

    public SparklineControl()
    {
        _root.Children.Add(_area);
        _root.Children.Add(_line);
        Content = _root;

        MinHeight = 44;
        SizeChanged += (_, _) => Redraw();
    }

    /// <summary>Closing prices, oldest first. Fewer than two points draws nothing.</summary>
    public void SetValues(IReadOnlyList<decimal> values)
    {
        _values = values ?? [];
        Redraw();
    }

    private void Redraw()
    {
        _line.Points.Clear();
        _area.Points.Clear();

        double width = ActualWidth;
        double height = ActualHeight;

        if (_values.Count < 2 || width <= 1 || height <= 1)
        {
            return;
        }

        decimal min = _values.Min();
        decimal max = _values.Max();
        decimal span = max - min;

        // A perfectly flat series would divide by zero; draw it down the middle instead.
        double Normalise(decimal value) => span == 0m ? 0.5d : (double)((value - min) / span);

        double stepX = width / (_values.Count - 1);
        const double padding = 4d;
        double usableHeight = Math.Max(height - (padding * 2), 1d);

        List<Point> linePoints = [];
        for (int i = 0; i < _values.Count; i++)
        {
            double x = i * stepX;
            double y = padding + ((1d - Normalise(_values[i])) * usableHeight);
            linePoints.Add(new Point(x, y));
        }

        foreach (Point point in linePoints)
        {
            _line.Points.Add(point);
            _area.Points.Add(point);
        }

        // Close the area down to the baseline so the fill reads as a volume, not a ribbon.
        _area.Points.Add(new Point(width, height));
        _area.Points.Add(new Point(0, height));

        int direction = Math.Sign(_values[^1] - _values[0]);
        Brush brush = PaletteLookup.Brush(direction switch
        {
            > 0 => "SafeInvestUpBrush",
            < 0 => "SafeInvestDownBrush",
            _ => "SafeInvestFlatBrush",
        });

        _line.Stroke = brush;
        _area.Fill = brush;
    }
}
