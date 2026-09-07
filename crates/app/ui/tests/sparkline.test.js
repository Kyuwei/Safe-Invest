import assert from "node:assert/strict";
import { test } from "node:test";

import { areaPath, direction, extent, linePath, valueY } from "../js/sparkline.js";

test("an empty or single-point series is not a curve", () => {
  assert.equal(linePath([], 100, 40), "");
  assert.equal(linePath([5], 100, 40), "");
  assert.equal(areaPath([5], 100, 40), "");
});

test("a rising series ends higher on screen than it starts", () => {
  const path = linePath([1, 2, 3], 100, 40);
  const ys = [...path.matchAll(/[ML]([\d.]+) ([\d.]+)/g)].map((m) => Number(m[2]));
  // SVG's y grows downward: a larger value must have a smaller y.
  assert.ok(ys[0] > ys[ys.length - 1], `y a fini plus bas : ${ys}`);
});

test("a falling series ends lower on screen", () => {
  const path = linePath([3, 2, 1], 100, 40);
  const ys = [...path.matchAll(/[ML]([\d.]+) ([\d.]+)/g)].map((m) => Number(m[2]));
  assert.ok(ys[0] < ys[ys.length - 1]);
});

test("a flat series is drawn down the middle rather than dividing by zero", () => {
  const path = linePath([7, 7, 7], 100, 40);
  const ys = [...path.matchAll(/[ML]([\d.]+) ([\d.]+)/g)].map((m) => Number(m[2]));
  assert.ok(ys.every((y) => Number.isFinite(y)));
  assert.ok(ys.every((y) => Math.abs(y - ys[0]) < 0.001), "une série plate doit être plate");
});

test("the curve spans the full width", () => {
  const path = linePath([1, 5, 3, 9], 120, 40);
  const xs = [...path.matchAll(/[ML]([\d.]+) ([\d.]+)/g)].map((m) => Number(m[1]));
  assert.equal(xs[0], 0);
  assert.equal(xs[xs.length - 1], 120);
});

test("gaps in the data do not produce NaN coordinates", () => {
  const path = linePath([1, Number.NaN, 3, null, 5], 100, 40);
  assert.ok(!path.includes("NaN"), path);
});

test("the filled shape closes along the bottom", () => {
  const area = areaPath([1, 2, 3], 100, 40);
  assert.ok(area.endsWith("Z"), area);
  assert.ok(area.includes(" 40 "), "le remplissage doit descendre jusqu'en bas");
});

test("direction reports the move across the whole window", () => {
  assert.equal(direction([1, 9, 2]), 1);
  assert.equal(direction([9, 1, 8]), -1);
  assert.equal(direction([4, 9, 4]), 0);
  assert.equal(direction([4]), 0);
});

test("extent describes where a series sits", () => {
  const bounds = extent([3, 1, 4, 1, 5]);
  assert.deepEqual(
    { low: bounds.low, high: bounds.high, first: bounds.first, last: bounds.last },
    { low: 1, high: 5, first: 3, last: 5 }
  );
  assert.equal(extent([]), null);
});

test("a reference line lands on the same scale as the curve", () => {
  const bounds = extent([100, 200]);

  // The top of the series is the top of the box, minus the padding; the bottom
  // is the bottom. A value halfway up must land halfway down the canvas.
  assert.equal(valueY(bounds, 200, 100), 2);
  assert.equal(valueY(bounds, 100, 100), 98);
  assert.equal(valueY(bounds, 150, 100), 50);
});

test("a target the curve never reached gets no line at all", () => {
  const bounds = extent([100, 120, 140]);

  // Above and below the series: clamping either to the edge of the box would
  // draw the portfolio brushing a target it was nowhere near.
  assert.equal(valueY(bounds, 250, 100), null);
  assert.equal(valueY(bounds, 10, 100), null);

  // And nothing sensible to draw against a flat series or a missing target.
  assert.equal(valueY(extent([100, 100]), 100, 100), null);
  assert.equal(valueY(bounds, Number.NaN, 100), null);
  assert.equal(valueY(null, 120, 100), null);
});
