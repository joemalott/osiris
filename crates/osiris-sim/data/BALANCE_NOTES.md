# Balance notes: core algorithms

Reconstructed from the Akhenaten reimplementation (AGPL) and, where noted, confirmed
against a Ghidra decompilation of the original Pharaoh.exe. Numeric constants referenced
here live in `balance.toml`; this file describes the *shape* of each system precisely
enough to reimplement it. Calendar: 50 ticks/day, 16 days/month, 12 months/year.

## Migration and immigrant house assignment

Once per day (tick 23 of the daily switch, right after `population.update_room()` at
tick 22), the migration system computes a signed "pressure" percentage:

1. `percentage = sentiment_table(city_sentiment) + unemployment_table(unemployment_pct)`
   — two independent lookup tables (first matching descending-sorted bucket wins),
   summed.
2. The result is clamped by any registered unemployment-cap range and zeroed if the
   population cap is reached or more than 3 enemies are currently invading.
3. If `percentage > 0`: desired immigration batch = `max_newcomers_per_update * percentage
   / 100`. If `percentage < 0`: desired emigration batch = `max_leftovers_per_update *
   |percentage| / 100` (emigration additionally requires population > 100).
4. The desired batch is added to a running queue (`immigration_queue_size` or
   `emigration_queue_size`). Only when the queue reaches `max_*_amount_per_batch` (4)
   does the game actually spawn one immigrant/emigrant figure group carrying that
   many people; the queue then resets. A 2-tick cooldown prevents immigration and
   emigration from both firing on the same evaluation.

House assignment is **not** distance- or radius-based. `create_immigrants` does a
two-pass linear scan over every house in building-array order:

- Pass 1: houses with more than 8 rooms free and no immigrant already en route
  (and reachable from the map entry point) each absorb `min(remaining, 4)` people.
- Pass 2: any remaining demand is spread to houses with any room at all, same cap
  logic.

Each selected house gets a single immigrant figure that pathfinds from the map's
entry point to that house and adds its people on arrival, provided the house still
has room. Emigration instead sorts houses **ascending by house level** (poorest
first) and pulls up to 4 people per house, so decay pressure preferentially empties
low-tier housing first.

Population also changes yearly (not just via migration): once per year, at the same
tick 23 slot, every house's occupants are aged forward one decennium bucket. Births
are computed as a percentage of the working-age population per age decennium; deaths
are computed from an 11 (health bracket) x 10 (age decennium) percentage table. Both
tables are additive age-census adjustments applied via `add_to_houses`/
`remove_from_houses`, not a per-person simulation.

## Sentiment

City sentiment is the average of a per-house "happiness" value (0-100), recomputed
across all occupied houses every 8 game-days. Below population 300, taxes, wages,
and unemployment contribute 0 and the house happiness floor is a difficulty-based
constant (60 normal). For a fully modeled house, seven contributions are summed and
clamped into [0,100] each cycle:

1. **Taxes**: a flat lookup on the player's tax rate alone (0-25%), independent of
   tax coverage. (This is distinct from `Tax_Sentiment_Model_Normal.txt`'s rate x
   coverage table, which governs a related but separately-tracked effect.)
2. **Wages**: compares city wage to the kingdom average wage; negative differences
   subtract half the gap, positive differences use a small tiered bonus table.
3. **Unemployment**: tiered penalty, worse than -3 above 25% unemployment.
4. **Food**: houses that eat 2+ distinct food types this week get +2; 1 type gets
   +1; 0 types accrues a `days_without_food` counter (capped at 3) and subtracts
   that counter directly. Persistent zero-food (hitting the per-level devolve
   threshold, 3 days) forces an immediate devolve regardless of desirability, and
   any nonzero shortage blocks evolution outright.
5. **Tents/huts penalty**: only applied to tent-tier houses, toggled on/off every
   other update cycle. The tier table used depends on whether the city already has
   Manors, plain Residences, or neither — cities with more housing variety punish
   a high tent percentage less.
6. **Religion coverage** and **7. Monuments**: both gated behind optional Akhenaten
   feature flags (treat as tunable extras, not confirmed original mechanics).

## Labor allocation

Workforce = 60% of the working-age population (ages 20-49 by default; Akhenaten
defaults to 20-59, which its own flag name implies is a change from the original
20-49 cutoff), plebs only.

Labor is allocated **top-down by category**, not by walker/need at the category
level:

1. If total worker demand across all categories ≤ available workers, everyone is
   employed and unemployment is 0.
2. Otherwise, categories with an explicit user-set priority rank (1-9, covering
   Food Production through Military) are filled first, in rank order.
3. Remaining unprioritized categories share workers round-robin, weighted by a
   fixed `default_priority` table (Food Production and Infrastructure/Government
   get the largest per-round share; Entertainment/Education/Religion the smallest).
