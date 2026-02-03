// Theme definitions for TUI
//
// Each theme module exports a Theme instance with colors ported from doom-emacs themes.

use crate::themes::theme::Theme;

crate::register_themes! {
    // Dark themes
    one => { name: "one", display: "One", dark: true },
    dracula => { name: "dracula", display: "Dracula", dark: true },
    gruvbox => { name: "gruvbox", display: "Gruvbox", dark: true },
    nord => { name: "nord", display: "Nord", dark: true },
    tokyo_night => { name: "tokyo-night", display: "Tokyo Night", dark: true },
    monokai_pro => { name: "monokai-pro", display: "Monokai Pro", dark: true },
    palenight => { name: "palenight", display: "Palenight", dark: true },
    solarized_dark => { name: "solarized-dark", display: "Solarized Dark", dark: true },
    material => { name: "material", display: "Material", dark: true },
    horizon => { name: "horizon", display: "Horizon", dark: true },
    oceanic_next => { name: "oceanic-next", display: "Oceanic Next", dark: true },
    zenburn => { name: "zenburn", display: "Zenburn", dark: true },
    snazzy => { name: "snazzy", display: "Snazzy", dark: true },
    city_lights => { name: "city-lights", display: "City Lights", dark: true },
    ayu_dark => { name: "ayu-dark", display: "Ayu Dark", dark: true },
    challenger_deep => { name: "challenger-deep", display: "Challenger Deep", dark: true },
    fairy_floss => { name: "fairy-floss", display: "Fairy Floss", dark: true },
    laserwave => { name: "laserwave", display: "Laserwave", dark: true },
    moonlight => { name: "moonlight", display: "Moonlight", dark: true },
    vibrant => { name: "vibrant", display: "Vibrant", dark: true },
    wilmersdorf => { name: "wilmersdorf", display: "Wilmersdorf", dark: true },
    spacegrey => { name: "spacegrey", display: "Spacegrey", dark: true },
    rouge => { name: "rouge", display: "Rouge", dark: true },
    old_hope => { name: "old-hope", display: "Old Hope", dark: true },
    nova => { name: "nova", display: "Nova", dark: true },
    molokai => { name: "molokai", display: "Molokai", dark: true },
    ir_black => { name: "ir-black", display: "IR Black", dark: true },
    henna => { name: "henna", display: "Henna", dark: true },
    ephemeral => { name: "ephemeral", display: "Ephemeral", dark: true },
    miramare => { name: "miramare", display: "Miramare", dark: true },
    acario_dark => { name: "acario-dark", display: "Acario Dark", dark: true },
    shades_of_purple => { name: "shades-of-purple", display: "Shades of Purple", dark: true },
    outrun_electric => { name: "outrun-electric", display: "Outrun Electric", dark: true },
    homage_black => { name: "homage-black", display: "Homage Black", dark: true },
    iosvkem => { name: "iosvkem", display: "Iosvkem", dark: true },
    // Light themes
    one_light => { name: "one-light", display: "One Light", dark: false },
    gruvbox_light => { name: "gruvbox-light", display: "Gruvbox Light", dark: false },
    nord_light => { name: "nord-light", display: "Nord Light", dark: false },
    solarized_light => { name: "solarized-light", display: "Solarized Light", dark: false },
    tomorrow_day => { name: "tomorrow-day", display: "Tomorrow Day", dark: false },
    ayu_light => { name: "ayu-light", display: "Ayu Light", dark: false },
    earl_grey => { name: "earl-grey", display: "Earl Grey", dark: false },
    flatwhite => { name: "flatwhite", display: "Flatwhite", dark: false },
    acario_light => { name: "acario-light", display: "Acario Light", dark: false },
}
