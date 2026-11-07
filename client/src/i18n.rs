// The words the player reads, in the language chosen (the settings'
// Language row; kept between runs), compiled in from client/lang.json: a
// table per language, of strings or tables of them, its keys snake_case
// (see the wizard-i18n crate). Each string is a method of Text, named by
// its path (en.pause.title: pause_title); one with values in it ({n})
// takes them, by name, and is formatted with format!. Every language has
// every string (else the build fails, saying which). The parameters'
// labels, docs, group names and options (param.<id>, doc.<id>, group.<id>,
// option.<id>.<option, snake_case>), named in params.json, are looked up
// by path at run time: see key. Adding a language: its table in lang.json.

use std::sync::atomic::{AtomicU8, Ordering};

#[wizard_i18n::langs("lang.json")]
pub trait Text: Sync {
    fn lang_name(&self) -> &'static str; // English

    // pause
    fn pause_title(&self) -> &'static str; // Paused
    fn pause_settings(&self) -> &'static str; // Settings
    fn pause_controls(&self) -> &'static str; // Controls
    fn pause_parameters(&self) -> &'static str; // Parameters
    fn pause_parameters_with_shader(&self) -> &'static str; // (the parameters come with the shader)

    // tree
    fn tree_title(&self) -> &'static str; // Skill tree
    fn tree_next_level(&self, souls: u64) -> String; // Next level: {souls} souls
    fn tree_learnt(&self) -> &'static str; // Learnt in full
    fn tree_needs(&self, skill: &str, level: u8) -> String; // Needs {skill} {level}
    fn tree_learn(&self) -> &'static str; // Learn
    fn tree_back(&self) -> &'static str; // Back

    // settings
    fn settings_language(&self) -> &'static str; // Language
    fn settings_mode(&self) -> &'static str; // Mode
    fn settings_camera(&self) -> &'static str; // Camera
    fn settings_enemies(&self) -> &'static str; // Enemies
    fn settings_platform(&self) -> &'static str; // Platform
    fn settings_song(&self) -> &'static str; // Song
    fn settings_volume(&self) -> &'static str; // Volume
    fn settings_music(&self) -> &'static str; // Music
    fn settings_effects(&self) -> &'static str; // Effects
    fn settings_on(&self) -> &'static str; // on
    fn settings_off(&self) -> &'static str; // off
    fn settings_quality(&self) -> &'static str; // Quality
    fn settings_render_scale(&self) -> &'static str; // Render scale
    fn settings_dynamic(&self) -> &'static str; // dynamic
    fn settings_target_fps(&self) -> &'static str; // Target fps
    fn settings_progress(&self) -> &'static str; // Progress
    fn settings_reset_progress(&self) -> &'static str; // Reset progress
    fn settings_reset_progress_tip(&self) -> &'static str; // Forget every soul and skill, and start afresh
    fn settings_reset_title(&self) -> &'static str; // Reset progress?
    fn settings_reset_body(&self, souls: u64) -> String; // All {souls} souls and every skill learnt will be lost.
    fn settings_reset_yes(&self) -> &'static str; // Reset
    fn settings_reset_no(&self) -> &'static str; // Cancel
    fn settings_dynamic_tip(&self) -> &'static str; // Scale the resolution to keep the target frame rate

    // sides
    fn sides_one_vs_all(&self) -> &'static str; // 1 vs all
    fn sides_one_vs_all_tip(&self) -> &'static str; // The enemies all fight him
    fn sides_free_for_all(&self) -> &'static str; // Free for all
    fn sides_free_for_all_tip(&self) -> &'static str; // The enemies fight each other too

    // platform
    fn platform_rock(&self) -> &'static str; // Rock
    fn platform_ice(&self) -> &'static str; // Ice
    fn platform_magic_carpet(&self) -> &'static str; // Magic carpet

