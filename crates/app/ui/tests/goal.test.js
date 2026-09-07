// Run with: node --test crates/app/ui/tests/
//
// Node's own test runner, so the interface gains tests without gaining a
// package.json.

import assert from "node:assert/strict";
import { test } from "node:test";

import { annualisedRate, goalPreview, verdictFor } from "../js/goal.js";

const YEAR = 365.25 * 24 * 3600 * 1000;
const NOW = Date.UTC(2026, 0, 1);

test("doubling in a year is about +100 %", () => {
  const rate = annualisedRate(10000, 20000, NOW + YEAR, NOW);
  assert.ok(Math.abs(rate - 100) < 0.5, `obtenu ${rate}`);
});

test("a modest goal gives a modest rate", () => {
  const rate = annualisedRate(10000, 12000, NOW + 2 * YEAR, NOW);
  assert.ok(Math.abs(rate - 9.54) < 0.1, `obtenu ${rate}`);
});

test("a goal that is already met has no rate to report", () => {
  assert.equal(annualisedRate(10000, 9000, NOW + YEAR, NOW), null);
  assert.equal(annualisedRate(10000, 10000, NOW + YEAR, NOW), null);
});

test("a deadline in the past or minutes away has no rate", () => {
  assert.equal(annualisedRate(10000, 20000, NOW - YEAR, NOW), null);
  assert.equal(annualisedRate(10000, 20000, NOW + 60000, NOW), null);
});

test("each band of the verdict is reachable", () => {
  // The bug this guards: with the bands in the wrong order, "> 40" swallowed
  // everything above it and the strongest warning could never be shown.
  assert.match(verdictFor(5), /raisonnable/);
  assert.match(verdictFor(20), /ambitieux mais jouable/);
  assert.match(verdictFor(60), /très ambitieux/);
  assert.match(verdictFor(9900), /aucun placement réel/);
});

test("the bands do not overlap", () => {
  const verdicts = [5, 20, 60, 9900].map(verdictFor);
  assert.equal(new Set(verdicts).size, 4, "chaque palier doit dire autre chose");
});

test("becoming a millionaire from ten thousand in a year is called out", () => {
  const sentence = goalPreview(10000, 1000000, NOW + YEAR, NOW);
  assert.match(sentence, /par an/);
  assert.match(sentence, /aucun placement réel/);
});

test("an incomplete form says nothing rather than guessing", () => {
  assert.equal(goalPreview(0, 1000, NOW + YEAR, NOW), "");
  assert.equal(goalPreview(1000, 0, NOW + YEAR, NOW), "");
  assert.equal(goalPreview(1000, 2000, Number.NaN, NOW), "");
});

test("a deadline too close says so plainly", () => {
  assert.match(goalPreview(1000, 2000, NOW + 3600000, NOW), /trop proche/);
});
