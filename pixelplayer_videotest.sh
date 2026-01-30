#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 4 ]; then
  echo "Usage: $0 <host> <port> <width> <height>"
  exit 1
fi

HOST="$1"
PORT="$2"
WIDTH="$3"
HEIGHT="$4"

echo "Streaming full frame:"
echo "  target = $HOST:$PORT"
echo "  size   = ${WIDTH}x${HEIGHT}"

gst-launch-1.0 -e -vvvv \
  videotestsrc ! "video/x-raw,format=RGBA,width=$WIDTH,height=$HEIGHT" ! queue ! \
  videorate ! 'video/x-raw,framerate=(fraction)60/1' ! \
  rsimage2pixelflut offset-x=0 offset-y=0 partitions=6 ! \
  queue ! pxmultitcpsink hosts="$HOST:$PORT#20" threads=6
