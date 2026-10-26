# Turn-based Auto-battling Experiments

This is a toy project for experimenting with game strategies in a turn-based combat,
akin to fights in role-playing games like Might & Magic 3/4/5, Eye of the Beholder,
Bard's Tale, etc.

... with the twist of having ✨Artificial✨Intelligence✨ select your turns.

---

This is definitely not rocket science being done here. Expect more minimax than AlphaZero.

## Example outcome

The following is an example of how it may unfold. (Names are generated at random on every run;
the ones below are the fixed names the narration tests use.)

We see the hero party, consisting of _Brull_, as well as the enemy party, consisting of _Ziuon_ and _Molphige_.
_Ziuon_ deals only 5 damage with their stick, but _Molphige_ deals 20 with their fists, which will take
_Brull_ out in one hit. Fights are played in rounds, and within a round the fastest combatant acts first.
_Brull_ is the fastest (speed 6), so the only viable option is to take _Molphige_ out before he can swing.

That is indeed what happens. _Ziuon_ trades one blow, takes a haymaker in return and, acting last in
round 2, sounds the retreat. With nobody left to act in that round, there is no parting shot.
The lines marked _tactician_ explain each choice: what the acting side expects, and the best
alternatives it passed up, with their scores from the initiator's point of view
(`≤`/`≥` mark bounds that alpha-beta pruning left open).

```
⚔  Brull stands against Ziuon and Molphige.

══ Round 1 ══  initiative: Brull(6) → Molphige(5) → Ziuon(3)
Brull jabs Molphige — 10 damage. Molphige is down!
   ↳ tactician: expects they escape (1.5)
       alt: punch Ziuon    → defeat  (≤ -20)
       alt: glare at Ziuon → defeat  (≤ -20)
       +2 more
Ziuon jabs at Brull with a stick — 5 damage.
   ↳ tactician: expects escape (1.5)
       alt: glare at Brull → escape  (≥ 1.9)
       alt: order retreat  → escape  (2)

  Brull     ███████████████░░░░░ 15/20
  Ziuon     ████████████████████ 15/15
  Molphige  ✝ fallen

══ Round 2 ══  initiative: Brull(6) → Ziuon(3)
Brull lands a haymaker on Ziuon — 10 damage.
   ↳ tactician: expects they escape (1.5)
       alt: glare at Ziuon → they escape  (≤ 1)
       alt: order retreat  → escape       (≤ -1)
Ziuon sounds the retreat and flees!
   ↳ tactician: expects escape (1.5)
       alt: whack Brull    → defeat  (10)
       alt: glare at Brull → defeat  (≥ 14)

  Brull     ███████████████░░░░░ 15/20
  Ziuon     ↩ fled
  Molphige  ✝ fallen

🏁 Ziuon fled; Brull holds the field after 2 rounds.
```

A line with search statistics follows at the end. Run it with `task run`; the log plays back live
in a terminal, `task run -- --fast` prints it all at once and `task run -- --no-explain` leaves out
the tactician notes.

## Rules of ~~Engagement~~ the Game

- [x] Two factions are fighting each other and take turns in making moves.
- Within each faction, a party of one or more participants
  is allowed to make a move. Such an action can be,
  - [x] Attacking a single opponent,
  - [ ] Attacking a group of opponents (area effects),
  - [ ] Applying an effect to a party member,
  - [ ] Skip the turn, i.e. do nothing.
- In addition, the faction as a whole can flee.
  - [x] If a faction flees, the opposing faction gets one last turn.
- [ ] Party members can panic and either flee, be paralyzed or attack their own faction.
 
As for actions,

- [x] Some actions are instantaneous (attacking an enemy),
- [ ] Some actions take preparation (e.g. preparing a magic spell),
- [ ] Some actions have requirements (e.g. a potion must exist to be used)

## Things to do

- [x] Regular minimax ([`src/solver.rs`](src/solver.rs)).
- [x] Multiple turns per party ([`src/action_iterator.rs`](src/action_iterator.rs)).
- [x] Implement Alpha-Beta pruning ([`src/value.rs`](src/value.rs)).
- [x] Implement Iterative Deepening.
- [ ] Implement A*-like heuristics.
