//! Finding the player's Pharaoh game data. Osiris looks for an install holding the
//! game's `Data/` folder: the folder the user picked before, next to the program (or
//! the program sitting inside the install), the user folder, the usual GOG and Steam
//! install places, and the current directory. When none is found it asks for the
//! folder with the system's folder picker and remembers the answer.

use crate::lang::{tr, trf};
use std::path::{Path, PathBuf};

/// Where the picked folder is remembered, in the user folder.
const REMEMBERED: &str = "data_dir.txt";

/// Whether `dir` holds the game: its `Data/` folder with the art in it.
fn is_game(dir: &Path) -> bool {
    dir.join("Data").is_dir()
}

/// The game folder for a folder the player picked: the install itself, the `Data/`
/// folder inside it, or a folder holding `PharaohData`.
fn game_in(dir: &Path) -> Option<PathBuf> {
    [dir.to_path_buf(), dir.join("PharaohData")]
        .into_iter()
        .chain(dir.parent().filter(|_| dir.file_name().is_some_and(|n| n.eq_ignore_ascii_case("data"))).map(Path::to_path_buf))
        .find(|p| is_game(p))
}

/// Install folders whose names start with "Pharaoh" under the usual GOG and Steam
/// places.
fn installs() -> Vec<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let mut bases = Vec::new();
    if cfg!(windows) {
        for drive in ["C:", "D:", "E:"] {
            bases.push(PathBuf::from(format!("{drive}\\GOG Games")));
            bases.push(PathBuf::from(format!("{drive}\\Games")));
            bases.push(PathBuf::from(format!("{drive}\\SteamLibrary\\steamapps\\common")));
            bases.push(PathBuf::from(format!("{drive}\\Sierra")));
        }
        for pf in [env("ProgramFiles(x86)"), env("ProgramFiles")].into_iter().flatten() {
            bases.push(pf.join("GOG Galaxy\\Games"));
            bases.push(pf.join("GOG Games"));
            bases.push(pf.join("Steam\\steamapps\\common"));
            bases.push(pf.join("Sierra"));
            bases.push(pf.clone());
        }
    } else if let Some(home) = env("HOME") {
        bases.push(home.join("GOG Games"));
        bases.push(home.join("Games"));
        bases.push(home.join(".steam/steam/steamapps/common"));
        bases.push(home.join(".local/share/Steam/steamapps/common"));
        bases.push(home.join("Library/Application Support/Steam/steamapps/common"));
    }
    let mut found = Vec::new();
    for base in bases {
        let Ok(entries) = std::fs::read_dir(&base) else { continue };
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().to_ascii_lowercase().starts_with("pharaoh") {
                found.push(e.path());
            }
        }
    }
    found
}

/// The game data, if Osiris can find it without asking.
pub fn find(user_dir: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(saved) = std::fs::read_to_string(user_dir.join(REMEMBERED)) {
        candidates.push(PathBuf::from(saved.trim()));
    }
    if let Ok(exe) = std::env::current_exe() {
        for dir in exe.ancestors().skip(1).take(5) {
            candidates.push(dir.join("PharaohData"));
            candidates.push(dir.to_path_buf());
        }
    }
    candidates.push(user_dir.join("PharaohData"));
    candidates.push(PathBuf::from("PharaohData"));
    candidates.extend(installs());
    candidates.into_iter().find(|p| is_game(p))
}

/// Asks for the game folder until the player picks one holding the game or gives up,
/// and remembers it.
pub fn pick(user_dir: &Path) -> Option<PathBuf> {
    use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
    let mut text = format!("{}\n\n{}", tr("Osiris plays from your own copy of Pharaoh (the GOG or Steam \"Pharaoh + Cleopatra\", or the original CD install)."), tr("Choose the folder Pharaoh is installed in: the one holding its Data folder and mission1.pak."));
    let choose_folder = tr("Choose Folder...").to_owned();
    loop {
        let go = MessageDialog::new()
            .set_level(MessageLevel::Info)
            .set_title(tr("Find Pharaoh"))
            .set_description(&text)
            .set_buttons(MessageButtons::OkCancelCustom(choose_folder.clone(), tr("Quit").into()))
            .show();
        let choose = match &go {
            MessageDialogResult::Ok => true,
            MessageDialogResult::Custom(label) => *label == choose_folder,
            _ => false,
        };
        if !choose {
            return None;
        }
        let dir = rfd::FileDialog::new().set_title(tr("Choose the Pharaoh folder")).pick_folder()?;
        if let Some(game) = game_in(&dir) {
            let _ = std::fs::write(user_dir.join(REMEMBERED), game.to_string_lossy().as_bytes());
            return Some(game);
        }
        text = format!("{}\n\n{}", trf("{0} doesn't hold Pharaoh's Data folder.", &[&dir.display()]), tr("Choose the folder Pharaoh is installed in: the one holding its Data folder and mission1.pak."));
    }
}

/// Shows a fatal error where the player can see it: a window without a console
/// (Windows, or an app launched from Finder) would otherwise just vanish.
pub fn show_error(message: &str) {
    rfd::MessageDialog::new().set_level(rfd::MessageLevel::Error).set_title("Osiris").set_description(message).set_buttons(rfd::MessageButtons::Ok).show();
}