    // camera
    fn camera_over_shoulder(&self) -> &'static str; // Over the shoulder
    fn camera_over_shoulder_tip(&self) -> &'static str; // The camera looks past his right shoulder
    fn camera_centred(&self) -> &'static str; // Centred
    fn camera_centred_tip(&self) -> &'static str; // The camera looks at him, centred
    fn camera_picture(&self) -> &'static str; // Picture
    fn camera_picture_tip(&self) -> &'static str; // Hold the scene still and fly the camera freely to frame a picture (Escape to come back)

    // picture
    fn picture_title(&self) -> &'static str; // Picture mode

    // audio
    fn audio_music_on_or_off(&self) -> &'static str; // Music on or off (M)
    fn audio_effects_on_or_off(&self) -> &'static str; // Sound effects on or off
    fn audio_no_sound(&self, error: &str) -> String; // No sound: {error}
    fn audio_music(&self) -> &'static str; // music
    fn audio_fx(&self) -> &'static str; // fx
    fn audio_music_on(&self) -> &'static str; // Music on (M)
    fn audio_music_off(&self) -> &'static str; // Music off (M)
    fn audio_effects_on(&self) -> &'static str; // Sound effects on
    fn audio_effects_off(&self) -> &'static str; // Sound effects off

    // mode
    fn mode_fixed(&self) -> &'static str; // Fixed
    fn mode_flight(&self) -> &'static str; // Flight
    fn mode_fight(&self) -> &'static str; // Fight
    fn mode_fixed_tip(&self) -> &'static str; // Fixed: on the flight path
    fn mode_flight_tip(&self) -> &'static str; // Flight: free flight
    fn mode_fight_tip(&self) -> &'static str; // Fight: enemies about
    fn mode_cycles(&self, mode: &str) -> String; // {mode} (F cycles)

    // bar
    fn bar_mode_tip(&self) -> &'static str; // Mode (F)
    fn bar_compiling(&self) -> &'static str; // compiling…
    fn bar_loading(&self) -> &'static str; // loading…
    fn bar_paused(&self) -> &'static str; //  · paused
    fn bar_controller(&self) -> &'static str; //  · controller
    fn bar_dynamic(&self) -> &'static str; //  dynamic
    fn bar_fold(&self) -> &'static str; // Fold the bar
    fn bar_unfold(&self) -> &'static str; // Unfold the bar

    // quality
    fn quality_low(&self) -> &'static str; // Low
    fn quality_med(&self) -> &'static str; // Med
    fn quality_high(&self) -> &'static str; // High
    fn quality_tip(&self, level: &str, loading: &str) -> String; // Quality: {level}{loading}
    fn quality_loading(&self) -> &'static str; //  (loading…)

    // controls
    fn controls_keyboard(&self) -> &'static str; // Keyboard
    fn controls_controller(&self) -> &'static str; // Controller
    fn controls_fly(&self) -> &'static str; // Fly (controller)
    fn controls_camera(&self) -> &'static str; // Camera (click to take the mouse)
    fn controls_mouse_wheel(&self) -> &'static str; // mouse · wheel
    fn controls_try_again(&self) -> &'static str; // Try again (defeated)
    fn controls_swap_xy(&self) -> &'static str; // Swap X and Y
    fn controls_swap_xy_tip(&self) -> &'static str; // For controllers that report X and Y the other way round
    fn controls_reset(&self) -> &'static str; // Reset to defaults
    fn controls_press(&self) -> &'static str; // press…
    fn controls_waiting_tip(&self) -> &'static str; // Press a key, or a button (Escape gives up)
    fn controls_choose_tip(&self) -> &'static str; // Choose, then press the key or button to bind

    // action
    fn action_forward(&self) -> &'static str; // Forward
    fn action_back(&self) -> &'static str; // Back
    fn action_left(&self) -> &'static str; // Left
    fn action_right(&self) -> &'static str; // Right
    fn action_climb(&self) -> &'static str; // Climb
    fn action_descend(&self) -> &'static str; // Descend
    fn action_dash(&self) -> &'static str; // Dash
    fn action_cast(&self) -> &'static str; // Cast
    fn action_shield(&self) -> &'static str; // Shield (hold)
    fn action_zoom_in(&self) -> &'static str; // Zoom in
    fn action_zoom_out(&self) -> &'static str; // Zoom out
    fn action_pause(&self) -> &'static str; // Pause / menu
    fn action_spell_slot(&self, n: usize) -> String; // Spell slot {n}

