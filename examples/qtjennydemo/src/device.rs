//! The typed core of the demo's `BackEnd`: the device settings the
//! QML shows (a volume, a brightness, two wake locks) and the
//! services it asks for (vibration, a notification).
//!
//! What the device can do is a value, [`Capabilities`], not a
//! platform guess: a refused request comes back as a
//! [`DeviceError`] and nothing pretends it happened. The desktop
//! has none of the Android services the demo is about, so
//! [`Device::desktop`] has no capability at all.

/// Which of the demo's services a device offers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// The right to write system settings (brightness).
    pub system_settings: bool,
    /// A vibrator motor.
    pub vibrator: bool,
    /// A notification service.
    pub notifications: bool,
    /// A power manager that can hold the device awake.
    pub power_manager: bool,
}

/// Why a request was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DeviceError {
    /// Brightness is a system setting and the right to write one
    /// was not granted.
    #[error("writing system settings is not permitted on this device")]
    SettingsNotWritable,
    /// There is no vibrator.
    #[error("this device cannot vibrate")]
    NoVibrator,
    /// There is no notification service.
    #[error("this device has no notification service")]
    NoNotifications,
    /// There is no power manager to hold a wake lock.
    #[error("this device has no power manager to hold a wake lock")]
    NoPowerManager,
}

/// A wake lock the device holds. At most one is held: taking one
/// releases the other.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WakeLock {
    #[default]
    None,
    /// Screen and CPU stay on.
    Full,
    /// Only the CPU stays on.
    Partial,
}

/// Brightness runs 0 to 255, as the QML's slider says.
pub const BRIGHTNESS_MAX: i32 = 255;

/// The device the demo controls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    capabilities: Capabilities,
    min_volume: i32,
    max_volume: i32,
    volume: i32,
    brightness: i32,
    wake: WakeLock,
    vibrations: u32,
    notifications: u32,
}

impl Device {
    /// A device with the given services and volume range.
    pub fn new(capabilities: Capabilities, min_volume: i32, max_volume: i32) -> Self {
        let max_volume = max_volume.max(min_volume);
        Device {
            capabilities,
            min_volume,
            max_volume,
            volume: min_volume,
            brightness: 0,
            wake: WakeLock::None,
            vibrations: 0,
            notifications: 0,
        }
    }

    /// A desktop: none of the Android services, and no audio stream
    /// to range a volume over (0 to 0).
    pub fn desktop() -> Self {
        Device::new(Capabilities::default(), 0, 0)
    }

    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    pub fn min_volume(&self) -> i32 {
        self.min_volume
    }

    pub fn max_volume(&self) -> i32 {
        self.max_volume
    }

    pub fn volume(&self) -> i32 {
        self.volume
    }

    pub fn brightness(&self) -> i32 {
        self.brightness
    }

    pub fn wake_lock(&self) -> WakeLock {
        self.wake
    }

    /// How many times the device was made to vibrate.
    pub fn vibrations(&self) -> u32 {
        self.vibrations
    }

    /// How many notifications it posted.
    pub fn notifications(&self) -> u32 {
        self.notifications
    }

    /// Sets the volume, kept inside the range. Returns the volume
    /// now in effect.
    pub fn set_volume(&mut self, volume: i32) -> i32 {
        self.volume = volume.clamp(self.min_volume, self.max_volume);
        self.volume
    }

    /// Sets the brightness, kept inside 0 to 255. Refused without
    /// the right to write system settings.
    pub fn set_brightness(&mut self, brightness: i32) -> Result<i32, DeviceError> {
        if !self.capabilities.system_settings {
            return Err(DeviceError::SettingsNotWritable);
        }
        self.brightness = brightness.clamp(0, BRIGHTNESS_MAX);
        Ok(self.brightness)
    }

    pub fn vibrate(&mut self) -> Result<(), DeviceError> {
        if !self.capabilities.vibrator {
            return Err(DeviceError::NoVibrator);
        }
        self.vibrations += 1;
        Ok(())
    }

    pub fn notify(&mut self) -> Result<(), DeviceError> {
        if !self.capabilities.notifications {
            return Err(DeviceError::NoNotifications);
        }
        self.notifications += 1;
        Ok(())
    }

    /// Holds `lock`, releasing whichever was held.
    pub fn hold(&mut self, lock: WakeLock) -> Result<(), DeviceError> {
        if !self.capabilities.power_manager {
            return Err(DeviceError::NoPowerManager);
        }
        self.wake = lock;
        Ok(())
    }

    /// Releases `lock` if it is the one held. Releasing what is not
    /// held is not an error: the QML disables on every uncheck.
    pub fn release(&mut self, lock: WakeLock) {
        if self.wake == lock {
            self.wake = WakeLock::None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phone() -> Device {
        Device::new(
            Capabilities {
                system_settings: true,
                vibrator: true,
                notifications: true,
                power_manager: true,
            },
            0,
            15,
        )
    }

    #[test]
    fn a_desktop_refuses_every_service() {
        let mut d = Device::desktop();
        assert_eq!(d.set_brightness(100), Err(DeviceError::SettingsNotWritable));
        assert_eq!(d.vibrate(), Err(DeviceError::NoVibrator));
        assert_eq!(d.notify(), Err(DeviceError::NoNotifications));
        assert_eq!(d.hold(WakeLock::Full), Err(DeviceError::NoPowerManager));
        assert_eq!(
            (d.brightness(), d.vibrations(), d.wake_lock()),
            (0, 0, WakeLock::None)
        );
    }

    #[test]
    fn a_desktop_volume_has_no_range() {
        let mut d = Device::desktop();
        assert_eq!((d.min_volume(), d.max_volume()), (0, 0));
        assert_eq!(d.set_volume(9), 0);
    }

    #[test]
    fn volume_and_brightness_stay_in_range() {
        let mut d = phone();
        assert_eq!(d.set_volume(40), 15);
        assert_eq!(d.set_volume(-3), 0);
        assert_eq!(d.set_brightness(300), Ok(255));
        assert_eq!(d.set_brightness(-1), Ok(0));
    }

    #[test]
    fn one_wake_lock_at_a_time() {
        let mut d = phone();
        d.hold(WakeLock::Full).unwrap();
        d.hold(WakeLock::Partial).unwrap();
        assert_eq!(d.wake_lock(), WakeLock::Partial);
        // Releasing the lock that is not held leaves the held one.
        d.release(WakeLock::Full);
        assert_eq!(d.wake_lock(), WakeLock::Partial);
        d.release(WakeLock::Partial);
        assert_eq!(d.wake_lock(), WakeLock::None);
    }

    #[test]
    fn services_are_counted() {
        let mut d = phone();
        d.vibrate().unwrap();
        d.vibrate().unwrap();
        d.notify().unwrap();
        assert_eq!((d.vibrations(), d.notifications()), (2, 1));
    }

    #[test]
    fn an_inverted_range_collapses_to_its_minimum() {
        let d = Device::new(Capabilities::default(), 5, 2);
        assert_eq!((d.min_volume(), d.max_volume()), (5, 5));
    }
}
