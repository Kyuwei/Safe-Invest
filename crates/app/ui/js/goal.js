/**
 * What a goal actually demands, in a sentence.
 *
 * This is the only real arithmetic in the interface, and it is the one thing
 * the app says before anyone has played a single order — so it lives in its own
 * module and has tests. Everything else on screen arrives already computed from
 * Rust.
 */

const MILLIS_PER_YEAR = 365.25 * 24 * 3600 * 1000;

/** Under this, the horizon is too short for a yearly rate to mean anything. */
const MIN_YEARS = 0.01;

/**
 * Compound annual growth rate, in percent, or `null` when the question does not
 * have a sensible answer.
 */
export function annualisedRate(start, target, deadlineMs, nowMs) {
  if (!(start > 0) || !(target > start) || !Number.isFinite(deadlineMs)) return null;

  const years = (deadlineMs - nowMs) / MILLIS_PER_YEAR;
  if (!(years > MIN_YEARS)) return null;

  const rate = ((target / start) ** (1 / years) - 1) * 100;
  return Number.isFinite(rate) ? rate : null;
}

/**
 * The plain-language judgement on a yearly rate.
 *
 * Ordered from the most extreme down: the tests of a chained condition are read
 * in order, so putting the "> 40" band first would make "> 100" unreachable.
 */
export function verdictFor(rate) {
  if (rate > 100) return "aucun placement réel ne tient ça, même une année";
  if (rate > 40) return "c'est très ambitieux : aucun placement ne tient ce rythme durablement";
  if (rate > 8) return "c'est ambitieux mais jouable";
  return "c'est raisonnable, proche de ce que fait un marché actions sur longue durée";
}

/** The whole sentence, or an empty string when there is nothing to say yet. */
export function goalPreview(start, target, deadlineMs, nowMs = Date.now()) {
  if (!(start > 0) || !(target > start) || !Number.isFinite(deadlineMs)) return "";

  const rate = annualisedRate(start, target, deadlineMs, nowMs);
  if (rate === null) {
    return "Cette date est trop proche pour que l'objectif ait un sens.";
  }

  const rounded = rate.toFixed(1).replace(".", ",");
  return `Il faudrait environ +${rounded} % par an — ${verdictFor(rate)}.`;
}