    // mouse
    fn mouse_left(&self) -> &'static str; // Left click
    fn mouse_right(&self) -> &'static str; // Right click
    fn mouse_middle(&self) -> &'static str; // Middle click
    fn mouse_mouse4(&self) -> &'static str; // Mouse 4
    fn mouse_mouse5(&self) -> &'static str; // Mouse 5

    // spell
    fn spell_fire_bolt(&self) -> &'static str; // Fire bolt
    fn spell_lightning_strike(&self) -> &'static str; // Lightning strike
    fn spell_wind_blade(&self) -> &'static str; // Wind blade
    fn spell_healing(&self) -> &'static str; // Healing
    fn spell_frost_lance(&self) -> &'static str; // Frost lance
    fn spell_arcane_missiles(&self) -> &'static str; // Arcane missiles
    fn spell_meteor(&self) -> &'static str; // Meteor
    fn spell_gravity_well(&self) -> &'static str; // Gravity well
    fn spell_spirit_familiar(&self) -> &'static str; // Spirit familiar

    // hud
    fn hud_health(&self) -> &'static str; // Health
    fn hud_mana(&self) -> &'static str; // Mana
    fn hud_dash(&self) -> &'static str; // Dash
    fn hud_boss(&self) -> &'static str; // Archmage
    fn hud_give_up(&self) -> &'static str; // Hold to give up
    fn hud_rewound(&self) -> &'static str; // Time rewound

    // status (a structure's blessing, near it)
    fn status_hearth(&self) -> &'static str; // Hearth
    fn status_hearth_what(&self) -> &'static str; // Health comes back, slowly
    fn status_ley_line(&self) -> &'static str; // Ley line
    fn status_ley_line_what(&self) -> &'static str; // Mana refills twice as fast
    fn status_ancient_ward(&self) -> &'static str; // Ancient ward
    fn status_ancient_ward_what(&self) -> &'static str; // Spells cool down half again as fast

    // defeat
    fn defeat_title(&self) -> &'static str; // Defeated
    fn defeat_try_again(&self) -> &'static str; // Try again
    fn defeat_by_fire(&self) -> &'static str; // Burnt to cinders
    fn defeat_by_lightning(&self) -> &'static str; // Struck down
    fn defeat_by_wind(&self) -> &'static str; // Torn apart by the wind
    fn defeat_undone(&self) -> &'static str; // Undone
    fn defeat_none_defeated(&self) -> &'static str; // No enemies defeated
    fn defeat_one_defeated(&self) -> &'static str; // 1 enemy defeated
    fn defeat_many_defeated(&self, n: u32) -> String; // {n} enemies defeated

    // params
    fn params_copy_changes(&self) -> &'static str; // Copy changes
    fn params_copy_changes_tip(&self) -> &'static str; // Every value that differs from its default, as JSON
    fn params_reset_all(&self) -> &'static str; // Reset all
    fn params_copied(&self) -> &'static str; // copied
    fn params_reset(&self) -> &'static str; // Reset
    fn params_reset_group(&self, group: &str) -> String; // Reset {group}
    fn params_compile_time(&self) -> &'static str; // Compile-time: changing it rebuilds the shader
    fn params_compile_time_fixed(&self) -> &'static str; // Compile-time: fixed without the dev server
    fn params_recompile(&self) -> &'static str; // Recompile
    fn params_recompile_n(&self, n: usize) -> String; // Recompile ({n})
    fn params_recompile_tip(&self) -> &'static str; // Rebuild the shader with the compile-time parameters changed (marked ●)
    fn params_nothing_to_recompile(&self) -> &'static str; // Nothing changed to recompile
    fn params_changed_tip(&self) -> &'static str; // Changed: Recompile to rebuild the shader with it

