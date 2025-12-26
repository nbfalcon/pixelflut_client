# Brainrot
WIDTH=$((3840/8))
HEIGHT=$((2160/8))

CV_WIDTH=1920
CV_HEIGHT=1080

#OX=0
#OY=0
OX=$((CV_WIDTH-WIDTH))
OY=$((CV_HEIGHT-HEIGHT))

GST_PLUGIN_PATH=./target/release gst-launch-1.0 filesrc location='Subway Surfers (2024) - Gameplay [4K 16x9] No Copyright [i0M4ARe9v0Y].webm' ! queue ! decodebin ! videoconvert ! videoscale ! "video/x-raw,format=(string)RGBA,width=$WIDTH,height=$HEIGHT" ! queue  ! rsimage2pixelflut offset-x=$OX offset-y=$OY ! queue ! tcpclientsink host=10.55.1.200 port=1234 sync=false
GST_PLUGIN_PATH=./target/release gst-launch-1.0 filesrc location='Subway Surfers (2024) - Gameplay [4K 16x9] No Copyright [i0M4ARe9v0Y].webm' ! queue ! decodebin ! videoconvert ! videoscale ! "video/x-raw,format=(string)RGBA,width=$WIDTH,height=$HEIGHT" ! queue  ! rsimage2pixelflut offset-x=$OX offset-y=$OY ! queue ! tcpclientsink host=localhost port=4000 sync=false
