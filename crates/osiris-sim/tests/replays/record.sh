#!/bin/sh
# Records the replay fixtures that tests/replays.rs plays back, with the script
# harness (see osiris-app/src/script.rs, the `record` step). The recordings hold
# campaign missions from the original game, so git ignores them: record them from
# your own copy of the game before a change you want to check, and again after a
# change that alters the simulation on purpose. Run it from the repository root:
#
#   cargo build --release -p osiris-app
#   sh crates/osiris-sim/tests/replays/record.sh [game data dir]
#
# Steps before `record` set the city up and go into the starting save; everything
# after it must be a command (build, road, clear, town, fuzz, ticks, taxrate,
# route, import, export, cheat, ...), or the replay can't repeat it.
set -e
DATA=${1:-${OSIRIS_TEST_DATA:-PharaohData}}
OUT=$(dirname "$0")
run() {
    mission=$1
    name=$2
    script=$3
    ./target/release/osiris --data "$DATA" --mission "$mission" --size 320x240 --screenshot /tmp/osiris-replay-fixture.png \
        --script "$script" 2>&1 | grep '^recorded'
}

# A town on a floodplain (housing, food, water, services from the build menus),
# a water lift, ditches and a farm, through a year and a half and two floods.
run 3 town_flood "allowall; safe; treasury 20000; record $OUT/town_flood.osiris-replay; build 7 59,17; build 8 41,19 59,19; build 8 60,16 60,10; build 100 50,20; build 100 61,11; road 49,23 62,23; road 49,23 49,40; build 199 54,24; town 40 5; ticks 6000; taxrate 11; ticks 9000"

# A town attacked by an enemy army (the mockattack1 cheat), which burns its way in.
run 6 invasion "record $OUT/invasion.osiris-replay; town 60 3; ticks 3000; cheat mockattack1; ticks 5800"

# A small pyramid from site preparation into its raising, with masons, carpenters,
# work camps and stone in the yards.
run 12 monument "safe; globallabor; fullstaff; noinvasions; treasury 100000; monuments 13,0,0; allow 253; build 253 58,40; road 36,52 55,52; road 55,52 74,52; road 36,56 55,56; road 36,52 36,56; build 179 37,57; build 179 39,57; build 177 41,57; build 72 37,53; build 72 40,53; build 72 43,53; build 72 46,53; build 72 49,53; build 72 52,53; build 72 55,53; build 72 58,53; stock 37,53 24 3200; stock 40,53 24 3200; stock 43,53 25 3200; stock 46,53 25 3200; stock 49,53 25 3200; stock 52,53 25 3200; stock 41,57 20 600; record $OUT/monument.osiris-replay; build 199 43,57; build 199 46,57; ticks 10000; taxrate 8; ticks 10000"

# Two land trade routes opened, exports set and luxury goods imported, with more to
# sell than a year's allowances take, so caravans buy and sell in both years (fire and
# collapse off, so the yards outlast the town).
run 12 trade "safe; treasury 20000; globallabor; fullstaff; road 111,141 111,118; build 72 112,125; build 72 112,130; build 72 108,125; stock 112,125 11 2400; stock 112,130 12 3200; stock 108,125 13 3200; trade 19 import 1600; record $OUT/trade.osiris-replay; town 50 3; route 8; route 10; export 11; export 11; export 12; export 12; export 13; export 13; ticks 16000"
