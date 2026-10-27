//! Files a scenario gives the application: its QML, pictures, fonts, media,
//! translations and text files, beside it, served or in the user's home.

use cucumber::{given, then};
use gherkin::Step;

use crate::backend::BackendError;
use crate::fixture;
use crate::world::{LjosWorld, MAIN_FILE};

use super::{StepResult, docstring};

#[given("the QML:")]
fn the_qml(w: &mut LjosWorld, step: &Step) -> StepResult {
    w.stage(MAIN_FILE, &docstring(step)?);
    w.load(MAIN_FILE)
}

#[given(regex = r#"^the file "([^"]+)":$"#)]
fn the_file(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    w.stage(&name, &docstring(step)?);
    Ok(())
}

// What `ljos design export` writes: the module Ljos.Design as a directory
// of plain QML, which Qt imports as well.
#[given("the files of the module Ljos.Design")]
fn the_design_module(w: &mut LjosWorld) -> StepResult {
    use ljos_design::module::{DIRECTORY, Seeds, files};
    for f in files(Seeds::Hues) {
        w.stage(&format!("{DIRECTORY}/{}", f.name), &f.text);
    }
    Ok(())
}

// A Qt Linguist file, as a translator hands it over. Its `language`
// attribute says which language it is for.
#[given(regex = r#"^the translation file "([^"]+\.ts)":$"#)]
fn the_translation_file(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    let ts = docstring(step)?;
    // An XML declaration must be the first thing in the file, and the doc
    // string keeps the indentation it has in the scenario.
    let ts = ts.trim_start();
    w.each(|b| b.environment()?.translation(&name, ts))
}

// An image the QML can name by a relative URL. It is made here, in one
// colour, so that no binary file has to live beside the scenarios.
#[given(regex = r#"^an? (\d+) by (\d+) image file "([^"]+)" filled with "(#[0-9a-fA-F]+)"$"#)]
fn an_image_file(
    w: &mut LjosWorld,
    width: u32,
    height: u32,
    name: String,
    colour: String,
) -> StepResult {
    let bytes = fixture::image_file(&name, width, height, &colour)?;
    w.each(|b| b.files()?.resource(&name, &bytes))
}

// A PDF document beside the QML, with a page of each size in points
// ("300 by 200, 150 by 400"), each painted in its middle half.
#[given(regex = r#"^a PDF file "([^"]+)" of pages "([^"]+)" painted "(#[0-9a-fA-F]+)"$"#)]
fn a_pdf_file(w: &mut LjosWorld, name: String, pages: String, colour: String) -> StepResult {
    let pages = pages
        .split(',')
        .map(|page| {
            let (w, h) = page
                .split_once(" by ")
                .ok_or_else(|| format!("a page is \"W by H\", not {page:?}"))?;
            let side = |n: &str| n.trim().parse::<f64>().map_err(|e| format!("{n:?}: {e}"));
            Ok((side(w)?, side(h)?))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let bytes = fixture::pdf(&pages, &colour)?;
    w.each(|b| b.files()?.resource(&name, &bytes))
}

// An animated GIF beside the QML, one colour a frame, each frame shown
// for the same time; the colours are a list ("#ff0000, #00ff00").
#[given(
    regex = r#"^an? (\d+) by (\d+) animated image file "([^"]+)" of frames "([^"]+)" each shown for (\d+) milliseconds$"#
)]
fn an_animated_image_file(
    w: &mut LjosWorld,
    width: u32,
    height: u32,
    name: String,
    colours: String,
    delay: u32,
) -> StepResult {
    let colours: Vec<String> = colours.split(',').map(|c| c.trim().to_owned()).collect();
    let bytes = fixture::animated_gif(width, height, &colours, delay)?;
    w.each(|b| b.files()?.resource(&name, &bytes))
}

// The same, at `$SERVER/X.gif` on the scenario's web server.
#[given(
    regex = r#"^the web server serves an? (\d+) by (\d+) animated image file "([^"]+)" of frames "([^"]+)" each shown for (\d+) milliseconds$"#
)]
fn a_served_animated_image_file(
    w: &mut LjosWorld,
    width: u32,
    height: u32,
    name: String,
    colours: String,
    delay: u32,
) -> StepResult {
    let colours: Vec<String> = colours.split(',').map(|c| c.trim().to_owned()).collect();
    let bytes = fixture::animated_gif(width, height, &colours, delay)?;
    w.web()?.serve(&name, bytes);
    Ok(())
}

// A file with an image's name and something else inside.
#[given(regex = r#"^a file "([^"]+)" that is not an image$"#)]
fn not_an_image(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| b.files()?.resource(&name, &fixture::not_an_image()))
}

// The same, at `$SERVER/X.png` on the scenario's web server.
#[given(
    regex = r#"^the web server serves an? (\d+) by (\d+) image file "([^"]+)" filled with "(#[0-9a-fA-F]+)"$"#
)]
fn a_served_image_file(
    w: &mut LjosWorld,
    width: u32,
    height: u32,
    name: String,
    colour: String,
) -> StepResult {
    let bytes = fixture::image_file(&name, width, height, &colour)?;
    w.web()?.serve(&name, bytes);
    Ok(())
}