    // loadout
    fn loadout_title(&self) -> &'static str; // Loadout
    fn loadout_empty(&self) -> &'static str; // (empty)
    fn loadout_assign(&self) -> &'static str; // Assign
    fn loadout_clear(&self) -> &'static str; // Clear
    fn loadout_press(&self) -> &'static str; // press a spell button…
    fn loadout_full(&self) -> &'static str; // No slot free: clear a button first
    fn loadout_used(&self, used: usize, slots: usize) -> String; // {used} of {slots} slots used
    fn loadout_back(&self) -> &'static str; // Back

    // skill

    // skill.soul
    fn skill_soul_name(&self) -> &'static str; // Soul bond
    fn skill_soul_what(&self) -> &'static str; // Souls restore 5 more mana

    // skill.vitality
    fn skill_vitality_name(&self) -> &'static str; // Vitality
    fn skill_vitality_what(&self) -> &'static str; // 2 more health

    // skill.healing
    fn skill_healing_what(&self) -> &'static str; // Unlocks healing (channelled)

    // skill.mending
    fn skill_mending_name(&self) -> &'static str; // Mending
    fn skill_mending_what(&self) -> &'static str; // Healing 30% faster

    // skill.well
    fn skill_well_name(&self) -> &'static str; // Mana well
    fn skill_well_what(&self) -> &'static str; // 20 more mana

    // skill.flow
    fn skill_flow_name(&self) -> &'static str; // Mana flow
    fn skill_flow_what(&self) -> &'static str; // Mana refills 25% faster

    // skill.kindling
    fn skill_kindling_name(&self) -> &'static str; // Kindling
    fn skill_kindling_what(&self) -> &'static str; // Fire bolts charge 25% faster

    // skill.inferno
    fn skill_inferno_name(&self) -> &'static str; // Inferno
    fn skill_inferno_what(&self) -> &'static str; // Fire bursts reach 20% further

    // skill.wind
    fn skill_wind_what(&self) -> &'static str; // Unlocks the wind blades

    // skill.cool
    fn skill_cool_name(&self) -> &'static str; // Cool winds
    fn skill_cool_what(&self) -> &'static str; // Wind blades heat 15% less

    // skill.lightning
    fn skill_lightning_what(&self) -> &'static str; // Unlocks the lightning strike

    // skill.static
    fn skill_static_name(&self) -> &'static str; // Static
    fn skill_static_what(&self) -> &'static str; // Lightning locks on 20% sooner

    // skill.slot
    fn skill_slot_name(&self) -> &'static str; // Spell slot

    // skill.slot2
    fn skill_slot2_what(&self) -> &'static str; // A second spell slot

    // skill.slot3
    fn skill_slot3_what(&self) -> &'static str; // A third spell slot

    // skill.slot4
    fn skill_slot4_what(&self) -> &'static str; // A fourth spell slot

    // skill.frost
    fn skill_frost_what(&self) -> &'static str; // Unlocks the frost lance

    // skill.arcane
    fn skill_arcane_what(&self) -> &'static str; // Unlocks the arcane missiles

    // skill.meteor
    fn skill_meteor_what(&self) -> &'static str; // Unlocks the meteor

    // skill.gravity
    fn skill_gravity_what(&self) -> &'static str; // Unlocks the gravity well

    // skill.familiar
    fn skill_familiar_what(&self) -> &'static str; // Unlocks the spirit familiar

    // skill.chrono
    fn skill_chrono_name(&self) -> &'static str; // Chrono revive
    fn skill_chrono_what(&self) -> &'static str; // Once a run, a defeat rewinds time

    // skill.glacier
    fn skill_glacier_name(&self) -> &'static str; // Deep frost
    fn skill_glacier_what(&self) -> &'static str; // Frost lances' chill lasts 30% longer

    // skill.singularity
    fn skill_singularity_name(&self) -> &'static str; // Singularity
    fn skill_singularity_what(&self) -> &'static str; // Gravity wells reach 20% further and burst 50% harder

    // skill.volley
    fn skill_volley_name(&self) -> &'static str; // Arcane volley
    fn skill_volley_what(&self) -> &'static str; // One more arcane missile lock

    // skill.fork
    fn skill_fork_name(&self) -> &'static str; // Forked lightning
    fn skill_fork_what(&self) -> &'static str; // Chain lightning leaps to one more

