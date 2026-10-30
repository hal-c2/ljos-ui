//! The world a scenario's program runs in: fonts, clocks, locale, style,
//! the user's and the desktop's settings, the network and translations.

use std::time::Duration;

use qml_front::project::ControlsStyle;

use super::{
    HostSetting, LjosBackend, application_font, failed, set_js_clock, set_js_clock_stalled,
};
use crate::backend::{Environment, Result};

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

    fn js_clock_stalled(&mut self, stall: Duration) -> Result<()> {
        self.js_stall = Some(stall);
        if let Some(live) = &self.live {
            set_js_clock_stalled(&live.engine, stall)?;
        }
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

    // The `.ts` file is read as it is, the way a development build reads
    // one; a shipped application would have compiled it to `.ljl`.
    fn translation(&mut self, name: &str, ts: &str) -> Result<()> {
        let catalog = ljos_i18n::ts::TsFile::read(ts)
            .map_err(|e| failed(format!("{name}: {e}")))?
            .catalog();
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
