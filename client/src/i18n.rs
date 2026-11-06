// The words the player reads, in the language chosen (the settings'
// Language row; kept between runs). The English is the key: tr("Settings")
// is "Settings" in English and its translation in another language, or the
// English again where a table has none. A string with values in it marks
// them {} (in order: see trf). Adding a language: a Lang, its name, and a
// table of (English, translation) pairs.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Es,
}

pub const LANGS: [Lang; 2] = [Lang::En, Lang::Es];

impl Lang {
    // Its own name for itself.
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Es => "Español",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Es => "es",
        }
    }

    pub fn from_key(k: &str) -> Option<Lang> {
        LANGS.into_iter().find(|l| l.key() == k)
    }

    fn table(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Lang::En => &[],
            Lang::Es => ES,
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn lang() -> Lang {
    LANGS[LANG.load(Ordering::Relaxed) as usize % LANGS.len()]
}

pub fn set_lang(l: Lang) {
    LANG.store(LANGS.iter().position(|&x| x == l).unwrap_or(0) as u8, Ordering::Relaxed);
}

// `en` in the language chosen.
pub fn tr(en: &'static str) -> &'static str {
    tr_in(lang(), en)
}

pub fn tr_in(l: Lang, en: &'static str) -> &'static str {
    l.table().iter().find(|(k, _)| *k == en).map_or(en, |(_, v)| v)
}

// The same, its {}s filled in with `args`, in order.
pub fn trf(en: &'static str, args: &[&dyn std::fmt::Display]) -> String {
    trf_in(lang(), en, args)
}

pub fn trf_in(l: Lang, en: &'static str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::new();
    let mut args = args.iter();
    let mut parts = tr_in(l, en).split("{}");
    if let Some(first) = parts.next() {
        out.push_str(first);
    }
    for p in parts {
        if let Some(a) = args.next() {
            out.push_str(&a.to_string());
        }
        out.push_str(p);
    }
    out
}

const ES: &[(&str, &str)] = &[
    // the pause menu
    ("Paused", "En pausa"),
    ("Settings", "Ajustes"),
    ("Controls", "Controles"),
    ("Parameters", "Parámetros"),
    ("Language", "Idioma"),
    ("Mode", "Modo"),
    ("Camera", "Cámara"),
    ("Platform", "Plataforma"),
    ("Rock", "Roca"),
    ("Ice", "Hielo"),
    ("Magic carpet", "Alfombra mágica"),
    ("Song", "Canción"),
    ("Volume", "Volumen"),
    ("Music", "Música"),
    ("Effects", "Efectos"),
    ("on", "sí"),
    ("off", "no"),
    ("Quality", "Calidad"),
    ("Render scale", "Escala de render"),
    ("dynamic", "dinámica"),
    ("Target fps", "Fps objetivo"),
    ("Scale the resolution to keep the target frame rate", "Ajusta la resolución para mantener los fps objetivo"),
    ("Over the shoulder", "Sobre el hombro"),
    ("Centred", "Centrada"),
    ("Picture", "Foto"),
    ("The camera looks past his right shoulder", "La cámara mira por encima de su hombro derecho"),
    ("The camera looks at him, centred", "La cámara lo mira, centrada"),
    (
        "Hold the scene still and fly the camera freely to frame a picture (Escape to come back)",
        "Detiene la escena y deja volar la cámara para encuadrar una foto (Escape para volver)",
    ),
    ("Picture mode · click to look, fly as he would · Esc to return", "Modo foto · clic para mirar, vuela como él · Esc para volver"),
    ("Music on or off (M)", "Música sí o no (M)"),
    ("Sound effects on or off", "Efectos de sonido sí o no"),
    ("No sound: {}", "Sin sonido: {}"),
    // the modes
    ("Fixed", "Fijo"),
    ("Flight", "Vuelo"),
    ("Fight", "Combate"),
    ("Fixed: on the flight path", "Fijo: en la ruta de vuelo"),
    ("Flight: free flight", "Vuelo: vuelo libre"),
    ("Fight: enemies about", "Combate: hay enemigos"),
    ("{} (F cycles)", "{} (F cambia)"),
    ("Mode (F)", "Modo (F)"),
    // quality
    ("Low", "Baja"),
    ("Med", "Media"),
    ("High", "Alta"),
    ("Quality: {}{}", "Calidad: {}{}"),
    (" (loading…)", " (cargando…)"),
    // the controls
    ("Keyboard", "Teclado"),
    ("Controller", "Mando"),
    ("Forward", "Adelante"),
    ("Back", "Atrás"),
    ("Left", "Izquierda"),
    ("Right", "Derecha"),
    ("Climb", "Subir"),
    ("Descend", "Bajar"),
    ("Boost", "Acelerar"),
    ("Cast", "Lanzar"),
    ("Shield (hold)", "Escudo (mantener)"),
    ("Zoom in", "Acercar"),
    ("Zoom out", "Alejar"),
    ("Pause / menu", "Pausa / menú"),
    ("Fly (controller)", "Volar (mando)"),
    ("Camera (click to take the mouse)", "Cámara (clic para tomar el ratón)"),
    ("mouse · wheel", "ratón · rueda"),
    ("Try again (defeated)", "Reintentar (derrotado)"),
    ("Swap X and Y", "Intercambiar X e Y"),
    ("For controllers that report X and Y the other way round", "Para mandos que informan X e Y al revés"),
    ("Reset to defaults", "Restablecer"),
    ("press…", "pulsa…"),
    ("Press a key, or a button (Escape gives up)", "Pulsa una tecla o un botón (Escape cancela)"),
    ("Choose, then press the key or button to bind", "Elige y luego pulsa la tecla o el botón que asignar"),
    ("Left click", "Clic izquierdo"),
    ("Right click", "Clic derecho"),
    ("Middle click", "Clic central"),
    ("Mouse 4", "Ratón 4"),
    ("Mouse 5", "Ratón 5"),
    // the spells
    ("Fire bolt", "Bola de fuego"),
    ("Lightning strike", "Rayo"),
    ("Wind blade", "Cuchilla de viento"),
    ("Healing", "Curación"),
    // the fight
    ("Health", "Salud"),
    ("Mana", "Maná"),
    ("Defeated", "Derrotado"),
    ("Try again", "Reintentar"),
    ("↺ Reset", "↺ Reiniciar"),
    ("Start the fight again (View / Back on a controller)", "Empieza el combate de nuevo (Vista / Atrás en el mando)"),
    ("No enemies defeated", "Ningún enemigo derrotado"),
    ("1 enemy defeated", "1 enemigo derrotado"),
    ("{} enemies defeated", "{} enemigos derrotados"),
    ("{} defeated", "{} derrotados"),
    // the top bar
    ("compiling…", "compilando…"),
    ("loading…", "cargando…"),
    (" · paused", " · en pausa"),
    (" · controller", " · mando"),
    (" dynamic", " dinámica"),
    ("music", "música"),
    ("fx", "efectos"),
    ("Music on (M)", "Música activada (M)"),
    ("Music off (M)", "Música desactivada (M)"),
    ("Sound effects on", "Efectos de sonido activados"),
    ("Sound effects off", "Efectos de sonido desactivados"),
    ("Fold the bar", "Plegar la barra"),
    ("Unfold the bar", "Desplegar la barra"),
    // the parameters
    ("Copy changes", "Copiar cambios"),
    ("Every value that differs from its default, as JSON", "Cada valor distinto del predeterminado, en JSON"),
    ("Reset all", "Restablecer todo"),
    ("copied", "copiado"),
    ("Reset", "Restablecer"),
    ("Reset {}", "Restablecer {}"),
    ("Compile-time: changing it rebuilds the shader", "De compilación: cambiarlo recompila el shader"),
    ("Compile-time: fixed without the dev server", "De compilación: fijo sin el servidor de desarrollo"),
    ("(the parameters come with the shader)", "(los parámetros llegan con el shader)"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looked_up_and_filled() {
        assert_eq!(tr_in(Lang::En, "Settings"), "Settings");
        assert_eq!(trf_in(Lang::En, "{} enemies defeated", &[&3]), "3 enemies defeated");
        assert_eq!(tr_in(Lang::Es, "Settings"), "Ajustes");
        assert_eq!(trf_in(Lang::Es, "{} enemies defeated", &[&3]), "3 enemigos derrotados");
        assert_eq!(tr_in(Lang::Es, "not in the table"), "not in the table");
    }
}