4. Within a category, individual buildings receive workers proportional to a
   per-building weight based on how much of the city's housing they currently
   cover (`percentage_houses_covered`) — buildings serving more houses are
   staffed first. Water/health buildings use a separate even-distribution pass
   instead of weight-proportional allocation.
5. `unemployment_percentage = workers_unemployed / workers_available * 100`. A
   second, smoothed value (max +5 change per update) feeds government-facing
   advisor text so it doesn't spike instantly.

Labor seekers are the walker mechanism that actually delivers workers to a
building's doorstep in the simulation layer; the category/building allocation
above determines *how many* workers a building is entitled to, and the seeker
walker (dispatched based on a rolling `houses_covered` counter rather than
staffing percentage) is what makes that count materialize as employed people over
time.

## Tax collection

Monthly, in this fixed order: collect taxes, pay wages, pay interest, pay salary
(pharaoh's own stipend). Tax income per house is `population * tax_rate_multiplier`
(the multiplier is a per-house-level constant already in the model data, scaled by
a difficulty money-multiplier table). The amount actually banked each month is only
half of that theoretical tax base times the player's tax rate percentage
(`tax / 2 * tax_percentage / 100`), and only for houses currently covered by a tax
collector's route. Wages paid = `wage_rate * workers_employed / 10 / 12` (a monthly
fraction of an implied annual wage bill). Interest accrues only while the treasury
is negative, at a fixed annual rate divided by 12; the treasury going below -5000
marks the city "out of money." Once a year, a tribute payment is computed from a
population-tiered schedule (skipped entirely if the treasury is non-positive) —
loss years pay a small fixed amount, profit years pay the larger of a fixed floor
or 25% of profit.

## House evolution tick schedule

Every relevant subsystem runs once per day, in this tick order (from `city.cpp`'s
`update_tick` switch):

1. **Tick 1** — city-wide religion/entertainment coverage recomputed
   (`coverage.update`): for each service type (booth, bandstand, pavilion, zoo,
   temples/shrines), coverage% = `min(100, total_service_weight / population * 100)`.
2. **Tick 9** — `house_decay_services`: general per-house service decay pass.
3. **Tick 12** — `house_service_decay_houses_covered`: decrements the rolling
   `houses_covered` counters used to decide when labor-seeker-style walkers should
   be dispatched again.
4. **Tick 22** — `population.update_room()`: recomputes total city housing capacity
   and room-in-houses from every house's `population_room()`.
5. **Tick 23** — migration evaluation (see above), immediately followed by the
   once-yearly births/deaths pass when due.
6. **Tick 24** — evict occupants of any house that has become overcrowded
   (e.g. after a plague or infestation reduces capacity).
7. **Tick 27** — well water range restamped around every well (radius 3).
8. **Tick 28** — water/religion coverage propagated onto individual houses
   (`has_water_access` flags set from the tick-27 stamp; water-carrier-walked
   coverage already lands continuously via the walker service radius).
9. **Tick 31** — `buildings_generate_figure`: every roamer-spawning building checks
   its staffing-based spawn timer and dispatches a new walker if due.
10. **Tick 33** — `avg_coverage.update`: rolling average entertainment/culture
    coverage recomputed.
11. **Tick 35** — `house_service_update_health`: health-service aggregation
    (apothecary/physician presence count) folded into each house.
12. **Tick 36** — `house_service_calculate_culture_aggregates`: raw per-service
    walker-visit fields (each 0-96, decaying by 1/day, refreshed to 96 on a walker
    visit) are aggregated into the four evolve-relevant numbers: entertainment
    (weighted sum of juggler/musician/dancer/senet/zoo fields plus a city-wide
    average term, capped at 100), education (0-3 presence flag from
    school/library/academy), num_gods (count of active temples, or 1 for shrine-only
    access), and health (count of apothecary/physician presence, 0-2).
13. **Tick 38** — desirability recalculated from surrounding building types (per
    the cost/desirability model already in `Pharaoh_Model_Normal.txt`).
14. **Tick 39** — `house_process_evolve`: each house compares its aggregated
    service levels, desirability, and food/water status against its level's
    evolve/devolve thresholds. A house that fails its requirements is marked for
    decay; it only actually drops a level after `devolve_delay` (2) **consecutive**
    daily failures, giving a one-day grace period against transient dips.

Per-house service "coverage" is therefore not a single smooth 0-100 percentage but
20+ independent byte fields (one per service type: booth juggler, bandstand
juggler/musician, pavilion musician/dancer, senet player, zookeeper, magistrate,
bullfighter, school, library, academy, apothecary, dentist, mortuary, physician,
water supply, each of the 5 temples, bazaar access, drunkard). A walker's service
visit sets the relevant field(s) on every house within a 2-tile radius of its
*current* tile (not just its final stop) to the max value (96); the field then
decays by exactly 1 per day until the next visit. This means a single well-timed
walker pass can "bank" nearly three months of coverage.

## Service coverage mechanics

