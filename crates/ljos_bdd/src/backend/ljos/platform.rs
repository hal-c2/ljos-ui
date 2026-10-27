//! The platform services a scenario stands in for: the clipboard, system
//! dialogs, drags in and out, permissions, sensors, positions and the
//! links the program opens.

use ljos_runtime::Clock as _;

use super::{HostSetting, LjosBackend, drop_action, failed};
use crate::backend::{BackendError, Platform, Result};

impl Platform for LjosBackend {
    fn set_clipboard(&mut self, text: &str) -> Result<()> {
        let live = self.live()?;
        ljos_quick::Clipboard::of(&live.engine).set_text(&live.engine, text);
        live.settle()
    }

    fn set_clipboard_html(&mut self, html: &str) -> Result<()> {
        use ljos_text::rich::{ParseOptions, RichText, TextFormat};
        let live = self.live()?;
        let doc = RichText::parse(TextFormat::Rich, html, &ParseOptions::default(), &|c| {
            Some(c.into())
        });
        // The characters as a plain text editor takes them: paragraphs on
        // lines, an image as nothing.
        let text: String = doc.text().chars().filter(|c| *c != '\u{FFFC}').collect();
        ljos_quick::Clipboard::of(&live.engine).set_text(&live.engine, &text);
        ljos_platform::Platform::of(&live.engine)
            .clipboard()
            .set_html(html, &text)
            .map_err(failed)?;
        live.settle()
    }

    fn clipboard(&mut self) -> Result<String> {
        let live = self.live()?;
        Ok(ljos_quick::Clipboard::of(&live.engine).text())
    }

    fn system_dialogs(&mut self) -> Result<Vec<(String, String)>> {
        Ok(self.platform.shown())
    }

    fn answer_system_dialog(&mut self, answer: crate::backend::SystemDialogAnswer) -> Result<()> {
        self.platform.answer(answer)?;
        let live = self.live()?;
        ljos_platform::Platform::of(&live.engine).finish(&live.engine);
        live.settle()
    }

    fn system_dialogs_stay(&mut self) -> Result<()> {
        self.platform.dialogs_stay();
        Ok(())
    }

    fn drag_from_outside(&mut self, step: &crate::backend::OutsideDrag) -> Result<()> {
        use crate::backend::OutsideDrag;
        use ljos_platform::{DragData, ExternalDrag};
        let actions = self.outside_actions;
        let live = self.live()?;
        let drag = match step {
            OutsideDrag::Enter { formats, at } => {
                live.outside_at = *at;
                ExternalDrag::Entered {
                    data: DragData::from_formats(formats.iter().map(|(f, d)| (f, d.as_bytes()))),
                    at: Some(*at),
                    actions,
                }
            }
            OutsideDrag::Offer { formats, at } => ExternalDrag::Offered {
                formats: formats.clone(),
                at: *at,
                actions,
            },
            OutsideDrag::Move(at) => {
                live.outside_at = *at;
                ExternalDrag::moved(*at)
            }
            OutsideDrag::Propose(name) => {
                let proposed = drop_action(name)?;
                self.outside_actions.proposed = proposed;
                let live = self.live()?;
                ExternalDrag::Moved {
                    at: live.outside_at,
                    actions: Some(ljos_platform::DragActions {
                        allowed: actions.allowed,
                        proposed,
                    }),
                }
            }
            OutsideDrag::Leave => ExternalDrag::Left,
            // What it carries was said when it came in.
            OutsideDrag::Drop => ExternalDrag::Dropped {
                data: DragData::default(),
                at: None,
            },
        };
        let live = self.live()?;
        live.outside_taken = ljos_platform::external_drag(&live.engine, &drag);
        live.settle()
    }

    fn outside_drag_action(&mut self) -> Result<Option<String>> {
        Ok(self.live()?.outside_taken.map(|a| a.name().to_owned()))
    }

    fn outside_drags_allow(&mut self, allowed: &[String], proposed: &str) -> Result<()> {
        self.outside_actions = ljos_platform::DragActions {
            allowed: allowed
                .iter()
                .map(|a| drop_action(a))
                .collect::<Result<_>>()?,
            proposed: drop_action(proposed)?,
        };
        Ok(())
    }

    fn drags_out(&mut self) -> Result<()> {
        self.platform.carry_drags_out();
        if let Some(live) = &self.live {
            self.platform.install_drag_out(&live.engine);
        }
        Ok(())
    }

    fn drag_out_asked(&mut self) -> Result<Option<crate::backend::DragOutAsked>> {
        self.live()?.settle()?;
        Ok(self
            .platform
            .drag_out()
            .and_then(|out| out.asked())
            .map(|drag| crate::backend::DragOutAsked {
                formats: drag
                    .data
                    .formats()
                    .into_iter()
                    .map(|f| {
                        let data = drag.data.data(&f).unwrap_or_default();
                        (f, data)
                    })
                    .collect(),
                allowed: drag
                    .actions
                    .allowed
                    .iter()
                    .map(|a| a.name().to_owned())
                    .collect(),
                proposed: drag.actions.proposed.name().to_owned(),
            }))
    }

