#!/bin/bash
# Renders every monument at every building phase and stitches one contact
# sheet per monument, for eyeballing the art against the original.
#
#   packaging/monument_sheet.sh [OUT_DIR] [KIND ...]
#
# Uses mission 39 (a Valley map with room for nearly everything), or the
# mission named after a monument, and the release binary; build it first with cargo build --release.
set -u
cd "$(dirname "$0")/.."
OUT=${1:-/tmp/monuments}
shift || true
BIN=./target/release/osiris
mkdir -p "$OUT"

# kind:title:name[:mission] (title = text group 198 entry, which scenario slot allows it)
ALL="229:33:small_royal_tomb 234:34:medium_royal_tomb 235:35:large_royal_tomb:40 236:36:grand_royal_tomb:44
210:21:sphinx 222:25:mausoleum 264:24:sun_temple 262:22:small_obelisk 263:23:large_obelisk
258:18:small_mastaba 259:19:medium_mastaba 260:20:large_mastaba
319:8:small_stepped 324:9:medium_stepped 250:10:large_stepped
241:1:small_bent 242:2:medium_bent
243:3:small_mudbrick 244:4:medium_mudbrick 245:5:large_mudbrick
253:13:small_pyramid 254:14:medium_pyramid 255:15:large_pyramid
251:11:stepped_complex 252:12:grand_stepped_complex 246:6:mudbrick_complex 247:7:grand_mudbrick_complex
256:16:pyramid_complex 257:17:grand_pyramid_complex"

want=" $* "
SETUP="allowall; noinvasions; nodisasters; safe; sidebar collapse"
for entry in $ALL; do
    IFS=: read -r kind title name mission <<<"$entry"
    [ $# -gt 0 ] && [[ "$want" != *" $kind "* && "$want" != *" $name "* ]] && continue
    MISSION=${mission:-39}
    # Mausoleum and sun temple want sandstone, obelisks granite, in yards
    # first: eight yards of 3200 units each, on the first spots that don't overlap.
    yards=""
    case $kind in 222|264) stone=30 ;; 262|263) stone=26 ;; *) stone="" ;; esac
    if [ -n "$stone" ]; then
        picked=$($BIN --mission $MISSION --size 320x240 --screenshot /tmp/ms_probe.png --script "$SETUP; spots 72 2000" 2>&1 \
            | grep -o '([0-9]*, [0-9]*)' | tr -d '() ' | awk -F, 'BEGIN { n = 0 } {
                for (i = 0; i < n; i++) { dx = X[i] - $1; dy = Y[i] - $2; if (dx * dx < 9 && dy * dy < 9) next }
                X[n] = $1; Y[n] = $2; n++; print $1 "," $2; if (n == 8) exit }')
        for y in $picked; do yards="$yards; build 72 $y; stock $y $stone 3200"; done
    fi
    base="$SETUP$yards; monuments $title,0,0"
    spot=$($BIN --mission $MISSION --size 320x240 --screenshot /tmp/ms_probe.png --script "$base; spots $kind 1" 2>&1 \
        | sed -n 's/.*nearest \[(\([0-9]*\), \([0-9]*\))\].*/\1,\2/p')
    if [ -z "$spot" ]; then
        echo "$name ($kind): no spot on mission $MISSION" >&2
        continue
    fi
    # Zoom to fit the footprint: an isometric tile is 60 pixels across, and a
    # w x h block of them (w + h) * 30; leave room for tall art.
    zoom=$($BIN --mission $MISSION --size 320x240 --screenshot /tmp/ms_probe.png --script "$base; build $kind $spot; viewkind $kind" 2>&1 \
        | sed -n 's/.*spans \([0-9]*\)x\([0-9]*\) tiles.*/\1 \2/p' | awk '{ z = 820 / (($1 + $2) * 30 + 120); if (z > 1) z = 1; printf "%.2f", z }')
    rm -f "$OUT/${name}"_*.png
    for p in $(seq 0 30); do
        if [ "$kind" -ge 229 ] && [ "$kind" -le 236 ]; then
            [ "$p" -gt 4 ] && break
            phase="tombstage $p $((p * 25))"
        else
            phase="monphase $p 9999"
        fi
        log=$($BIN --mission $MISSION --size 960x720 --screenshot "$OUT/${name}_$(printf %02d "$p").png" \
            --script "$base; build $kind $spot; $phase; viewkind $kind; zoom $zoom; monstatus" 2>&1)
        echo "$log" | grep -qi 'error' && { echo "$name phase $p: $(echo "$log" | grep -i error | head -1)" >&2; break; }
        echo "$log" | grep -q 'finished true' && break
    done
    montage -font /System/Library/Fonts/Supplemental/Arial.ttf -label '%t' "$OUT/${name}"_*.png -tile 4x -geometry 480x360+4+4 -background '#222' -fill white \
        "$OUT/sheet_${name}.png" 2>/dev/null
    echo "$name: $(ls "$OUT/${name}"_*.png | wc -l | tr -d ' ') phases -> $OUT/sheet_${name}.png"
done
