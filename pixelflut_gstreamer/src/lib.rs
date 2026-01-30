#![feature(addr_parse_ascii)]
#![feature(ascii_char)]
#![feature(nonpoison_rwlock)]
#![feature(sync_nonpoison)]
#![feature(nonpoison_mutex)]

use gstreamer::{
    glib::{self, types::StaticType},
    Rank,
};

use crate::{
    pxmultitcpsink::PXMultiTCPSink, rsduplicator::Duplicator, rsimage2pixelflut::PixelflutConvert,
};

pub(crate) mod connectionpool;
pub(crate) mod partitioned_buffer_meta;
mod pxmultitcpsink;
mod rsduplicator;
mod rsimage2pixelflut;
pub(crate) mod simplethreadpool;

fn plugin_init(plugin: &gstreamer::Plugin) -> Result<(), glib::BoolError> {
    gstreamer::Element::register(
        Some(plugin),
        "rsimage2pixelflut",
        Rank::NONE,
        PixelflutConvert::static_type(),
    )?;
    gstreamer::Element::register(
        Some(plugin),
        "pxmultitcpsink",
        Rank::NONE,
        PXMultiTCPSink::static_type(),
    )?;
    gstreamer::Element::register(
        Some(plugin),
        "rsduplicator",
        Rank::NONE,
        Duplicator::static_type(),
    )?;
    Ok(())
}

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
