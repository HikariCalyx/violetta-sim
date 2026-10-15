/**
 * Round generation lives on the server so the Durable Object can score a click itself.
 * The page is only told the shuffle so it can animate it — it never reports an outcome
 * ("success: true" from a browser would be one line of devtools).
 *
 * NOTE: the current game already draws the special character with open eyes, so the
 * answer is visible to whoever watches the screen. Moving the shuffle here raises the
 * bar to "you must actually play", not to "the answer is hidden".
 */

/** Rounds in one game; the player must be right about every one of them. */
export const ROUNDS_PER_ATTEMPT = 4;

/** One swap phase: `order[character] = slot` (1-based), a permutation of 1..4. */
export interface Round {
  order: number[];
}

export interface RoundSet {
  /** Which character wears the make-up, 0-3. The same one for all rounds of a game. */
  special: number;
  rounds: Round[];
}

function secureRandom(): number {
  const buf = new Uint32Array(1);
  crypto.getRandomValues(buf);
  return buf[0] / 2 ** 32;
}

function permutation(size: number, random: () => number): number[] {
  const slots = Array.from({ length: size }, (_, i) => i + 1);
  for (let i = slots.length - 1; i > 0; i--) {
    const j = Math.floor(random() * (i + 1));
    [slots[i], slots[j]] = [slots[j], slots[i]];
  }
  return slots;
}

export function makeRounds(random: () => number = secureRandom): RoundSet {
  return {
    special: Math.floor(random() * 4),
    rounds: Array.from({ length: ROUNDS_PER_ATTEMPT }, () => ({ order: permutation(4, random) })),
  };
}

/** The slot the player has to click in round `roundIndex`. */
export function answerSlot(set: RoundSet, roundIndex: number): number {
  return set.rounds[roundIndex].order[set.special];
}
