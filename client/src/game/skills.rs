// The skill tree: souls (gathered from the enemies he defeats) spent on
// lasting boosts, a level at a time, each skill reached through the one
// before it from the root (the start of the game's incremental side). Kept
// between runs, as "id:level" pairs. What the levels add up to: Boosts.

use crate::i18n::{text, Text};
use std::collections::BTreeMap;

pub struct Skill {
    pub id: &'static str,
    pub name: fn(&dyn Text) -> &'static str,
    pub what: fn(&dyn Text) -> &'static str, // a level's effect
    // each level's cost, in souls (as many levels as there are costs)
    pub costs: &'static [u64],
    // what each level needs first (needs[0] the first level's, its first
    // the skill it hangs from in the tree), as (skill, its level): some
    // levels need another skill's, so the branches interleave
    pub needs: &'static [&'static [(&'static str, u8)]],
    pub at: (f32, f32), // where in the tree (across, down: a grid's cells)
}

impl Skill {
    pub fn levels(&self) -> u8 {
        self.costs.len() as u8
    }

    // The skill it hangs from in the tree (its first level's first need).
    pub fn parent(&self) -> Option<&'static str> {
        self.needs.first().and_then(|n| n.first()).map(|n| n.0)
    }
}

// (each skill's levels' costs rise less than doubling, unevenly; the
// branches cross: vitality's third level waits on healing, healing's
// mending on more vitality, and so on)
pub const SKILLS: &[Skill] = &[
    Skill { id: "soul", name: |t| t.skill_soul_name(), what: |t| t.skill_soul_what(), costs: &[1], needs: &[&[]], at: (0.0, 0.0) },
    Skill { id: "vitality", name: |t| t.skill_vitality_name(), what: |t| t.skill_vitality_what(), costs: &[2, 4, 8, 14], needs: &[&[("soul", 1)], &[], &[("healing", 1)], &[("mending", 1)]], at: (-1.0, -1.0) },
    Skill { id: "healing", name: |t| t.spell_healing(), what: |t| t.skill_healing_what(), costs: &[5], needs: &[&[("vitality", 2)]], at: (-1.0, -2.0) },
    Skill { id: "mending", name: |t| t.skill_mending_name(), what: |t| t.skill_mending_what(), costs: &[5, 9, 15], needs: &[&[("healing", 1)], &[("vitality", 3)], &[("chrono", 1)]], at: (-1.0, -3.0) },
    Skill { id: "chrono", name: |t| t.skill_chrono_name(), what: |t| t.skill_chrono_what(), costs: &[10, 14, 20, 28], needs: &[&[("healing", 1), ("vitality", 3)], &[("mending", 1)], &[("vitality", 4)], &[("mending", 3)]], at: (-2.0, -2.0) },
    Skill { id: "well", name: |t| t.skill_well_name(), what: |t| t.skill_well_what(), costs: &[2, 5, 10], needs: &[&[("soul", 1)], &[], &[("flow", 1)]], at: (1.0, -1.0) },
    Skill { id: "flow", name: |t| t.skill_flow_name(), what: |t| t.skill_flow_what(), costs: &[4, 8, 14], needs: &[&[("well", 2)], &[], &[("well", 3)]], at: (1.0, -2.0) },
    Skill { id: "kindling", name: |t| t.skill_kindling_name(), what: |t| t.skill_kindling_what(), costs: &[3, 6, 11], needs: &[&[("soul", 1)], &[], &[("inferno", 1)]], at: (-2.0, 0.0) },
    Skill { id: "inferno", name: |t| t.skill_inferno_name(), what: |t| t.skill_inferno_what(), costs: &[7, 14], needs: &[&[("kindling", 2)], &[("kindling", 3)]], at: (-3.0, 0.0) },
    Skill { id: "meteor", name: |t| t.spell_meteor(), what: |t| t.skill_meteor_what(), costs: &[16], needs: &[&[("inferno", 1), ("vitality", 2)]], at: (-3.0, -1.0) },
    Skill { id: "wind", name: |t| t.spell_wind_blade(), what: |t| t.skill_wind_what(), costs: &[4], needs: &[&[("soul", 1)]], at: (2.0, 0.0) },
    Skill { id: "cool", name: |t| t.skill_cool_name(), what: |t| t.skill_cool_what(), costs: &[3, 6, 12], needs: &[&[("wind", 1)], &[], &[("frost", 1)]], at: (3.0, 0.0) },
    Skill { id: "frost", name: |t| t.spell_frost_lance(), what: |t| t.skill_frost_what(), costs: &[7], needs: &[&[("wind", 1), ("cool", 1)]], at: (2.0, 1.0) },
    Skill { id: "gravity", name: |t| t.spell_gravity_well(), what: |t| t.skill_gravity_what(), costs: &[14], needs: &[&[("frost", 1), ("static", 1)]], at: (3.0, 1.0) },
    Skill { id: "lightning", name: |t| t.spell_lightning_strike(), what: |t| t.skill_lightning_what(), costs: &[4], needs: &[&[("soul", 1)]], at: (1.0, 1.0) },
    Skill { id: "static", name: |t| t.skill_static_name(), what: |t| t.skill_static_what(), costs: &[5, 11], needs: &[&[("lightning", 1)], &[("arcane", 1)]], at: (1.0, 2.0) },
    Skill { id: "arcane", name: |t| t.spell_arcane_missiles(), what: |t| t.skill_arcane_what(), costs: &[8], needs: &[&[("soul", 1), ("lightning", 1)]], at: (0.0, 1.0) },
    Skill { id: "familiar", name: |t| t.spell_spirit_familiar(), what: |t| t.skill_familiar_what(), costs: &[15], needs: &[&[("arcane", 1), ("reaper", 1)]], at: (0.0, 2.0) },
    Skill { id: "reaper", name: |t| t.skill_reaper_name(), what: |t| t.skill_reaper_what(), costs: &[8, 16, 30], needs: &[&[("soul", 1)], &[("slot2", 1)], &[("familiar", 1)]], at: (-1.0, 1.0) },
    Skill { id: "glacier", name: |t| t.skill_glacier_name(), what: |t| t.skill_glacier_what(), costs: &[6, 10, 16], needs: &[&[("frost", 1)], &[("cool", 2)], &[]], at: (2.0, 2.0) },
    Skill { id: "singularity", name: |t| t.skill_singularity_name(), what: |t| t.skill_singularity_what(), costs: &[12, 20], needs: &[&[("gravity", 1)], &[("glacier", 2)]], at: (4.0, 1.0) },
    Skill { id: "volley", name: |t| t.skill_volley_name(), what: |t| t.skill_volley_what(), costs: &[8, 13, 20], needs: &[&[("arcane", 1)], &[("static", 1)], &[("familiar", 1)]], at: (-1.0, 2.0) },
    Skill { id: "fork", name: |t| t.skill_fork_name(), what: |t| t.skill_fork_what(), costs: &[8, 15], needs: &[&[("static", 1)], &[("volley", 1)]], at: (1.0, 3.0) },
    Skill { id: "kindred", name: |t| t.skill_kindred_name(), what: |t| t.skill_kindred_what(), costs: &[12, 20], needs: &[&[("familiar", 1)], &[("reaper", 2)]], at: (0.0, 3.0) },
    Skill { id: "swift", name: |t| t.skill_swift_name(), what: |t| t.skill_swift_what(), costs: &[3, 6, 10], needs: &[&[("soul", 1)], &[("vitality", 1)], &[("wind", 1)]], at: (-2.0, 1.0) },
    Skill { id: "aegis", name: |t| t.skill_aegis_name(), what: |t| t.skill_aegis_what(), costs: &[5, 10], needs: &[&[("well", 1)], &[("flow", 2)]], at: (2.0, -1.0) },
    Skill { id: "starfall", name: |t| t.skill_starfall_name(), what: |t| t.skill_starfall_what(), costs: &[12, 20], needs: &[&[("meteor", 1)], &[("inferno", 2)]], at: (-4.0, -1.0) },
    Skill { id: "slot2", name: |t| t.skill_slot_name(), what: |t| t.skill_slot2_what(), costs: &[5], needs: &[&[("soul", 1)]], at: (0.0, -1.0) },
    Skill { id: "slot3", name: |t| t.skill_slot_name(), what: |t| t.skill_slot3_what(), costs: &[14], needs: &[&[("slot2", 1), ("vitality", 2)]], at: (0.0, -2.0) },
    Skill { id: "slot4", name: |t| t.skill_slot_name(), what: |t| t.skill_slot4_what(), costs: &[30], needs: &[&[("slot3", 1), ("flow", 1)]], at: (0.0, -3.0) },
];

// Whether a skill unlocks something new (a spell, or a spell slot), rather
// than improving: drawn as a hexagon.
pub fn unlocks(s: &Skill) -> bool {
    matches!(s.id, "lightning" | "wind" | "healing" | "frost" | "arcane" | "meteor" | "gravity" | "familiar" | "slot2" | "slot3" | "slot4")
}

pub fn skill(id: &str) -> Option<&'static Skill> {
    SKILLS.iter().find(|s| s.id == id)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skills {
    levels: BTreeMap<String, u8>,
}

// What his skills add up to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Boosts {
    pub health: i32,      // more health
    pub mana: f32,        // more mana
    pub regen: f32,       // mana refilling, times
    pub heal: f32,        // healing, times
    pub charge: f32,      // fire bolts charging, times
    pub blast: f32,       // fire bursts' reach, times
    pub heat: f32,        // wind blades' heat, times
    pub lock: f32,        // lightning's lock-on time, times
    pub soul_mana: f32,   // more mana a soul brings
    pub souls: u32,       // souls a defeat brings
    pub spells: [bool; super::spells::N_SPELLS], // the spells he can cast (the fire bolt always)
    pub slots: usize,      // his spell slots (1 to 4)
    pub revive: f32,       // the part of his health a chrono revive brings him back with (0: none)
    pub frost: f32,        // a frost lance's chill, times
    pub well_reach: f32,   // a gravity well's reach, times
    pub well_harm: f32,    // and its burst's harm, times
    pub locks: usize,      // more arcane missile locks
    pub chain: usize,      // more leaps of his chain lightning
    pub familiar: f32,     // his familiar's stay, and its pace, times
    pub dash: f32,         // the dash's cooldown, times
    pub shield: f32,       // the shield's mana, times
    pub meteor: f32,       // a meteor's calling down, times
}

