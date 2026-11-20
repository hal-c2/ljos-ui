//! The world a scenario's program runs in: fonts, clocks, locale, style,
//! the user's and the desktop's settings, the network and translations.

use std::time::Duration;

use qml_front::project::ControlsStyle;

use super::{
    HostSetting, LjosBackend, application_font, failed, set_js_clock, set_js_clock_stalled,
};
use crate::backend::{Environment, Result};

/// How long [`Environment::profile_ready`] waits for the web engine: the
/// longest a view waits for a new profile unless a scenario says otherwise.
const PROFILE_READY_WAIT: Duration = ljos_webengine::PROFILE_WAIT;

impl Environment for LjosBackend {
    fn system_fonts(&mut self) -> Result<()> {
        if self.live.is_some() {
            return Err(failed("the system's fonts are given before the QML"));
        }
        if !self.system_fonts {
            self.system_fonts = true;
            ljos_quick::set_text_engine(ljos_quick::TextEngine::with_system_fonts());
        }
        Ok(())
    }

    fn compiled_ahead_of_time(&mut self) -> Result<()> {
        if self.live.is_some() {
            return Err(failed(
                "the files are compiled ahead of time before the QML is given",
            ));
        }
        self.ahead = true;
        Ok(())
    }

    fn qt_qml_lowered(&mut self) -> Result<()> {
        if self.live.is_some() {
            return Err(failed(
                "the files are lowered to the Ljós API before the QML is given",
            ));
        }
        self.lowered = true;
        Ok(())
    }

    fn aot_check(&mut self, on: bool) -> Result<()> {
        self.aot_check = Some(on);
        if let Some(live) = &self.live {
            live.engine.set_aot_check(on);
        }
        Ok(())
    }

    fn tier1_check(&mut self, on: bool) -> Result<()> {
        self.tier1_check = Some(on);
        if let Some(live) = &self.live {
            live.engine.set_tier1_check(on);
        }
        Ok(())
    }

    fn own_log_handler(&mut self) -> Result<()> {
        self.live()?.engine.set_log_handler(|_| {});
        Ok(())
    }

    fn profile_wait(&mut self, wait: Duration) -> Result<()> {
        // Every view of the process: the runner sets it back before each
        // scenario.
        ljos_webengine::set_profile_wait(Some(wait));
        Ok(())
    }

    fn hold_new_profiles(&mut self) -> Result<()> {
        // Every profile of the process: the runner stops holding before
        // each scenario.
        ljos_webengine::hold_new_profiles(true);
        Ok(())
    }

    fn profile_ready(&mut self, storage_name: &str) -> Result<()> {
        let since = std::time::Instant::now();
        // CEF readies storage on the Engine thread, in the work the view's
        // wait keeps asking for, which a finish waits for.
        loop {
            // Both read on one pass: storage readied during a finish that
            // ran past the deadline is still too late.
            let readied = ljos_webengine::profile_readied(storage_name);
            let late = since.elapsed() >= PROFILE_READY_WAIT;
            if readied && !late {
                break;
            }
            if late {
                return Err(failed(format!(
                    "the web engine had not readied the storage of profile {storage_name:?} after {} s{}",
                    PROFILE_READY_WAIT.as_secs(),
                    ljos_webengine::unavailable()
                        .map(|why| format!(": {why}"))
                        .unwrap_or_default()
                )));
            }
            self.live()?.drive(|d| d.finish_promised())?;
            // Nothing promised (a view that no longer waits) returns at once.
            std::thread::sleep(Duration::from_millis(5));
        }
        // A profile readied while the scenario holds them is held; one
        // that is not let go was made before the hold, or is not the one
        // the scenario names.
        if !ljos_webengine::let_profile_go(storage_name) && ljos_webengine::holding_profiles() {
            return Err(failed(format!(
                "the storage of profile {storage_name:?} is ready, but it was not held unready, so nothing was let go"
            )));
        }
        self.live()?.settle()
    }

    fn js_clock_stalled(&mut self, stall: Duration) -> Result<()> {
        self.js_stall = Some(stall);
        if let Some(live) = &self.live {
            set_js_clock_stalled(&live.engine, stall)?;
        }
        Ok(())
    }

    fn clock_reading(&mut self, cost: Duration) -> Result<()> {
        if self.live.is_some() {
            return Err(failed("the clock's readings are given before the QML"));
        }
        self.clock_reading = Some(cost);
        Ok(())
    }

    fn keep_pictures(&mut self) -> Result<()> {
        if self.live.is_some() {
            return Err(failed("the pictures are kept from before the QML"));
        }
        self.keep_pictures = true;
        Ok(())
    }

    fn js_clock(&mut self, native: Duration) -> Result<()> {
        self.js_native = Some(native);
        if let Some(live) = &self.live {
            set_js_clock(&live.engine, native)?;
        }
        Ok(())
    }

