#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 5 ]; then
  echo "Usage: $0 <file> <host> <port> <width> <height>"
  exit 1
fi

FILE="$1"
HOST="$2"
PORT="$3"
WIDTH="$4"
HEIGHT="$5"

echo "Streaming full frame:"
echo "  file   = $FILE"
echo "  target = $HOST:$PORT"
echo "  size   = ${WIDTH}x${HEIGHT}"

gst-launch-1.0 -e -vvvv \
  filesrc location="$FILE" ! queue ! decodebin ! queue ! \
  videoconvert ! videoscale ! \
  video/x-raw,format=RGBA,width=$WIDTH,height=$HEIGHT ! \
  rsimage2pixelflut offset-x=840 offset-y=360 ! \
  tcpclientsink host="$HOST" port="$PORT" sync=true
