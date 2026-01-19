// Doom-inspired themes for MultiCode TUI
//
// Each theme module exports a Theme instance with colors ported from doom-emacs themes.

mod doom_acario_dark;
mod doom_acario_light;
mod doom_ayu_dark;
mod doom_ayu_light;
mod doom_challenger_deep;
mod doom_city_lights;
mod doom_dracula;
mod doom_earl_grey;
mod doom_ephemeral;
mod doom_fairy_floss;
mod doom_flatwhite;
mod doom_gruvbox;
mod doom_gruvbox_light;
mod doom_henna;
mod doom_homage_black;
mod doom_horizon;
mod doom_iosvkem;
mod doom_ir_black;
mod doom_laserwave;
mod doom_material;
mod doom_miramare;
mod doom_molokai;
mod doom_monokai_pro;
mod doom_moonlight;
mod doom_nord;
mod doom_nord_light;
mod doom_nova;
mod doom_oceanic_next;
mod doom_old_hope;
mod doom_one;
mod doom_one_light;
mod doom_outrun_electric;
mod doom_palenight;
mod doom_rouge;
mod doom_shades_of_purple;
mod doom_snazzy;
mod doom_solarized_dark;
mod doom_solarized_light;
mod doom_spacegrey;
mod doom_tokyo_night;
mod doom_tomorrow_day;
mod doom_vibrant;
mod doom_wilmersdorf;
mod doom_zenburn;

use crate::tui::app_tui::theme::Theme;

/// Theme metadata for display in picker
pub struct ThemeInfo {
    pub name: &'static str,
    pub display_name: &'static str,
    pub is_dark: bool,
}

/// All available themes (dark themes first, then light)
pub const THEMES: &[ThemeInfo] = &[
    // Dark themes
    ThemeInfo {
        name: "doom-one",
        display_name: "One",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-dracula",
        display_name: "Dracula",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-gruvbox",
        display_name: "Gruvbox",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-nord",
        display_name: "Nord",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-tokyo-night",
        display_name: "Tokyo Night",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-monokai-pro",
        display_name: "Monokai Pro",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-palenight",
        display_name: "Palenight",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-solarized-dark",
        display_name: "Solarized Dark",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-material",
        display_name: "Material",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-horizon",
        display_name: "Horizon",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-oceanic-next",
        display_name: "Oceanic Next",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-zenburn",
        display_name: "Zenburn",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-snazzy",
        display_name: "Snazzy",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-city-lights",
        display_name: "City Lights",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-ayu-dark",
        display_name: "Ayu Dark",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-challenger-deep",
        display_name: "Challenger Deep",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-fairy-floss",
        display_name: "Fairy Floss",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-laserwave",
        display_name: "Laserwave",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-moonlight",
        display_name: "Moonlight",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-vibrant",
        display_name: "Vibrant",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-wilmersdorf",
        display_name: "Wilmersdorf",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-spacegrey",
        display_name: "Spacegrey",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-rouge",
        display_name: "Rouge",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-old-hope",
        display_name: "Old Hope",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-nova",
        display_name: "Nova",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-molokai",
        display_name: "Molokai",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-ir-black",
        display_name: "IR Black",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-henna",
        display_name: "Henna",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-ephemeral",
        display_name: "Ephemeral",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-miramare",
        display_name: "Miramare",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-acario-dark",
        display_name: "Acario Dark",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-shades-of-purple",
        display_name: "Shades of Purple",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-outrun-electric",
        display_name: "Outrun Electric",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-homage-black",
        display_name: "Homage Black",
        is_dark: true,
    },
    ThemeInfo {
        name: "doom-iosvkem",
        display_name: "Iosvkem",
        is_dark: true,
    },
    // Light themes
    ThemeInfo {
        name: "doom-one-light",
        display_name: "One Light",
        is_dark: false,
    },
    ThemeInfo {
        name: "doom-gruvbox-light",
        display_name: "Gruvbox Light",
        is_dark: false,
    },
    ThemeInfo {
        name: "doom-nord-light",
        display_name: "Nord Light",
        is_dark: false,
    },
    ThemeInfo {
        name: "doom-solarized-light",
        display_name: "Solarized Light",
        is_dark: false,
    },
    ThemeInfo {
        name: "doom-tomorrow-day",
        display_name: "Tomorrow Day",
        is_dark: false,
    },
    ThemeInfo {
        name: "doom-ayu-light",
        display_name: "Ayu Light",
        is_dark: false,
    },
    ThemeInfo {
        name: "doom-earl-grey",
        display_name: "Earl Grey",
        is_dark: false,
    },
    ThemeInfo {
        name: "doom-flatwhite",
        display_name: "Flatwhite",
        is_dark: false,
    },
    ThemeInfo {
        name: "doom-acario-light",
        display_name: "Acario Light",
        is_dark: false,
    },
];

