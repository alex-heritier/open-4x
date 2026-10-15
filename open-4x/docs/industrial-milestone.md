# Japan–Manchuria playable milestone

Start the desktop campaign with `cargo run -p fourx-client -- --nation japan`.
The intended playtest is an hour commanding Japan: develop production, bring colonial
resources home, take Russian Manchuria, and respond to pressure from China. Chinese
aggression is a weighted campaign roll, not a scripted event or guaranteed declaration.

## Controls

- **Treasury & market** opens stockpiles, fixed-price trades, loans and the weekly budget.
  Hover that button for the income/upkeep breakdown in the status strip.
- Open a city to build a **Factory** or, on the coast, a **Port**. Buildings use city
  production and compete with unit recruitment. Factories operate automatically.
- Select a unit and hold **X while right-clicking** to move or attack with its stack.
  The context menu also offers **Order whole stack**. Different movement allowances can
  leave slower members behind; their standing orders continue next week.
- The unit action bar offers **Load** for land units sharing a square with a transport,
  and **Unload** for a passenger or loaded transport in port. Cargo arriving by sea
  requires a built Port. Ordinary troops retain the existing naval landing rules.
- **Dispatches** opens the dated campaign log. Demands also open the relevant leader's
  window, with accept/refuse choices. Diplomacy offers peace and 25-gold payments.
- **End Turn** or Space advances seven calendar days. Existing production, income and
  movement rates retain their per-turn values. There is no twelfth-turn boom.

## Goods and industry

Six goods are held nationally: Coal, Iron, Food, Cotton, Steel and Supplies.
Coal and iron map resources produce the corresponding good; wheat, corn, rice and
livestock produce Food; cotton produces Cotton. Other resources retain their tile
bonuses without becoming additional commodities.

Each qualifying owned, city-claimed tile produces one good every four weeks. Home
cities deposit goods directly. Colonies create capturable cargo carts; carts with the
same good waiting at the same city accumulate their cargo rather than proliferating.
An unembarked cart entering a home city deposits its entire cargo into its current
owner's stockpile and disappears.

Scenario cities can specify `"home": false`. Home ownership is fixed at the scenario
start, survives capture, and is restored by liberation; capturing a foreign home city
makes it a colony for its occupier. The bundled AoI scenario explicitly marks colonial
holdings, including Japanese Korea and Taiwan and Russian Manchuria. New settlements
count as home only when connected by a land route to a nearby home city (12 tiles).

Every Factory can make up to two of each recipe per week:

- 1 Coal + 1 Iron → 1 Steel.
- 1 Food + 1 Cotton → 1 Supplies.

Factories stop when an input is exhausted. Artillery and ships require Steel to start
recruitment; combat infantry and cavalry require Supplies. Starting a project reserves
its goods once; changing to another project refunds the unused reservation. Civilian
workers and pioneers do not require manufactured goods. Cargo cannot be recruited.

Fixed market buy prices are Coal 40, Iron 40, Food 20, Cotton 30, Steel 160 and Supplies 100
gold. Sales pay 75% of the buy price, rounded down. The market has unlimited liquidity
in this initial implementation; supply and demand pricing is deferred.

## Treasury

Default construction costs are 100 production for a Factory and 60 for a Port.
Units cost 4 gold weekly; buildings cost 8; cargo has no upkeep. Loans supply 200 gold,
charge 2% weekly interest rounded up, and are capped at three outstanding loans.
Repayment pays off one principal or the remaining debt, whichever is smaller.

A negative treasury triggers forced sales: buildings first, then units ordered by
their construction cost. Sales pay 50% of construction cost. If no saleable assets
remain, the treasury defaults to zero. The AI uses these same prices, costs and loans,
and recruits within its income. Threatened or insufficiently defended cities prioritize
their garrison over new economic buildings.

## Stack battles

**The entire battle resolves in one attack command in the current campaign turn.**
Combat rounds never advance the date. The battle panel is playback of an already
resolved result, and can be skipped without changing the result.

Fresh, mobile attackers on the selected unit's tile are sorted by attack; defenders
by defense. Width is the smaller force, capped at three on mountains or in jungle.
Each active lane rolls one ordinary Civ3 combat round at a time. Defensive bombardment,
hit points, terrain/fortification modifiers and promotions retain their existing rules.
There is no retreat and no back row in a stack battle.

A losing attacker is replaced from the fresh reserve. A winner has finished attacking
for the turn; another fresh attacker takes the next defender. Surviving defenders may
fight again. Combat ends when no fresh attackers remain or all defenders are defeated.
Only after clearing the tile does the surviving attacking stack advance, capturing
noncombatants and the city. Naval stacks cannot capture land.

## Diplomacy and AI

Computer nations roll for war against neighbors sharing a territorial border. Local
strength within eight tiles of that border, an opponent's existing wars, and refusal
of a demand change the odds. A weaker AI can still declare war; superiority increases
its chance. The human nation never declares war automatically.

A stronger neighbor can demand 10% of treasury gold (at least 1), or one available good
when gold is scarce, with a 12-week cooldown per pair. AI nations roll for acceptance.
War weariness grows weekly, by two for combat unit losses and by eight for city losses.
An AI accepts peace when its weariness exceeds the proposer's by five, or after twenty
weeks without a city changing hands. Each keeps its occupation. A twelve-week truce
blocks automatic war rolls after peace.

The AI builds factories when inputs are available, ports for shipping, and transports
for colonial cargo. It sends carts over land where possible, otherwise through colonial
ports to home ports, and assigns nearby spare defenders as escorts. Routes and searches
are bounded; transport choices are intentionally simple in this first pass.

## Content and saves

The base pack sets all numeric constants above in `pack.json` under `rules.economy`; see
`EconomicRules` in `crates/sim/src/industry.rs` for the supported keys and validation.
`enabled` defaults to false for custom packs; the base pack enables physical goods.
Industrial output remains a statistic but cannot win the base campaign. Conquest is
the remaining campaign victory condition.

Protocol version is 6 and save format is 5. Earlier saves are rejected because their
calendar and economic state do not describe this campaign. New saves include goods,
cargo, construction, buildings, debt, demands, war weariness and the RNG.

Cargo art is original procedural bitmap art generated by `python art/build.py cargo`.
For reproducible UI captures, `--panel treasury` or `--panel dispatches` opens that
panel at startup. `--replay-battle --load SAVE` replays a saved stack-battle result.
