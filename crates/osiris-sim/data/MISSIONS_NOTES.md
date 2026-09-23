# Mission data notes

`missions.toml` in this directory holds per-scenario campaign rules (buildings available,
funds, environment, win conditions, mission variables, tutorial/progressive unlocks, and
scripted messages) for Osiris, extracted as **facts** from the Akhenaten reimplementation of
Pharaoh (AGPL; github.com/dalerank/Akhenaten). Nothing in Akhenaten's code
was copied or adapted — only game-design facts (numbers, building rosters, trigger conditions)
were read out of its JS mission scripts and re-expressed as TOML data.

Source: `src/scripts/mission/m_NNN_*.js`, one file per scenario, imported by `src/scripts/missions.js`.
Scenario ids 0..52 are the 53 campaign missions from the original `PharaohData/mission1.pak`,
in the order listed by `PharaohData/campaign.txt` under `[MISSION_NAMES]` — mission id, scenario
id, and the `m_NNN_*` filename number are all the same number. Files `m_128_alexandria.js` through
`m_135_enkomi.js` are non-campaign scripted maps (multiplayer/sandbox/custom scenarios) and are
out of scope for `missions.toml`.

## How mission scripting works (Akhenaten source engine)

Each `m_NNN_*.js` file defines one `missionN { ... }` config block (map file, starting building
roster, funds, environment flags, win_criteria, and a `vars {}` table of mission-local scalar
state) followed by a set of event-handler functions tagged with `[event=...]` or `[es=...]`
(the difference is `es=` fires once even for a saved/resumed game, `event=` is a plain
subscription — not otherwise meaningful for this data file). Handlers read/write `mission.<var>`
state (backed by the `vars {}` block) and call into the `city`/`ui`/`migration` script APIs.

The building roster a mission starts with is the `buildings [...]` array in the config block —
every building listed there is available from the very first day. Two mechanisms sit on top of
that static roster:

