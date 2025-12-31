#!/bin/bash
export GST_PLUGIN_PATH=./target/release
gst-launch-1.0 -vvvvv videotestsrc ! 'video/x-raw,width=1920,height=1080' ! queue ! rsimage2pixelflut ! fakesink sync=false