    fn key_bindings(&mut self, platform: ljos_quick::keys::Platform) -> Result<()> {
        self.key_bindings = Some(platform);
        if let Some(live) = &self.live {
            ljos_quick::keys::set_platform(&live.engine, platform);
        }
        Ok(())
    }

    fn wall_clock(&mut self, epoch_ms: f64) -> Result<()> {
        self.wall_clock = Some(epoch_ms);
        if let Some(live) = &self.live {
            live.engine.set_wall_clock_origin(Some(epoch_ms));
        }
        Ok(())
    }

    fn locale_environment(&mut self, vars: &[(String, String)]) -> Result<()> {
        if self.live.is_some() {
            return Err(failed(
                "a program's environment is given before anything is loaded",
            ));
        }
        self.locale_env = Some(vars.to_vec());
        Ok(())
    }

    fn controls_style(&mut self, name: &str) -> Result<()> {
        if self.live.is_some() {
            return Err(failed("a style is chosen before anything is loaded"));
        }
        self.controls_style = Some(ControlsStyle {
            name: name.to_owned(),
            fallback: None,
        });
        Ok(())
    }

    fn command_line_style(&mut self, name: &str) -> Result<()> {
        if self.live.is_some() {
            return Err(failed("a style is chosen before anything is loaded"));
        }
        self.command_line_style = Some(name.to_owned());
        Ok(())
    }

    fn controls_configuration(&mut self, text: &str) -> Result<()> {
        if self.live.is_some() {
            return Err(failed("a configuration is given before anything is loaded"));
        }
        self.controls_conf = Some(text.to_owned());
        Ok(())
    }

    fn controls_variable(&mut self, name: &str, value: &str) -> Result<()> {
        if self.live.is_some() {
            return Err(failed("a variable is given before anything is loaded"));
        }
        self.controls_vars.push((name.to_owned(), value.to_owned()));
        Ok(())
    }

    fn system_setting(&mut self, setting: crate::backend::SystemSetting) -> Result<()> {
        self.apply_or_queue(HostSetting::System(setting))
    }

    fn app_setting(&mut self, setting: crate::backend::SystemSetting) -> Result<()> {
        self.apply_or_queue(HostSetting::App(setting))
    }

    fn desktop_setting(
        &mut self,
        namespace: &str,
        key: &str,
        value: ljos_a11y::SettingValue,
    ) -> Result<()> {
        // The shell follows the key of the desktop it runs on; a test may
        // say either.
        if let Some(font) = ljos_a11y::DesktopSettings::font_of(namespace, key, &value) {
            let font = match font {
                ljos_a11y::DesktopFont::Pango(name) => {
                    ljos_controls::desktop_font::from_pango(&name)
                }
                ljos_a11y::DesktopFont::Qt(s) => ljos_controls::desktop_font::from_qt_string(&s),
            };
            return match font {
                Some(font) => self.apply_or_queue(HostSetting::Font(font)),
                None => Ok(()),
            };
        }
        // A key the shell does not read is not reported, as on the bus.
        if !self.desktop.set(namespace, key, &value) {
            return Ok(());
        }
        let report = self.desktop.prefs();
        self.apply_or_queue(HostSetting::Desktop(report))
    }

    fn network(&mut self, status: ljos_platform::NetworkStatus) -> Result<()> {
        self.apply_or_queue(HostSetting::Network(status))
    }

    fn link_colour(&mut self, colour: &str) -> Result<()> {
        let colour = ljos_engine::Color::parse(colour)
            .ok_or_else(|| failed(format!("{colour:?} is not a colour")))?;
        self.apply_or_queue(HostSetting::LinkColour(colour))
    }

    // The file is read as it is, the way a development build reads one; a
    // shipped application would have compiled it to `.ljl`.
    fn translation(&mut self, name: &str, text: &str) -> Result<()> {
        let catalog = if name.ends_with(".po") {
            ljos_i18n::po::PoFile::parse(text)
                .map_err(|e| failed(format!("{name}: {e}")))?
                .catalog(name)
        } else {
            ljos_i18n::ts::TsFile::read(text)
                .map_err(|e| failed(format!("{name}: {e}")))?
                .catalog()
        };
        match self.live.as_mut() {
            Some(live) => {
                live.engine.install_catalog(Box::new(catalog));
                live.settle()
            }
            None => {
                self.catalogs.push(catalog);
                Ok(())
            }
        }
    }

    fn set_ui_language(&mut self, language: &str) -> Result<()> {
        let live = self.live()?;
        live.engine.set_ui_language(language);
        live.settle()
    }

    fn set_application_font(&mut self, font: Option<&[(String, String)]>) -> Result<()> {
        let font = font.map(application_font).transpose()?;
        let live = self.live()?;
        ljos_controls::set_application_font(&live.engine, font.as_ref());
        live.settle()
    }
}
