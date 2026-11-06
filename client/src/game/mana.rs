// Mana: what casting and the shield draw on (web/mana.ts). It refills
// steadily, after a short pause from the last time it was drawn on.

pub const MANA: f32 = 100.0;
const REGEN: f32 = 14.0; // per second
const PAUSE: f32 = 0.6; // seconds after spending before it refills

// What each spell costs (fire bolt, lightning strike, wind blade; healing
// costs by the second instead: HEAL_DRAIN).
pub const SPELL_COST: [f32; 4] = [12.0, 25.0, 8.0, 0.0];
pub const HEAL_DRAIN: f32 = 16.0; // a second, while channelling healing

#[derive(Clone, Debug)]
pub struct Mana {
    pub value: f32,
    // seconds since something wanted more than there was
    pub short: f32,
    since: f32, // seconds since it was last spent
}

impl Default for Mana {
    fn default() -> Self {
        Mana { value: MANA, short: 99.0, since: 99.0 }
    }
}

impl Mana {
    // Spends `amount` if there is that much (else notes the shortfall).
    pub fn spend(&mut self, amount: f32) -> bool {
        if self.value < amount {
            self.short = 0.0;
            return false;
        }
        self.value -= amount;
        self.since = 0.0;
        true
    }

    // Spends up to `amount` (for draining over time): whether it covered it.
    pub fn drain(&mut self, amount: f32) -> bool {
        let ok = self.value >= amount;
        self.value = (self.value - amount).max(0.0);
        self.since = 0.0;
        if !ok {
            self.short = 0.0;
        }
        ok
    }

    pub fn update(&mut self, dt: f32) {
        self.since += dt;
        self.short += dt;
        if self.since > PAUSE {
            self.value = (self.value + REGEN * dt).min(MANA);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spend_and_refill() {
        let mut m = Mana::default();
        assert!(m.spend(SPELL_COST[1]));
        assert_eq!(m.value, 75.0);
        // no refill during the pause
        m.update(0.5);
        assert_eq!(m.value, 75.0);
        m.update(0.2);
        assert!((m.value - (75.0 + 14.0 * 0.2)).abs() < 1e-4);
        // short of it
        m.value = 5.0;
        assert!(!m.spend(8.0));
        assert_eq!(m.short, 0.0);
        assert_eq!(m.value, 5.0);
        // a drain takes what there is
        assert!(!m.drain(6.0));
        assert_eq!(m.value, 0.0);
        // full at most
        for _ in 0..100 {
            m.update(0.1);
        }
        assert_eq!(m.value, MANA);
    }
}