    // skill.kindred
    fn skill_kindred_name(&self) -> &'static str; // Kindred spirit
    fn skill_kindred_what(&self) -> &'static str; // Your familiar stays 30% longer and casts 30% faster

    // skill.swift
    fn skill_swift_name(&self) -> &'static str; // Fleet foot
    fn skill_swift_what(&self) -> &'static str; // Dash comes back 15% sooner

    // skill.aegis
    fn skill_aegis_name(&self) -> &'static str; // Aegis
    fn skill_aegis_what(&self) -> &'static str; // The shield costs 20% less mana

    // skill.starfall
    fn skill_starfall_name(&self) -> &'static str; // Starfall
    fn skill_starfall_what(&self) -> &'static str; // Meteors are called down 25% faster

    // skill.reaper
    fn skill_reaper_name(&self) -> &'static str; // Reaper
    fn skill_reaper_what(&self) -> &'static str; // One more soul from each defeat
}

impl Lang {
    // Its own name for itself.
    pub fn name(self) -> &'static str {
        self.text().lang_name()
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn lang() -> Lang {
    Lang::ALL[LANG.load(Ordering::Relaxed) as usize % Lang::ALL.len()]
}

pub fn set_lang(l: Lang) {
    LANG.store(Lang::ALL.iter().position(|&x| x == l).unwrap_or(0) as u8, Ordering::Relaxed);
}

// The words in the language chosen.
pub fn text() -> &'static dyn Text {
    lang().text()
}

// A string by its dotted path, in the language chosen (else the path): for
// the parameters', named in params.json.
pub fn key(path: &str) -> &str {
    text().get(path).unwrap_or(path)
}

// A name as a key has it: "Edge supersampling" -> edge_supersampling.
pub fn snake(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_end_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looked_up_and_filled() {
        assert_eq!(Lang::En.text().pause_settings(), "Settings");
        assert_eq!(Lang::En.text().defeat_many_defeated(3), "3 enemies defeated");
        assert_eq!(Lang::Es.text().pause_settings(), "Ajustes");
        assert_eq!(Lang::Es.text().defeat_many_defeated(3), "3 enemigos derrotados");
        assert_eq!(Lang::Es.text().quality_tip("Alta", ""), "Calidad: Alta");
        assert_eq!(Lang::from_key("es"), Some(Lang::Es));
        // (the parameters', by path)
        assert_eq!(Lang::En.text().get("param.exposure"), Some("Exposure"));
        assert_eq!(Lang::Es.text().get("param.exposure"), Some("Exposición"));
        assert_eq!(Lang::En.text().get("option.aa.edge_supersampling"), Some("Edge supersampling"));
        assert_eq!(Lang::En.text().get("not.a.path"), None);
        assert_eq!(snake("Edge supersampling"), "edge_supersampling");
    }

    #[test]
    fn every_parameter_has_its_words() {
        // (each group's name, and its parameters' labels, docs and
        // options, through its groups within)
        fn keys(g: &serde_json::Value, out: &mut Vec<String>) {
            out.push(g["name"].as_str().unwrap().to_string());
            for p in g["params"].as_array().into_iter().flatten() {
                out.push(p["label"].as_str().unwrap().to_string());
                if let Some(d) = p["doc"].as_str() {
                    out.push(d.to_string());
                }
                for o in p["options"].as_array().into_iter().flatten() {
                    out.push(format!("option.{}.{}", p["id"].as_str().unwrap(), snake(o.as_str().unwrap())));
                }
            }
            for sub in g["groups"].as_array().into_iter().flatten() {
                keys(sub, out);
            }
        }
        let schema: serde_json::Value = serde_json::from_str(include_str!("../../params.json")).unwrap();
        let mut all = Vec::new();
        for g in schema["groups"].as_array().unwrap() {
            keys(g, &mut all);
        }
        assert!(all.len() > 100);
        for k in all {
            for l in Lang::ALL {
                assert!(l.text().get(&k).is_some(), "{} has no {k}", l.key());
            }
        }
    }
}
