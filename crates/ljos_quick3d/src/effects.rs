//! `QtQuick3D.Effects`: effects written in QML over `Effect`, in the module's
//! pack ([`crate::helpers`]). Their shaders are ours, and are resources so an
//! application carries no files for them.

use ljos::prelude::*;

macro_rules! shaders {
    ($($name:literal),* $(,)?) => {
        [$((
            concat!("ljos-quick3d-effects:/", $name),
            include_str!(concat!("../shaders/effects/", $name)),
        )),*]
    };
}

pub(crate) fn register(engine: &Engine) {
    let platform = Platform::of(engine);
    for (url, text) in shaders![
        "hdr-bright.frag",
        "hdr-blur-x.frag",
        "hdr-blur-y.frag",
        "hdr-combine.frag",
        "color-master.frag",
    ] {
        platform.register_resource(url, text.as_bytes());
    }
}
