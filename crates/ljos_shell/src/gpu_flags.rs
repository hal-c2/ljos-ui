//! Which of wgpu's debug instance flags the Android shell keeps.

use egui_wgpu::wgpu::InstanceFlags;

/// The flags a debug build starts with, less the debugging ones nobody
/// asked for. Either flag turns on Vulkan's debug-utils extension, and
/// naming objects through it segfaults in SwiftShader (the emulator's
/// Vulkan) while the device is made, so a debug build never drew a
/// frame there. When `WGPU_VALIDATION` or `WGPU_DEBUG` is set, the flag
/// it names stays as wgpu's own reading of the variable chose it.
pub(crate) fn asked_debug_flags(
    mut flags: InstanceFlags,
    set: impl Fn(&str) -> bool,
) -> InstanceFlags {
    for (name, flag) in [
        ("WGPU_VALIDATION", InstanceFlags::VALIDATION),
        ("WGPU_DEBUG", InstanceFlags::DEBUG),
    ] {
        if !set(name) {
            flags.remove(flag);
        }
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debugging_flags_go_unless_their_variable_is_set() {
        let debug = InstanceFlags::debugging();
        let kept = asked_debug_flags(debug, |_| false);
        assert!(!kept.intersects(InstanceFlags::VALIDATION | InstanceFlags::DEBUG));
        assert_eq!(
            kept,
            debug - InstanceFlags::VALIDATION - InstanceFlags::DEBUG,
            "only the debugging flags go"
        );
        let validation = asked_debug_flags(debug, |name| name == "WGPU_VALIDATION");
        assert!(validation.contains(InstanceFlags::VALIDATION));
        assert!(!validation.contains(InstanceFlags::DEBUG));
        let both = asked_debug_flags(debug, |_| true);
        assert_eq!(both, debug);
    }
}
