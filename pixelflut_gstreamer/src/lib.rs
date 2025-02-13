pub mod blit_image;
pub mod pixelflut_builder;
pub mod pixelflut_gstreamer;

use pixelflut_gstreamer::plugin_init;

gstreamer::plugin_define!(
    pixelflut,
    env!("CARGO_PKG_DESCRIPTION"),
    plugin_init,
    concat!(env!("CARGO_PKG_VERSION"), "-", env!("COMMIT_ID")),
    "MIT/X11",
    env!("CARGO_PKG_NAME"),
    env!("CARGO_PKG_NAME"),
    env!("CARGO_PKG_REPOSITORY"),
    env!("BUILD_REL_DATE")
);