Two distinct coverage systems coexist:

- **Per-house fields** (above) drive individual house evolution eligibility.
- **City-wide percentages** (`coverage.cpp`, tick 1) drive advisor ratings and the
  Culture rating: for booth/bandstand/pavilion/zoo, `coverage% = min(100,
  active_venues * population_covered_per_venue / population * 100)`, where
  `population_covered_per_venue` is a fixed constant per venue type (400 for a
  booth, 700 bandstand, 1200 pavilion, 7500 zoo — confirmed byte-identical against
  the original binary's coverage function). Religion coverage uses the same
  formula shape but with per-building weights of 150 (shrine)/375 (temple)/8000
  (temple complex), doubled for gods the city has taken as patron — also
  Ghidra-confirmed original behavior.

Roamer spawn cadence (firehouse, temples, physician, dentist, schools, artisans
guild, water supply, etc.) is staffing-driven, not fixed: a fully staffed building
spawns a new walker daily (0-day delay), dropping to 1/3/7/15-day delays as
staffing falls through 75/50/25/<25%, and no spawn at all when unstaffed. Labor
seekers are the exception — they're dispatched based on the rolling
`houses_covered` counter crossing the building's configured minimum, independent
of staffing level.

## Fire and collapse

Fire risk only has a chance to accumulate: each tick, a per-building hashed value
must match a per-tick global random value (effectively 1-in-8 odds), and only then
does `fire_risk` increase by the building's base risk value (scaled by a
difficulty multiplier reverse-engineered to match observed original burn times).
Crossing 1000 ignites the building. A firehouse visit is a **full reset** of
`fire_risk` to 0 for every building within its patrol radius — not a partial
reduction — so consistent firehouse coverage effectively makes fire risk
irrelevant. Burning ruins burn for a randomized 120-247 ticks, losing 0-15
ticks of duration per update, and periodically (every 4 ticks in desert climates,
every 8 elsewhere, gated by an additional ~1-in-4 roll) attempt to ignite one
adjacent non-fireproof building in a direction that itself re-rolls once a month.

Collapse risk accumulates unconditionally every tick (no 1-in-8 gate), same
1000-point threshold. An architect's visit is also a full reset to 0 within a
2-tile radius by default (the reduction can be tuned to a partial percentage, but
100% matches confirmed original behavior).

## Granary/bazaar/food distribution

Granaries hold up to 4 food types, gated at capacity bands (full 3200, three-quarter
2400, half 1600, quarter 800) that control accept/get order behavior rather than a
single hard cap — a granary can be configured to stop accepting at any of those
bands. Delivery priority is: accept-mode granaries first, then get-mode granaries,
always avoiding a granary flagged "empty all." Cart-based delivery moves food in
units of 100.

Bazaars stock up to 700 grain / 600 each of the other three food types. A market
buyer walker checks a bazaar's understocked food types (reorder thresholds at 600,
400, 200, 100 units) and non-food goods (150, 100, 50, 25), then walks out to the
nearest supplying granary/storage yard, picks up in 100-unit loads (up to the
gap in stock), and spawns a chained "delivery boy" per load carried back. Once
stocked, bazaar walkers roam roads and directly deposit food/goods onto houses
within a 2-tile radius per step, one food type delivered per pass, up to each
house's `food_storage_multiplier * population` capacity.

Each house consumes food weekly: `population * difficulty-adjusted consumption% /
100`, split across however many distinct food types that house level requires
(0 for the lowest tiers, rising to 1, then 2, then 3 at Palatial tiers), and divided
further by the weeks in the month. Running out of any required food type for
multiple consecutive days both hurts sentiment and can force a devolve.

## Flood cycle and farming

The flood cycle repeats on a 392-cycle "year" (25 ticks/cycle). Each year, a flood's
start cycle is `floor(season * 1.05 + 14.5)`, and it runs through five states
(imminent -> flooding -> inundated -> contracting -> resting -> farmable). Its
`period_length` (how long it lasts) is `quality * floodplain_width * 0.01` cycles —
so higher flood quality and wider floodplains both mean longer floods. Next year's
quality is randomized incrementally (`(previous + random(0-99) + 20) % 100`), so
quality drifts rather than being fully independent year to year.

Floodplain farms grow continuously to a fixed `progress_max` (2000) and, when the
flood becomes imminent (within 28 cycles of the predicted flood start), harvest an
amount scaled by that tile's fertility (`progress * fertility / 100`) rather than a
flat yield — poor soil directly reduces the harvest even at full growth. Meadow
farms (non-floodplain) ignore fertility and flood timing entirely, harvesting on a
fixed per-crop monthly calendar instead. At full growth and full fertility, both
farm types cap out at 800 units (8 cart loads) per harvest; grain farms
additionally produce straw as a secondary output at one-tenth that rate. An Osiris
blessing can double the effective yield multiplier on floodplain farms for
100-149 days.
