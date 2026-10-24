//! Screen captures (FUN_6f192ce0): Print Screen saves the frame as a 32-bit TGA in the
//! Screenshots folder, named WC3ScrnShot_<MMDDYY_HHMMSS>_<NN> with a count kept for the
//! session (the time in UTC: the standard library has no time zones).  SCREENCAP_MESSAGE,
//! or SCREENCAP_FAILED, shows in yellow for 64 frames as it fades (FUN_6f192f60).

use w3_ui::{Draw, FrameDb, Justify, Rect};

/// An uncompressed true-color TGA, bottom row first, in blue, green, red, and alpha.
fn tga(pixels: &[u8], width: u32, height: u32, bgra: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(18 + pixels.len());
    out.extend_from_slice(&[0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    out.extend_from_slice(&(width as u16).to_le_bytes());
    out.extend_from_slice(&(height as u16).to_le_bytes());
    out.extend_from_slice(&[32, 8]);
    let row = (width * 4) as usize;
    for r in (0..height as usize).rev() {
        for p in pixels[r * row..(r + 1) * row].chunks(4) {
            let (b, g, rd) = if bgra { (p[0], p[1], p[2]) } else { (p[2], p[1], p[0]) };
            out.extend_from_slice(&[b, g, rd, 255]);
        }
    }
    out
}

#[derive(Default)]
pub struct Capture {
    /// Print Screen was pressed: the next frame is saved.
    pub requested: bool,
    count: u32,
    message: Option<(String, u32)>,
}

impl Capture {
    /// The message drawn this frame, if one is showing.
    pub fn overlay(&mut self, db: &FrameDb) -> Option<Draw> {
        let (text, left) = self.message.as_mut()?;
        let alpha = (*left * 4).min(255) as f32 / 255.0;
        let d = Draw::Text {
            font: db.decorate("MasterFont"),
            size: 0.02,
            rect: Rect { x0: 0.0, x1: 0.8, y0: 0.0, y1: 0.03 },
            text: text.clone(),
            color: [1.0, 1.0, 0.0, alpha],
            justify_h: Justify::Start,
            justify_v: Justify::Start,
            shadow: None,
        };
        *left -= 1;
        if *left == 0 {
            self.message = None;
        }
        Some(d)
    }

    /// Saves a frame read back as rows of 8-bit pixels (`bgra` when blue comes first).
    pub fn save(&mut self, db: &FrameDb, pixels: Option<&[u8]>, width: u32, height: u32, bgra: bool) {
        let key = match pixels.and_then(|p| self.write(p, width, height, bgra).ok()) {
            Some(()) => "SCREENCAP_MESSAGE",
            None => "SCREENCAP_FAILED",
        };
        self.message = Some((db.string(key), 64));
    }

    fn write(&mut self, pixels: &[u8], width: u32, height: u32, bgra: bool) -> anyhow::Result<()> {
        let dir = crate::data::screenshot_dir().ok_or_else(|| anyhow::anyhow!("no folder for screenshots"))?;
        std::fs::create_dir_all(&dir)?;
        self.count += 1;
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
        let (days, rem) = (t.div_euclid(86400), t.rem_euclid(86400));
        let (y, m, d) = crate::replays::civil_from_days(days);
        let stamp = format!("{m:02}{d:02}{:02}_{:02}{:02}{:02}", y % 100, rem / 3600, rem % 3600 / 60, rem % 60);
        let path = dir.join(format!("WC3ScrnShot_{stamp}_{:02}.tga", self.count % 100));
        std::fs::write(&path, tga(pixels, width, height, bgra))?;
        log::info!("screenshot {}", path.display());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_frame_reads_back_as_drawn() {
        // Two rows of two pixels, red green / blue white, as an RGBA surface gives them.
        let px = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
        let img = image::load_from_memory_with_format(&super::tga(&px, 2, 2, false), image::ImageFormat::Tga).unwrap().to_rgba8();
        assert_eq!(img.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(img.get_pixel(1, 0).0, [0, 255, 0, 255]);
        assert_eq!(img.get_pixel(0, 1).0, [0, 0, 255, 255]);
        assert_eq!(img.get_pixel(1, 1).0, [255, 255, 255, 255]);
    }
}