impl Skills {
    pub fn level(&self, id: &str) -> u8 {
        self.levels.get(id).copied().unwrap_or(0)
    }

    // The next level's cost, if there is one and it can be had (what it
    // needs learnt).
    pub fn cost(&self, s: &Skill) -> Option<u64> {
        let l = self.level(s.id) as usize;
        let open = self.wanting(s).is_none();
        (l < s.costs.len() && open).then(|| s.costs[l])
    }

    // What the next level of `s` still needs, if anything: (skill, level).
    pub fn wanting(&self, s: &Skill) -> Option<(&'static Skill, u8)> {
        let l = self.level(s.id) as usize;
        s.needs.get(l).into_iter().flat_map(|n| n.iter()).find(|(id, at)| self.level(id) < *at).and_then(|&(id, at)| skill(id).map(|k| (k, at)))
    }

    // Learns the next level of `s` with `souls`, if there are enough.
    pub fn buy(&mut self, s: &Skill, souls: &mut u64) -> bool {
        match self.cost(s) {
            Some(c) if *souls >= c => {
                *souls -= c;
                *self.levels.entry(s.id.to_string()).or_insert(0) += 1;
                true
            }
            _ => false,
        }
    }

    pub fn boosts(&self) -> Boosts {
        let l = |id: &str| self.level(id) as f32;
        Boosts {
            health: 2 * self.level("vitality") as i32,
            mana: 20.0 * l("well"),
            regen: 1.0 + 0.25 * l("flow"),
            heal: 1.0 + 0.3 * l("mending"),
            charge: 1.0 + 0.25 * l("kindling"),
            blast: 1.0 + 0.2 * l("inferno"),
            heat: 0.85f32.powf(l("cool")),
            lock: 0.8f32.powf(l("static")),
            soul_mana: 5.0 * l("soul"),
            souls: 1 + self.level("reaper") as u32,
            spells: [
                true,
                self.level("lightning") > 0,
                self.level("wind") > 0,
                self.level("healing") > 0,
                self.level("frost") > 0,
                self.level("arcane") > 0,
                self.level("meteor") > 0,
                self.level("gravity") > 0,
                self.level("familiar") > 0,
            ],
            revive: 0.25 * l("chrono"),
            frost: 1.0 + 0.3 * l("glacier"),
            well_reach: 1.0 + 0.2 * l("singularity"),
            well_harm: 1.0 + 0.5 * l("singularity"),
            locks: self.level("volley") as usize,
            chain: self.level("fork") as usize,
            familiar: 1.0 + 0.3 * l("kindred"),
            dash: 0.85f32.powf(l("swift")),
            shield: 0.8f32.powf(l("aegis")),
            meteor: 0.75f32.powf(l("starfall")),
            slots: 1 + ["slot2", "slot3", "slot4"].iter().filter(|s| self.level(s) > 0).count(),
        }
    }

