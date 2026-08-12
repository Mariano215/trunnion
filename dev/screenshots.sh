#!/bin/zsh
# dev/screenshots: the console images in README.md and site/, taken by
# rendering the console rather than by cropping a window.
#
# The three that existed before this script were made by hand in August and
# went stale the day the console was redrawn: the README showed a dark blue
# interface that no longer existed, and so did the published site, because
# dev/build-site.py copies these same files. An image of a product is a claim
# about the product, and a hand-made one has no check behind it and no way to
# be refreshed except somebody remembering.
#
# So this is the same shape as ci/console-render.sh: build a fixture ledger
# with the real binary, serve it with the real binary, and photograph what a
# browser draws. Rerun it after any change to assets/ and commit what changes.
#
# Run from the repository root, after cargo build.
set -e

CHROME=${TRUNNION_CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}
BIN=target/debug/trunnion
OUT=docs/assets

if [ ! -x "$CHROME" ]; then
  echo "no headless browser at \"$CHROME\". Fix: install Google Chrome, or set TRUNNION_CHROME to a Chromium binary that supports --headless --screenshot"
  exit 1
fi
if [ ! -x "$BIN" ]; then
  echo "no trunnion binary at $BIN. Fix: run cargo build before dev/screenshots.sh"
  exit 1
fi

WORK=$(mktemp -d /tmp/trunnion-shots.XXXXXX)
L=$WORK/ledger
TAMPERED=$WORK/tampered
WS_HOME=$WORK/home
SERVER=
BROKEN=
WS=
cleanup() {
  for p in $SERVER $BROKEN $WS; do kill $p 2>/dev/null || true; done
  rm -rf $WORK
}
trap cleanup EXIT

# -- a ledger with something to look at --------------------------------------
#
# The same commands ci/console-render.sh builds its fixture from: a denial
# with a named rule, a sensor verdict and a capability run, and held calls in
# more than one state. A screenshot of an empty console would be honest and
# useless.
echo "clean finding" > $WORK/art.md
$BIN broker call $L Bash "rm -rf /" >/dev/null 2>&1 || true
$BIN orchestrate step $L repo.write docs/proof/fixtures/no-private-key.json $WORK/art.md user:mariano@local >/dev/null
$BIN broker call $L Bash "git push origin main" >/dev/null 2>&1 || true
$BIN broker call $L Bash "git push origin release" >/dev/null 2>&1 || true
$BIN broker call $L Read docs/PLAN.md >/dev/null 2>&1 || true

# One altered event, for the takeover image. Inside a string, so the envelope
# still parses and only the hashes give it away.
cp -R $L $TAMPERED
sed '3s|"id":"|"id":"tampered-|' $L/events.jsonl > $TAMPERED/events.jsonl
if cmp -s $L/events.jsonl $TAMPERED/events.jsonl; then
  echo "the tampered copy is identical to the clean one, so the takeover image would show a healthy console. Fix: the sed above no longer matches the envelope shape"
  exit 1
fi

# The workspace needs a registry. This repository is the project, and the
# registry is thrown away with $WORK so the operator's own is untouched.
TRUNNION_HOME=$WS_HOME $BIN project add . --id trunnion >/dev/null

origin_of() {
  local log=$1 i origin=
  for i in $(seq 1 50); do
    origin=$(sed -n 's|^console at \(http://[0-9.:]*\)/.*|\1|p' $log)
    [ -n "$origin" ] && break
    sleep 0.1
  done
  if [ -z "$origin" ]; then
    echo "the console printed no address in 5s: $(cat $log)"
    exit 1
  fi
  echo $origin
}

$BIN console $L 127.0.0.1:0 > $WORK/clean.log 2>&1 &
SERVER=$!
CLEAN=$(origin_of $WORK/clean.log)
$BIN console $TAMPERED 127.0.0.1:0 > $WORK/broken.log 2>&1 &
BROKEN=$!
BROKEN_ORIGIN=$(origin_of $WORK/broken.log)
TRUNNION_HOME=$WS_HOME $BIN console > $WORK/ws.log 2>&1 &
WS=$!
WS_ORIGIN=$(origin_of $WORK/ws.log)

# -- photograph it -----------------------------------------------------------
#
# The flags are ci/console-render.sh's, plus a window size and a screenshot
# instead of a DOM dump. --hide-scrollbars because a scrollbar in a product
# image is an artifact of the tool, not of the product.
#
# preferredColorScheme=1 forces light. Headless inherits the machine's
# appearance otherwise, so these images came out in the dark theme on a mac in
# dark mode, and the paper ground is the design in
# docs/design/console-a-loadbearing.html. Both themes are real; the one the
# README shows should not depend on who ran the script.
#
# Chrome is backgrounded and killed once the file stops growing, for the same
# reason ci/console-render.sh reads its DOM dump that way: with a page holding
# a five second poll the virtual clock never drains and --screenshot does not
# exit on its own. The first run of this script hung there.
shoot() {
  local name=$1 origin=$2 route=$3 height=${4:-900}
  "$CHROME" \
    --headless=new \
    --disable-gpu \
    --user-data-dir=$WORK/profile-$name \
    --no-first-run \
    --no-default-browser-check \
    --disable-default-apps \
    --disable-background-networking \
    --disable-component-update \
    --disable-client-side-phishing-detection \
    --disable-sync \
    --disable-domain-reliability \
    --safebrowsing-disable-auto-update \
    --metrics-recording-only \
    --disable-breakpad \
    --disable-crash-reporter \
    --no-pings \
    --password-store=basic \
    --use-mock-keychain \
    --host-resolver-rules="MAP * ~NOTFOUND, EXCLUDE 127.0.0.1" \
    --hide-scrollbars \
    --force-device-scale-factor=2 \
    --blink-settings=preferredColorScheme=1 \
    --window-size=1440,$height \
    --virtual-time-budget=6000 \
    --screenshot=$OUT/$name.png \
    "$origin/#/$route" > $WORK/chrome-$name.log 2>&1 &
  # A fixed wait, then kill. Polling the file size was tried first and is
  # worse: the file appears before it is complete, so the loop either breaks
  # mid-write and commits a truncated image or needs a stability counter that
  # is just this sleep with extra steps. --screenshot does not exit on its own
  # against a page holding a five second poll, so something has to end it
  # either way.
  local pid=$!
  sleep ${SHOT_WAIT:-15}
  kill $pid 2>/dev/null || true
  wait $pid 2>/dev/null || true
  if [ ! -s $OUT/$name.png ]; then
    echo "$name.png is empty. Fix: run the chrome command in dev/screenshots.sh by hand against a served console and read $WORK/chrome-$name.log"
    exit 1
  fi
  echo "  $OUT/$name.png  $(du -h $OUT/$name.png | cut -f1)"
}

echo "writing:"
shoot console-workspace $WS_ORIGIN     workspace 1500
shoot console-overview  $CLEAN         overview  1150
shoot console-ledger    $CLEAN         ledger     980
shoot console-tampered  $BROKEN_ORIGIN overview  1050

echo "done. These are committed, so review the diff before pushing: an image is a claim about the product and this is where that claim gets made."