/// Get a theme by name
pub fn get_theme(name: &str) -> Option<Theme> {
    match name {
        "doom-one" => Some(doom_one::theme()),
        "doom-dracula" => Some(doom_dracula::theme()),
        "doom-gruvbox" => Some(doom_gruvbox::theme()),
        "doom-nord" => Some(doom_nord::theme()),
        "doom-tokyo-night" => Some(doom_tokyo_night::theme()),
        "doom-monokai-pro" => Some(doom_monokai_pro::theme()),
        "doom-palenight" => Some(doom_palenight::theme()),
        "doom-solarized-dark" => Some(doom_solarized_dark::theme()),
        "doom-material" => Some(doom_material::theme()),
        "doom-horizon" => Some(doom_horizon::theme()),
        "doom-oceanic-next" => Some(doom_oceanic_next::theme()),
        "doom-zenburn" => Some(doom_zenburn::theme()),
        "doom-snazzy" => Some(doom_snazzy::theme()),
        "doom-city-lights" => Some(doom_city_lights::theme()),
        "doom-ayu-dark" => Some(doom_ayu_dark::theme()),
        "doom-challenger-deep" => Some(doom_challenger_deep::theme()),
        "doom-fairy-floss" => Some(doom_fairy_floss::theme()),
        "doom-laserwave" => Some(doom_laserwave::theme()),
        "doom-moonlight" => Some(doom_moonlight::theme()),
        "doom-vibrant" => Some(doom_vibrant::theme()),
        "doom-wilmersdorf" => Some(doom_wilmersdorf::theme()),
        "doom-spacegrey" => Some(doom_spacegrey::theme()),
        "doom-rouge" => Some(doom_rouge::theme()),
        "doom-old-hope" => Some(doom_old_hope::theme()),
        "doom-nova" => Some(doom_nova::theme()),
        "doom-molokai" => Some(doom_molokai::theme()),
        "doom-ir-black" => Some(doom_ir_black::theme()),
        "doom-henna" => Some(doom_henna::theme()),
        "doom-ephemeral" => Some(doom_ephemeral::theme()),
        "doom-miramare" => Some(doom_miramare::theme()),
        "doom-acario-dark" => Some(doom_acario_dark::theme()),
        "doom-shades-of-purple" => Some(doom_shades_of_purple::theme()),
        "doom-outrun-electric" => Some(doom_outrun_electric::theme()),
        "doom-homage-black" => Some(doom_homage_black::theme()),
        "doom-iosvkem" => Some(doom_iosvkem::theme()),
        "doom-one-light" => Some(doom_one_light::theme()),
        "doom-gruvbox-light" => Some(doom_gruvbox_light::theme()),
        "doom-nord-light" => Some(doom_nord_light::theme()),
        "doom-solarized-light" => Some(doom_solarized_light::theme()),
        "doom-tomorrow-day" => Some(doom_tomorrow_day::theme()),
        "doom-ayu-light" => Some(doom_ayu_light::theme()),
        "doom-earl-grey" => Some(doom_earl_grey::theme()),
        "doom-flatwhite" => Some(doom_flatwhite::theme()),
        "doom-acario-light" => Some(doom_acario_light::theme()),
        _ => None,
    }
}

/// List all available theme names
pub fn list_themes() -> Vec<&'static str> {
    THEMES.iter().map(|t| t.name).collect()
}

/// Get default theme name
pub fn default_theme_name() -> &'static str {
    "doom-henna"
}
