//! A custom campaign's war3campaign.w3f (game.dll FUN_6f7243f0): its name, author, and
//! description, its campaign screen's backdrop, sound, and fog, and its mission buttons.

use crate::{Error, Reader};

#[derive(Clone, Debug, Default)]
pub struct CampaignInfo {
    pub version: u32,
    pub campaign_version: u32,
    pub editor_version: u32,
    pub name: String,
    pub difficulty: String,
    pub author: String,
    pub description: String,
    /// Flag 1: the player chooses the difficulty.
    pub flags: u32,
    /// The UI\WorldEditData.txt [CampaignScreens] entry for the backdrop, or -1 for
    /// `background_path`.
    pub background: i32,
    pub background_path: String,
    pub minimap_path: String,
    /// The [AmbientSounds] entry played on the campaign screen, or -1 for `sound_path`.
    pub sound: i32,
    pub sound_path: String,
    /// Fog style (-1 for none), start, end, density, and color (from version 1).
    pub fog: (i32, f32, f32, f32, [u8; 4]),
    /// The cursor and interface race (from version 1).
    pub race: i32,
    pub missions: Vec<CampaignMission>,
    /// The maps in the campaign, each with a flag byte.
    pub maps: Vec<(u8, String)>,
}

/// One of the campaign screen's mission buttons.
#[derive(Clone, Debug, Default)]
pub struct CampaignMission {
    /// Shown from the start (any nonzero value: the editor leaves the rest undefined).
    pub visible: bool,
    pub chapter: String,
    pub title: String,
    pub map: String,
}

impl CampaignInfo {
    pub fn parse(data: &[u8]) -> Result<CampaignInfo, Error> {
        let mut r = Reader::new(data);
        let mut c = CampaignInfo { version: r.u32()?, ..Default::default() };
        if c.version >= 2 {
            return Err(Error::Format("unknown campaign info version"));
        }
        c.campaign_version = r.u32()?;
        c.editor_version = r.u32()?;
        c.name = r.cstr()?;
        c.difficulty = r.cstr()?;
        c.author = r.cstr()?;
        c.description = r.cstr()?;
        c.flags = r.u32()?;
        c.background = r.i32()?;
        c.background_path = r.cstr()?;
        c.minimap_path = r.cstr()?;
        c.sound = r.i32()?;
        c.sound_path = r.cstr()?;
        c.fog = (-1, 0.0, 0.0, 0.0, [0; 4]);
        if c.version >= 1 {
            c.fog = (r.i32()?, r.f32()?, r.f32()?, r.f32()?, r.bytes(4)?.try_into().unwrap());
            c.race = r.i32()?;
        }
        let n = r.u32()?;
        for _ in 0..n {
            let visible = r.u32()? != 0;
            c.missions.push(CampaignMission { visible, chapter: r.cstr()?, title: r.cstr()?, map: r.cstr()? });
        }
        if r.remaining() >= 4 {
            let n = r.u32()?;
            for _ in 0..n {
                c.maps.push((r.u8()?, r.cstr()?));
            }
        }
        Ok(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version_one() {
        let mut d = Vec::new();
        for v in [1u32, 158, 6030] {
            d.extend(v.to_le_bytes());
        }
        for s in ["TRIGSTR_001", "TRIGSTR_002", "TRIGSTR_004", "TRIGSTR_003"] {
            d.extend(s.as_bytes());
            d.push(0);
        }
        d.extend(1u32.to_le_bytes());
        d.extend(0i32.to_le_bytes());
        d.push(0);
        d.extend(b"UI\\Widgets\\Glues\\Minimap-CustomCampaign-Orc\0");
        d.extend(0i32.to_le_bytes());
        d.push(0);
        d.extend(0i32.to_le_bytes());
        for f in [0.0f32, 2000.0, 0.5] {
            d.extend(f.to_le_bytes());
        }
        d.extend([0x4d, 0x33, 0, 0xff]);
        d.extend(1i32.to_le_bytes());
        d.extend(1u32.to_le_bytes());
        d.extend(0x0012f8e5u32.to_le_bytes());
        d.extend(b"TRIGSTR_035\0TRIGSTR_010\0Demo03.w3m\0");
        d.extend(1u32.to_le_bytes());
        d.extend(b"\0Demo03.w3m\0");
        let c = CampaignInfo::parse(&d).unwrap();
        assert_eq!(c.name, "TRIGSTR_001");
        assert_eq!(c.minimap_path, "UI\\Widgets\\Glues\\Minimap-CustomCampaign-Orc");
        assert_eq!(c.fog.2, 2000.0);
        assert_eq!(c.race, 1);
        assert_eq!(c.missions.len(), 1);
        assert!(c.missions[0].visible);
        assert_eq!(c.missions[0].map, "Demo03.w3m");
        assert_eq!(c.maps, vec![(0, "Demo03.w3m".to_string())]);
    }
}
