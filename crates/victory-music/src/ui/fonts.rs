//! The fonts the phone draws with. None ship in the repository (it holds
//! no binary files): they are read from a folder the player chooses
//! (`VICTORY_FONTS_DIR`), with Rubik for titles and Inter for text (both
//! SIL Open Font License), and fall back to the system's own fonts.

use std::path::{Path, PathBuf};

use fontdue::{Font, FontSettings};

pub struct Fonts {
    /// Titles and the logo: Rubik Black.
    pub display: Font,
    /// Song titles: Rubik Bold.
    pub bold: Font,
    /// Text: Inter.
    pub text: Font,
    /// Labels: Inter SemiBold.
    pub semi: Font,
}

/// Files tried for each role, in order: in the fonts folder first, then the
/// system's.
const DISPLAY: &[&str] = &[
    "Rubik-Black.ttf",
    "seguibl.ttf",
    "ariblk.ttf",
    "DejaVuSans-Bold.ttf",
];
const BOLD: &[&str] = &[
    "Rubik-Bold.ttf",
    "segoeuib.ttf",
    "arialbd.ttf",
    "DejaVuSans-Bold.ttf",
];
const TEXT: &[&str] = &[
    "Inter-Regular.ttf",
    "segoeui.ttf",
    "arial.ttf",
    "DejaVuSans.ttf",
];
const SEMI: &[&str] = &[
    "Inter-SemiBold.ttf",
    "seguisb.ttf",
    "arialbd.ttf",
    "DejaVuSans-Bold.ttf",
];

fn system_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(win) = std::env::var_os("WINDIR") {
        dirs.push(PathBuf::from(win).join("Fonts"));
    }
    dirs.push(PathBuf::from(r"C:\Windows\Fonts"));
    dirs.push(PathBuf::from("/usr/share/fonts/truetype/dejavu"));
    dirs
}

impl Fonts {
    /// The fonts from `dir` (or `VICTORY_FONTS_DIR`), else the system's.
    pub fn load(dir: Option<&Path>) -> Result<Self, String> {
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(d) = dir {
            dirs.push(d.to_path_buf());
        }
        if let Some(d) = std::env::var_os("VICTORY_FONTS_DIR") {
            dirs.push(PathBuf::from(d));
        }
        dirs.extend(system_dirs());
        let find = |names: &[&str]| -> Result<Font, String> {
            for name in names {
                for d in &dirs {
                    let path = d.join(name);
                    if let Ok(bytes) = std::fs::read(&path)
                        && let Ok(f) = Font::from_bytes(bytes, FontSettings::default())
                    {
                        return Ok(f);
                    }
                }
            }
            Err(format!("none of {names:?} found in {dirs:?}"))
        };
        Ok(Self {
            display: find(DISPLAY)?,
            bold: find(BOLD)?,
            text: find(TEXT)?,
            semi: find(SEMI)?,
        })
    }
}
