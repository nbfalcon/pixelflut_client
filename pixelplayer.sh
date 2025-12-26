#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 5 ]; then
  echo "Usage: $0 <file> <host> <port> <width> <height> (got $#)"
  exit 1
fi

FILE="$1"
HOST="$2"
PORT="$3"
WIDTH="$4"
HEIGHT="$5"

HALF_W=$(( WIDTH / 2 ))
HALF_H=$(( HEIGHT / 2 ))

echo "Streaming quad decomposition:"
echo "  file   = $FILE"
echo "  target = $HOST:$PORT"
echo "  size   = ${WIDTH}x${HEIGHT}"
echo "  quads  = ${HALF_W}x${HALF_H}"

gst-launch-1.0 -e -vvvv \
  filesrc location="$FILE" ! decodebin name=d \
  d. ! queue ! videoscale gamma-decode=true ! queue ! videoconvert ! \
      "video/x-raw,format=(string)RGBA,width=$WIDTH,height=$HEIGHT" ! \
      queue ! tee name=t \
  \
  t. ! queue ! \
      videocrop right=$HALF_W bottom=$HALF_H ! \
      rsimage2pixelflut offset-x=0 offset-y=0 ! \
      queue ! tcpclientsink host="$HOST" port="$PORT" sync=false \
  \
  t. ! queue ! \
      videocrop left=$HALF_W bottom=$HALF_H ! \
      rsimage2pixelflut offset-x=$HALF_W offset-y=0 ! \
      queue ! tcpclientsink host="$HOST" port="$PORT" sync=false \
  \
  t. ! queue ! \
      videocrop right=$HALF_W top=$HALF_H ! \
      rsimage2pixelflut offset-x=0 offset-y=$HALF_H ! \
      queue ! tcpclientsink host="$HOST" port="$PORT" sync=false \
  \
  t. ! queue ! \
      videocrop left=$HALF_W top=$HALF_H ! \
      rsimage2pixelflut offset-x=$HALF_W offset-y=$HALF_H ! \
      queue ! tcpclientsink host="$HOST" port="$PORT" sync=false