    fn drag_out_given_up(&mut self) -> Result<bool> {
        self.live()?.settle()?;
        Ok(self.platform.drag_out().is_some_and(|out| out.given_up()))
    }

    fn drag_out_ends(&mut self, action: Option<&str>) -> Result<()> {
        let action = action.map(drop_action).transpose()?;
        let out = self
            .platform
            .drag_out()
            .cloned()
            .ok_or_else(|| BackendError::Failed("the window system carries no drags".into()))?;
        let live = self.live()?;
        if !out.end(&live.engine, action) {
            return Err(BackendError::Failed(
                "no drag out of the window is under way".into(),
            ));
        }
        live.settle()
    }

    fn drag_out_comes_back(&mut self, at: (f64, f64)) -> Result<()> {
        let asked = self
            .platform
            .drag_out()
            .and_then(|out| out.asked())
            .ok_or_else(|| {
                BackendError::Failed("no drag out of the window was asked for".into())
            })?;
        let live = self.live()?;
        live.outside_at = at;
        live.outside_taken = ljos_platform::external_drag(
            &live.engine,
            &ljos_platform::ExternalDrag::Entered {
                data: asked.data,
                at: Some(at),
                actions: asked.actions,
            },
        );
        live.settle()
    }

    fn copy_elsewhere(&mut self, text: &str) -> Result<()> {
        let live = self.live()?;
        // Straight to the clipboard, behind the application's back.
        ljos_platform::Platform::of(&live.engine)
            .clipboard()
            .set_text(ljos_platform::ClipboardMode::Clipboard, text)
            .map_err(failed)?;
        live.settle()
    }

    fn permissions_ask(&mut self) -> Result<()> {
        if self.live.is_some() {
            return Err(failed(
                "say that the system asks for permissions before the QML",
            ));
        }
        self.platform.permissions_ask();
        Ok(())
    }

    fn add_sensor(&mut self, kind: &str) -> Result<()> {
        if self.live.is_some() {
            return Err(failed("say what sensors the system has before the QML"));
        }
        self.platform.add_sensor(kind)
    }

    fn sensor_reads(&mut self, kind: &str, values: &[f64]) -> Result<()> {
        let timestamp = self.live()?.clock.now().as_micros();
        self.platform.sensor_reads(kind, timestamp, values)?;
        let live = self.live()?;
        ljos_platform::Platform::of(&live.engine).finish(&live.engine);
        live.settle()
    }

    fn system_position(&mut self, latitude: f64, longitude: f64, altitude: f64) -> Result<()> {
        if self.live.is_some() {
            return Err(failed("say where the system is before the QML"));
        }
        self.platform.system_position(latitude, longitude, altitude);
        Ok(())
    }

    fn system_position_moves(
        &mut self,
        latitude: f64,
        longitude: f64,
        altitude: f64,
    ) -> Result<()> {
        self.platform
            .position_moves(latitude, longitude, altitude)?;
        let live = self.live()?;
        ljos_platform::Platform::of(&live.engine).finish(&live.engine);
        live.settle()
    }

    fn opened_urls(&mut self) -> Result<Vec<String>> {
        Ok(self.platform.opened_urls())
    }

    fn permission_requests(&mut self) -> Result<Vec<String>> {
        Ok(self.platform.permission_requests())
    }

    fn answer_permission(&mut self, name: &str, granted: bool) -> Result<()> {
        self.platform.answer_permission(name, granted)?;
        let live = self.live()?;
        ljos_platform::Platform::of(&live.engine).finish(&live.engine);
        live.settle()
    }

    fn window_backdrop(&mut self, object: &str) -> Result<Option<crate::backend::BackdropAsked>> {
        let live = self.live()?;
        let win = live.find(object)?;
        let spec = ljos_controls::WindowSystem::of(&live.engine)
            .spec(&live.engine, win)
            .ok_or_else(|| failed(format!("\"{object}\" is no window")))?;
        use ljos_controls::WindowBackdrop as B;
        let kind = match spec.backdrop {
            B::None => return Ok(None),
            B::Blur => "blurred",
            B::Glass => "glass",
        };
        let look = match spec.color_scheme {
            ljos_quick::ColorScheme::Light => "light",
            ljos_quick::ColorScheme::Dark => "dark",
            ljos_quick::ColorScheme::Unknown => "the system's",
        };
        Ok(Some(crate::backend::BackdropAsked { kind, look }))
    }

    fn no_external_drops(&mut self) -> Result<()> {
        self.apply_or_queue(HostSetting::NoExternalDrops)
    }
}
