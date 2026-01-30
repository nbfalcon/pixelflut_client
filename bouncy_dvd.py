import os
import gi

gi.require_version("Gst", "1.0")
from gi.repository import Gst, GLib


class DVDLogo:
    def __init__(self, im_width, im_height, fb_width, fb_height):
        self.im_width = im_width
        self.im_height = im_height
        self.fb_width = fb_width
        self.fb_height = fb_height

        self.x = 0
        self.y = 0
        self.dx = 4
        self.dy = 16

    def get(self):
        return self.x, self.y

    def advance(self):
        self.x += self.dx
        self.y += self.dy

        if self.x + self.im_width >= self.fb_width or self.x <= 0:
            self.dx = -self.dx

        if self.y + self.im_height >= self.fb_height or self.y <= 0:
            self.dy = -self.dy


def setup_bus(pipeline: Gst.Pipeline):
    def on_message(_bus, message):
        t = message.type
        if t == Gst.MessageType.ERROR:
            print("ERROR:", message.parse_error())
        elif t == Gst.MessageType.WARNING:
            print("WARNING:", message.parse_warning())
        elif t == Gst.MessageType.INFO:
            print("INFO:", message.parse_info())

    bus = pipeline.get_bus()
    bus.connect("message", on_message)
    bus.add_signal_watch()


def mk_gstreamer():
    WIDTH = 3840 // 8
    HEIGHT = 2160 // 8
    CV_WIDTH = 1920
    CV_HEIGHT = 1080

    FILE = 'Tresorknacker in Berlin [14230136].mp4'
    HOST = '2001:67c:20a1:1561:e61d:2dff:fe4c:f6e1'
    PORT = 1337
    # FILE = "Subway Surfers (2024) - Gameplay [4K 16x9] No Copyright [i0M4ARe9v0Y].webm"
    # HOST = "10.55.1.200"
    # PORT = 1234

    pipeline = Gst.parse_launch(
        f"uridecodebin uri=https://cdn.c3voc.de/hls/s1/native_hd.m3u8 ! videoconvert ! videoscale ! capsfilter name=caps ! queue ! rsimage2pixelflut name=pixelflut ! queue ! tcpclientsink name=tcp"
    )
    setup_bus(pipeline)
    caps = pipeline.get_by_name('caps')
    # filesrc = pipeline.get_by_name("input")
    pixelflut = pipeline.get_by_name("pixelflut")
    tcp = pipeline.get_by_name("tcp")
    tcp.set_property("host", HOST)
    tcp.set_property("port", PORT)
    as_caps = Gst.Caps.from_string(
        f"video/x-raw,format=(string)RGBA,width=(int){WIDTH},height=(int){HEIGHT}"
    )
    assert as_caps
    caps.set_property('caps', as_caps)

    dvd = DVDLogo(WIDTH, HEIGHT, CV_WIDTH, CV_HEIGHT)

    def on_frame(pad, info, *_user):
        x, y = dvd.get()
        pixelflut.set_property("offset-x", x)
        pixelflut.set_property("offset-y", y)
        dvd.advance()
        return Gst.PadProbeReturn.OK

    pixelflut_sinkpad = pixelflut.get_static_pad("sink")
    pixelflut_sinkpad.add_probe(Gst.PadProbeType.BUFFER, on_frame)
    return pipeline


def main():
    os.environ["GST_PLUGIN_PATH"] = "./target/release"
    Gst.init()

    loop = GLib.MainLoop()
    pipeline = mk_gstreamer()
    pipeline.set_state(Gst.State.PLAYING)
    loop.run()


if __name__ == "__main__":
    main()
