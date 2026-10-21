<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Writing style: no AI tells

Applies to every piece of prose a person reads: the README, the `docs/` pages,
the `.claude/` rules, agents, skills, and memory, doc comments, and every commit,
PR, and issue body. It does not touch the vendored specs. It does not loosen the
technical rules; citations, honesty, and the comment budgets in `comments.md`
still apply.

Write plainly, so the text reads like a person wrote it for another person.

## Banned tells

1. **The "not X, but Y" setup.** Do not frame a point as a contrast for effect
   ("it is not a tool, it is an ecosystem"; "rather than", "instead of merely",
   "never simply"). State what the thing is. A contrast is allowed only when the
   reader genuinely holds the wrong belief and the sentence corrects it with
   facts on both sides.
2. **The rule of three.** Do not group adjectives or clauses into triads on a
   beat ("fast, simple, powerful"). A real list keeps its real length; a
   decorative triad gets cut to the one word that matters.
3. **Overused buzzwords.** Avoid delve, robust, elevate, testament, landscape,
   leverage, tapestry, underscore, foster, realm, seamless, empower, unlock,
   journey (as a metaphor), cutting-edge, state-of-the-art, game-changing,
   holistic, synergy, streamline, harness, pivotal. Use the plain word: read,
   strong, improve, shows, area, use.
4. **The em dash habit.** Do not use em dashes to attach explanatory clauses.
   Almost every one is a comma, a period, parentheses, or a colon. This is the
   most common tell, so check every one. A bullet that defines a term uses a
   colon inside the bold (`- **Term:** text`), not a dash. Prefer "16 to 32 GB"
   over an en-dash range in prose. Keep hyphens in flags (`--locked`) and
   compound words.
5. **Vague transitions and filler openings.** Do not open with "In today's
   fast-paced world", "As the landscape evolves", "It is worth noting that", or
   "At the end of the day". Open with the subject of the sentence.
6. **Adverb tics and hedging.** Cut "quietly", "genuinely", "simply", "notably",
   "importantly", "seamlessly" when they add nothing. State the fact.
7. **The TED-talk tone.** No inspirational build-ups, no rhetorical questions for
   effect, no "imagine a world where".
8. **Bold-formatting overuse.** Bold a term once where it is defined, not
   throughout for emphasis.

## How to write instead (Google developer style)

- Address the reader as "you", not "we" or "the user".
- Use active voice. Name who does what: "the server returns a 400", not "a 400
  is returned".
- Use present tense, including for the result of a step.
- Keep sentences short, near 25 words. If a clause can be deleted and the
  sentence still reads, delete it.
- Prefer concrete nouns and numbers to adjectives.

## Scope framing

The project has no fixed version scope. Do not describe scope as "v1", "not for
v1", "deferred", or "out of scope for now". The implementation checklist
(`docs/implementation.md`) is the scope, and release contents are decided during
development. Describe what is built and what is planned as build order, so
"implementation starts with R4B; the server serves every version", never a
version that caps what the project covers.

## Code comments and doc comments

- A comment says why, not what. The code shows what
  (<https://developers.google.com/style/api-reference-comments>).
- No essays in code. The budgets in `comments.md` are the limits.
- Doc comments follow the summary-line convention in `comments.md`. Document
  every public item, concisely.

## Enforcement

Prose has no lint, so this is review-enforced: hold every new doc, rule, PR, and
issue to it. Code comments are bounded by `scripts/checks/comment-style.sh`.

## Sources

- Google developer documentation style guide:
  <https://developers.google.com/style> · voice and tone
  <https://developers.google.com/style/tone> · API reference comments
  <https://developers.google.com/style/api-reference-comments>
- Wikipedia, "Signs of AI writing" (the community tell list).
