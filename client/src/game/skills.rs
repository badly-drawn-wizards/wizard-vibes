// The skill tree: souls (gathered from the enemies he defeats) spent on
// lasting boosts, a level at a time, each skill reached through the one
// before it from the root (the start of the game's incremental side). Kept
// between runs, as "id:level" pairs. What the levels add up to: Boosts.

use crate::i18n::tr;
use std::collections::BTreeMap;

pub struct Skill {
    pub id: &'static str,
    pub name: &'static str, // (English: the i18n key)
    pub what: &'static str, // a level's effect, likewise
    pub levels: u8,
    pub cost: u64, // the first level's; each after costs twice the one before
    pub needs: Option<&'static str>,
    pub at: (f32, f32), // where in the tree (-1..1 across, -1..1 down)
}

pub const SKILLS: &[Skill] = &[
    Skill { id: "soul", name: "Soul bond", what: "Souls restore 5 more mana", levels: 1, cost: 1, needs: None, at: (0.0, 0.0) },
    Skill { id: "vitality", name: "Vitality", what: "2 more health", levels: 3, cost: 3, needs: Some("soul"), at: (-0.45, -0.5) },
    Skill { id: "mending", name: "Mending", what: "Healing 30% faster", levels: 3, cost: 5, needs: Some("vitality"), at: (-0.45, -1.0) },
    Skill { id: "well", name: "Mana well", what: "20 more mana", levels: 3, cost: 3, needs: Some("soul"), at: (0.45, -0.5) },
    Skill { id: "flow", name: "Mana flow", what: "Mana refills 25% faster", levels: 3, cost: 5, needs: Some("well"), at: (0.45, -1.0) },
    Skill { id: "kindling", name: "Kindling", what: "Fire bolts charge 25% faster", levels: 3, cost: 4, needs: Some("soul"), at: (-0.9, 0.2) },
    Skill { id: "inferno", name: "Inferno", what: "Fire bursts reach 20% further", levels: 2, cost: 8, needs: Some("kindling"), at: (-0.9, 0.8) },
    Skill { id: "cool", name: "Cool winds", what: "Wind blades heat 15% less", levels: 3, cost: 4, needs: Some("soul"), at: (0.9, 0.2) },
    Skill { id: "static", name: "Static", what: "Lightning locks on 20% sooner", levels: 2, cost: 6, needs: Some("soul"), at: (0.45, 0.8) },
    Skill { id: "reaper", name: "Reaper", what: "One more soul from each defeat", levels: 3, cost: 10, needs: Some("soul"), at: (-0.25, 0.9) },
];

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
}

impl Skills {
    pub fn level(&self, id: &str) -> u8 {
        self.levels.get(id).copied().unwrap_or(0)
    }

    // The next level's cost, if there is one and it can be had (the skill
    // before it learnt).
    pub fn cost(&self, s: &Skill) -> Option<u64> {
        let l = self.level(s.id);
        let open = s.needs.is_none_or(|n| self.level(n) > 0);
        (l < s.levels && open).then(|| s.cost << l)
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
                    out.levels.insert(sk.id.to_string(), v.min(sk.levels));
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
    (tr(s.name), tr(s.what))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learnt_in_order_for_souls() {
        let mut k = Skills::default();
        let mut souls = 20;
        let vit = skill("vitality").unwrap();
        // (not before the root)
        assert!(!k.buy(vit, &mut souls));
        assert!(k.buy(skill("soul").unwrap(), &mut souls));
        assert!(k.buy(vit, &mut souls));
        assert!(k.buy(vit, &mut souls));
        assert_eq!((k.level("vitality"), souls), (2, 20 - 1 - 3 - 6));
        assert_eq!(k.boosts().health, 4);
        assert_eq!(Skills::restored(&k.saved()), k);
    }
}