    pub fn saved(&self) -> String {
        self.levels.iter().map(|(k, v)| format!("{k}:{v}")).collect::<Vec<_>>().join(",")
    }

    pub fn restored(s: &str) -> Skills {
        let mut out = Skills::default();
        for pair in s.split(',') {
            if let Some((k, v)) = pair.split_once(':') {
                if let (Some(sk), Ok(v)) = (skill(k), v.parse::<u8>()) {
                    out.levels.insert(sk.id.to_string(), v.min(sk.levels()));
                }
            }
        }
        out
    }
}

impl Default for Boosts {
    fn default() -> Self {
        Skills::default().boosts()
    }
}

// A skill's name and effect, as shown.
pub fn shown(s: &Skill) -> (&'static str, &'static str) {
    ((s.name)(text()), (s.what)(text()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_need_is_a_skill_s_level() {
        for s in SKILLS {
            assert_eq!(s.needs.len(), s.costs.len(), "{}", s.id);
            for &(id, at) in s.needs.iter().flat_map(|n| n.iter()) {
                let k = skill(id).unwrap_or_else(|| panic!("{} needs {id}", s.id));
                assert!(at >= 1 && at <= k.levels(), "{} needs {id} {at}", s.id);
            }
        }
    }

    #[test]
    fn the_upgrades_reach_their_spells() {
        let b = Skills::restored("volley:2,fork:1,swift:1,glacier:3,singularity:1,kindred:2,aegis:1,starfall:2").boosts();
        assert_eq!((b.locks, b.chain), (2, 1));
        assert!((b.dash - 0.85).abs() < 1e-6 && (b.frost - 1.9).abs() < 1e-6 && (b.familiar - 1.6).abs() < 1e-6);
        assert!((b.well_reach - 1.2).abs() < 1e-6 && (b.well_harm - 1.5).abs() < 1e-6);
        assert!((b.shield - 0.8).abs() < 1e-6 && (b.meteor - 0.5625).abs() < 1e-6);
    }

    #[test]
    fn learnt_in_order_for_souls() {
        let mut k = Skills::default();
        let mut souls = 20;
        assert_eq!(k.boosts().spells, [true, false, false, false, false, false, false, false, false]);
        let vit = skill("vitality").unwrap();
        // (not before the root)
        assert!(!k.buy(vit, &mut souls));
        assert!(k.buy(skill("soul").unwrap(), &mut souls));
        assert!(k.buy(vit, &mut souls));
        assert!(k.buy(vit, &mut souls));
        assert_eq!((k.level("vitality"), souls), (2, 20 - 1 - 2 - 4));
        assert_eq!(k.boosts().health, 4);
        // (vitality's third level waits on healing)
        assert_eq!(k.wanting(vit).map(|(s, l)| (s.id, l)), Some(("healing", 1)));
        assert!(!k.buy(vit, &mut souls));
        assert!(k.buy(skill("healing").unwrap(), &mut souls));
        assert!(k.buy(vit, &mut souls));
        assert_eq!(souls, 20 - 1 - 2 - 4 - 5 - 8);
        assert_eq!(Skills::restored(&k.saved()), k);
    }
}