- **`city.use_building(BUILDING_X, bool)`** — explicitly enables/disables a building type that
  is *not* in the starting `buildings[]` list (or, in the tutorial missions, explicitly disables
  ones that *are* nominally available but shouldn't be buildable yet). This is the mechanism
  `missions.toml`'s `unlocks[].enable` models.
- **`city.set_advisor_available(ADVISOR_X, 0/1)`** — same idea for advisor panel tabs. Most
  missions just turn every advisor on unconditionally in their `event_mission_start` handler
  (not modeled as an "unlock" — it's static, not progressive). A handful of missions gate a
  specific advisor behind the same beat that gates a building (e.g. mission 1 gates
  `ADVISOR_RELIGION` behind the same gold-mining threshold that unlocks the temple); those are
  captured in `unlocks[].advisors`.

**Only six of the 53 campaign missions actually call `city.use_building(...)`: 0, 1, 2, 3, 4, and
38.** Every other mission places its entire building roster in the starting `buildings[]` array
and never gates anything further — for those, `missions.toml` omits the `unlocks` key entirely.
This was confirmed by grepping `city.use_building(` across every `m_0*.js`/`m_1*.js` file in the
mission directory.

### Tutorial missions 0 and 1

Missions 0 (Nubt) and 1 (Thinis) are the game's onboarding missions and work differently from
every other mission: their starting `buildings[]` list is deliberately tiny (just houses and
roads for mission 0), and the full early-game roster is unlocked in tutorial beats gated on
in-game events rather than on a fixed schedule:

- **Fire** (`event_fire_damage`) unlocks the firehouse the first time a building catches fire.
- **Collapse** (`event_collase_damage` — sic, that's the actual event name in the source engine,
  a spelling artifact of the original game) unlocks the architect's post the first time a
  building collapses.
- **Population threshold** (`event_population_changed`, `ev.value >= N`) unlocks food
  infrastructure (hunting lodge, granary, bazaar) once the settlement reaches a target
  population — this is also the point mission 0 first shows the population advisor.
- **Resource-stored threshold** (`event_granary_resource_added`, granary amount for a specific
  resource `>= N`) unlocks clean water infrastructure once enough game meat has been stockpiled.

Each beat fires its `ui.popup_message(...)` exactly once (guarded by a `mission.tutorial_*_handled`
boolean) and the handler re-applies all unlocked-so-far state again in `event_mission_start` so a
reloaded save reflects prior progress. `mission0`/`mission1`'s `goal_tooltip` function (not
modeled in `missions.toml` — it's UI text logic, not data) picks which of the "#missionN_goal_*"
tooltip strings to show based on the same boolean flags, so the tutorial has a running task list
in the UI in lockstep with the `unlocks[]` sequence in the data file.

### Missions 2, 3, 4: early gameplay unlock chains

Missions 2 (Perwadjyt), 3 (Nekhen) and 4 (Mennefer) continue the same pattern on a smaller scale:
each has 2-3 `unlocks[]` steps, each gated on a resource-stockpile threshold (figs/pottery,
beer, papyrus/bricks) or, in mission 4's case, a housing-tier building count
(`house_spacious_apartment >= 1`) and a disease event. Unlike missions 0/1 these don't disable
anything nominally in the starting roster — they only *add* to it (industry buildings, trade
dock, guild/monument buildings) once the player demonstrates the prerequisite economy exists.

### Mission 38: population-gated monument unlocks

Mission 38 (Thutmose in the Valley) is the one non-tutorial mission with `use_building()` calls:
it withholds the lamp/paint/artisans-guild trio and the small royal tomb until the city's
population crosses 400 and 800 respectively (`event_population_changed`). No popup message
accompanies either beat in the source script. A source comment flags these thresholds as
"provisional until pak dump of OG tutorial beats" — carried into `missions.toml` as a `notes`
caveat on that mission.

## `unlocks[].when` condition vocabulary

- `"start"` — unconditional, applied in the `event_mission_start` handler (used for narrative
  grouping of everything a mission disables-then-never-re-gates at boot, when there's no better
  single condition to hang it on).
- `"population >= N"` — `event_population_changed` / `event_migration_update`, `ev.value >= N`.
- `"resource_stored:<resource> >= N"` — a granary or storage-yard amount for `<resource>`
  (lowercase `RESOURCE_*` name, e.g. `gamemeat`, `figs`, `pottery`, `beer`, `papyrus`, `bricks`)
  reaches `N`, from `event_granary_resource_added` (granary) or `event_warehouse_filled`
  (storage yard, read via `city.yards_stored(RESOURCE_X)`).
- `"building_built:<building_key>"` — first instance of a building type exists, either from
  `event_building_create` (fired once, on construction) or `event_advance_day` polling
  `city.count_active_buildings()`/`count_total_buildings()` > 0.
- `"gold_delivered >= N"` — `city.finance.this_year.income.gold_delivered` (gold sold this year)
  reaches `N`, polled on `event_advance_day` (mission 1's gold-mining tutorial beat).
- `"fire"` — `event_fire_damage`, first fire.
- `"collapse"` — `event_collase_damage`, first building collapse.
- `"disease"` — `event_city_disease`, first disease outbreak.

`messages[].trigger` is documentary free text, not restricted to this vocabulary — it reuses the
vocabulary strings where a message lines up exactly with an `unlocks[]` step, and otherwise gives
a short plain-English description (this matters for the many missions running Pharaoh-favour
resource-request chains and invasion waves — see below).

## `win[]` criteria vocabulary

Every mission sets a `win_criteria {}` block with some subset of these leaves, each
`{enabled, goal}` except the two time-based ones:

- `population`, `culture`, `prosperity`, `housing_count`, `monuments`, `kingdom` — `{enabled, goal}`,
  goal is a plain integer target.
- `housing_level` — `{enabled, goal}`; goal is either a plain integer house-tier index, or (a few
  missions) a `BUILDING_HOUSE_*` constant naming the tier directly — `missions.toml` normalizes
  both forms to the `buildings.toml` house key as a string when the source used the constant form
  (e.g. mission 3's `HOUSE_MODEST_APARTMENT` → `"house_modest_apartment"`), and a bare integer
  otherwise.
- `survival_time`, `time_limit` — `{enabled, years}` (note: `years`, not `goal`).
- Optional `milestone25_year` / `milestone50_year` / `milestone75_year` — appear on the later
  missions with scripted `events[]` timelines; normalized in `missions.toml` to
  `win.milestones = { p25, p50, p75 }`.

A leaf with `enabled = false` still appears in `missions.toml` (as informative dead data showing
what the original design considered and turned off), preserving whatever `goal`/`years` value the
source left on it, if any.

## What's intentionally NOT modeled in `missions.toml`

Scoped out because it's map geometry, trade/economy flavor, or narrative content rather than
sim-affecting mission rules:

- **Pharaoh-favour systems** (`mission_favour_events.js`, and the many per-mission
  `mission<N>_pharaoh_request_*` / `*_recurring_request_*` handlers): scripted recurring resource
  requests (`city.create_good_request`) and multi-wave foreign-army invasions
  (`city.start_foreign_army_invasion`, `mission_pharaoh_favour_invasion_tick`) tied to calendar
  months/years or resource thresholds. These affect reputation (kingdom rating) and trade demand,
  and drive a lot of the popup-message volume in the mid-to-late campaign, but they never gate a
  building or advisor, so they're out of scope for `unlocks[]`. Where a mission runs a
  substantial number of these, its `missions.toml` entry has a one-line `notes` pointer instead of
  an exhaustive message list.
- **Map geometry**: `herd_points_prey/predator`, `entry_point`/`exit_point`,
  `river_entry_point`/`river_exit_point`, `invasion_points_land/sea`, `disembark_points`,
  `fishing_points`.
- **Empire/trade data**: `cities[]`, `empire_routes[]`, `empire_texts[]`, `map_background`,
  `hide_pak_*` flags, `routes[]`.
- **Presentation**: `sounds {}` (briefing/victory voice-over paths), `choice`/`choicescreen`
  fields (campaign-tree branching UI), `selection_subtitle`/`selection_text` (flavor text),
  `burial_provisions[]` (monument-completion resource costs).
- **`events[]`** — structured environmental/economic event timelines (`EVENT_TYPE_CLAY_PIT_FLOOD`,
  demand spikes, etc.) that a handful of later missions declare as data rather than code. These
  are flavor/economy pacing, not building-availability rules.
- **`stages {}`** — appears on a few missions (e.g. mission 5) purely as a documentation grouping
  of which buildings belong to which "tutorial stage"; nothing in the source engine reads it.

## Building-constant mapping notes

Every `BUILDING_*` constant referenced anywhere in the 53 campaign mission scripts maps cleanly to
a `buildings.toml` key by engine id, with one exception:

- **`BUILDING_CLEAR_LAND`** (engine id 9) has no `buildings.toml` entry — it's the terrain-clearing
  tool, not a placeable building, and is omitted from every mission's `buildings = [...]` list
  (missions 0, 1, and 38 reference it in source; all three omit it here with a comment).
- **`BUILDING_HOUSE_VACANT_LOT`** is a source-engine compile-time alias for
  `BUILDING_HOUSE_CRUDE_HUT` (`constexpr e_building_type BUILDING_HOUSE_VACANT_LOT =
  BUILDING_HOUSE_CRUDE_HUT;` in `building_type.h`) — mapped to `"house_crude_hut"` throughout.
