/**
 * Turning a list of numbers into a curve.
 *
 * Pure functions with no DOM, so they can be tested by `node --test` alongside
 * the goal arithmetic. The shape a curve gives is the whole point of drawing
 * one, so it is worth being sure the maths is right.
 */

/** Where a series sits, and how flat it is. */
export function extent(values) {
  const clean = values.filter((value) => Number.isFinite(value));
  if (clean.length === 0) return null;

  const low = Math.min(...clean);
  const high = Math.max(...clean);
  return { low, high, first: clean[0], last: clean[clean.length - 1], count: clean.length };
}

/**
 * An SVG path through `values`, drawn to fit a `width` × `height` box.
 *
 * Returns an empty string for fewer than two points: a single reading is not a
 * curve, and drawing a dot where a trend belongs would say something untrue.
 */
export function linePath(values, width, height, padding = 2) {
  const bounds = extent(values);
  if (!bounds || bounds.count < 2) return "";

  return points(values, bounds, width, height, padding)
    .map(([x, y], index) => `${index === 0 ? "M" : "L"}${x.toFixed(2)} ${y.toFixed(2)}`)
    .join(" ");
}

/** The same shape, closed along the bottom, for a soft fill under the line. */
export function areaPath(values, width, height, padding = 2) {
  const bounds = extent(values);
  if (!bounds || bounds.count < 2) return "";

  const plotted = points(values, bounds, width, height, padding);
  const first = plotted[0];
  const last = plotted[plotted.length - 1];

  return (
    plotted.map(([x, y], i) => `${i === 0 ? "M" : "L"}${x.toFixed(2)} ${y.toFixed(2)}`).join(" ") +
    ` L${last[0].toFixed(2)} ${height} L${first[0].toFixed(2)} ${height} Z`
  );
}

/** +1 if the series ended higher than it started, -1 lower, 0 flat. */
export function direction(values) {
  const bounds = extent(values);
  if (!bounds || bounds.count < 2) return 0;
  if (bounds.last > bounds.first) return 1;
  if (bounds.last < bounds.first) return -1;
  return 0;
}

function points(values, bounds, width, height, padding) {
  const clean = values.filter((value) => Number.isFinite(value));
  const span = bounds.high - bounds.low;
  const usable = Math.max(height - padding * 2, 1);

  return clean.map((value, index) => {
    const x = clean.length === 1 ? 0 : (index / (clean.length - 1)) * width;
    // A flat series would divide by zero; draw it down the middle instead.
    const ratio = span === 0 ? 0.5 : (value - bounds.low) / span;
    // SVG's y grows downward, so a higher value must sit closer to the top.
    return [x, padding + (1 - ratio) * usable];
  });
}