// A picture whose bytes are about as many as its pixels, as a
// photograph's are, at `$SERVER/X.png`.
#[given(regex = r#"^the web server serves an? (\d+) by (\d+) noisy image file "([^"]+)"$"#)]
fn a_served_noisy_image_file(
    w: &mut LjosWorld,
    width: u32,
    height: u32,
    name: String,
) -> StepResult {
    let bytes = fixture::noisy_png(width, height)?;
    w.web()?.serve(&name, bytes);
    Ok(())
}

#[given(regex = r#"^the web server serves a file "([^"]+)" that is not an image$"#)]
fn served_not_an_image(w: &mut LjosWorld, name: String) -> StepResult {
    w.web()?.serve(&name, fixture::not_an_image());
    Ok(())
}

// A font file of features/text/fonts, beside the QML under its own name.
#[given(regex = r#"^the font file "([^"]+)"$"#)]
fn the_font_file(w: &mut LjosWorld, name: String) -> StepResult {
    let bytes = feature_file("text/fonts", &name)?;
    w.each(|b| b.files()?.resource(&name, &bytes))
}

// A sound or a film of features/multimedia/media, beside the QML under its
// own name.
#[given(regex = r#"^the media file "([^"]+)"$"#)]
fn the_media_file(w: &mut LjosWorld, name: String) -> StepResult {
    let bytes = feature_file("multimedia/media", &name)?;
    w.each(|b| b.files()?.resource(&name, &bytes))
}

// The same, at `$SERVER/X` on the scenario's web server.
#[given(regex = r#"^the web server serves the media file "([^"]+)"$"#)]
fn a_served_media_file(w: &mut LjosWorld, name: String) -> StepResult {
    let bytes = feature_file("multimedia/media", &name)?;
    w.web()?.serve(&name, bytes);
    Ok(())
}

// A sprite sheet of features/animation/sprites, beside the QML under its
// own name.
#[given(regex = r#"^the sprite sheet "([^"]+)"$"#)]
fn the_sprite_sheet(w: &mut LjosWorld, name: String) -> StepResult {
    let bytes = feature_file("animation/sprites", &name)?;
    w.each(|b| b.files()?.resource(&name, &bytes))
}

// A file with a font's name and something else inside.
#[given(regex = r#"^a file "([^"]+)" that is not a font$"#)]
fn not_a_font(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| b.files()?.resource(&name, &fixture::not_an_image()))
}

// A text file beside the QML (a settings file, say).
#[given(regex = r#"^the text file "([^"]+)":$"#)]
fn the_text_file(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    let text = docstring(step)?;
    w.each(|b| b.files()?.text_file(&name, &text))
}

// The same, at `$SERVER/X` on the scenario's web server.
#[given(regex = r#"^the web server serves the text file "([^"]+)":$"#)]
fn a_served_text_file(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    let text = docstring(step)?;
    w.web()?.serve(&name, text.trim().as_bytes().to_vec());
    Ok(())
}

// A text file under the user's home directory.
#[given(regex = r#"^the user has the text file "([^"]+)":$"#)]
fn the_user_file(w: &mut LjosWorld, path: String, step: &Step) -> StepResult {
    let text = docstring(step)?;
    w.each(|b| b.files()?.user_file(&path, &text))
}

#[then(regex = r#"^the text file "([^"]+)" has the line "([^"]*)"$"#)]
fn text_file_has_line(w: &mut LjosWorld, name: String, line: String) -> StepResult {
    w.each(|b| match b.files()?.read_text_file(&name)? {
        Some(text) if text.lines().any(|l| l == line) => Ok(()),
        Some(text) => Err(BackendError::Failed(format!(
            "{name} has no line {line:?}; it holds:\n{text}"
        ))),
        None => Err(BackendError::Failed(format!("there is no file {name}"))),
    })
}

#[then(regex = r#"^the text file "([^"]+)" does not exist$"#)]
fn text_file_absent(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| match b.files()?.read_text_file(&name)? {
        None => Ok(()),
        Some(text) => Err(BackendError::Failed(format!("{name} exists:\n{text}"))),
    })
}

#[given(regex = r#"^the web server streams (\d+) bytes at "([^"]+)"$"#)]
fn streamed_bytes(w: &mut LjosWorld, count: usize, name: String) -> StepResult {
    w.web()?.stream_bytes(&name, count);
    Ok(())
}

#[given(
    regex = r#"^the web server streams (\d+) zero bytes after hex "([0-9a-f]+)" at "([^"]+)"$"#
)]
fn prefixed_streamed_bytes(
    w: &mut LjosWorld,
    count: usize,
    hex: String,
    name: String,
) -> StepResult {
    if !hex.len().is_multiple_of(2) {
        return Err("hex requires pairs of digits".into());
    }
    let prefix = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    w.web()?.stream_prefixed_bytes(&name, count, prefix);
    Ok(())
}

/// A file of `features/<dir>`, which the scenarios keep beside themselves.
fn feature_file(dir: &str, name: &str) -> Result<Vec<u8>, String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../features")
        .join(dir)
        .join(name);
    std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
}
