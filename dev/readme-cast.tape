# The cast rendered into docs/assets/first-hour.svg by dev/termcast.py.
#
# Lines beginning "$ " are typed on screen and run for real; whatever the
# binary prints is what the frame shows. A line beginning "! " is setup: it
# runs and its output is discarded, so the cast can stage a tampered copy
# without showing the plumbing. "# " is a comment, and "> " is a comment
# drawn on screen between commands.
#
# trunnion on PATH is a link to target/debug/trunnion, so the command shown is
# the command that ran.

! rm -rf /tmp/trunnion-cast
$ trunnion ledger init /tmp/trunnion-cast/ledger

> ask the broker to run something destructive
$ trunnion broker call /tmp/trunnion-cast/ledger Bash "rm -rf /"

> the record is checkable without trusting the binary that wrote it
$ trunnion ledger verify /tmp/trunnion-cast/ledger

> now copy the ledger and alter one character of one event
! cp -r /tmp/trunnion-cast/ledger /tmp/trunnion-cast/tampered
! python3 dev/termcast.py --tamper /tmp/trunnion-cast/tampered
$ trunnion ledger verify /tmp/trunnion-cast/tampered
