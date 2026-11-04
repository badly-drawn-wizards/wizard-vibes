// His shield (web/shield.ts): held up (R, or LB on a controller) it stops
// the enemies' spells, at a cost in mana to raise, to hold and for each
// spell it stops; it drops when the mana runs out. He cannot cast while it
// is up. The renderer draws it (shield_layer in fut/shading.fut) from
// `amount` and `since_hit`.

use super::mana::Mana;
use super::sfx;

const RAISE_COST: f32 = 10.0;
const HOLD_COST: f32 = 15.0; // per second
const BLOCK_COST: f32 = 6.0;
const UP_RATE: f32 = 7.0; // how fast it rises and falls (1/s)

#[derive(Clone, Debug)]
pub struct Shield {
    pub amount: f32,    // 0 down .. 1 up
    pub since_hit: f32, // seconds since it stopped a spell
    pub wanted: bool,   // R held
    raised: bool,       // paid for and held
    // the sounds of raising and dropping it (sfx kinds), for the caller
    pub sounds: Vec<usize>,
}

impl Default for Shield {
    fn default() -> Self {
        Shield { amount: 0.0, since_hit: 10.0, wanted: false, raised: false, sounds: Vec::new() }
    }
}

impl Shield {
    // Up enough to stop spells.
    pub fn up(&self) -> bool {
        self.raised && self.amount > 0.5
    }

    // Up at all (casting is blocked from the moment it is raised).
    pub fn raised_now(&self) -> bool {
        self.raised
    }

    // A spell hit it.
    pub fn block(&mut self, mana: &mut Mana) {
        self.since_hit = 0.0;
        if !mana.drain(BLOCK_COST) {
            self.drop_it();
        }
    }

    // `held`: the controller's button, as well as the key; `can`: whether he
    // may raise it now (not while down, say).
    pub fn update(&mut self, dt: f32, held: bool, can: bool, mana: &mut Mana) {
        self.since_hit += dt;
        let want = (self.wanted || held) && can;
        if want && !self.raised && mana.spend(RAISE_COST) {
            self.raised = true;
            self.sounds.push(sfx::SHIELD_UP);
        } else if !want && self.raised {
            self.drop_it();
        }
        if self.raised && !mana.drain(HOLD_COST * dt) {
            self.drop_it();
        }
        self.amount += ((if self.raised { 1.0 } else { 0.0 }) - self.amount) * (1.0 - (-dt * UP_RATE).exp());
        if self.amount < 1e-3 {
            self.amount = 0.0;
        }
    }

    fn drop_it(&mut self) {
        if !self.raised {
            return;
        }
        self.raised = false;
        self.sounds.push(sfx::SHIELD_DOWN);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raise_hold_drop() {
        let mut m = Mana::default();
        let mut s = Shield::default();
        s.update(0.1, true, true, &mut m);
        assert!(s.raised_now());
        assert_eq!(s.sounds, vec![sfx::SHIELD_UP]);
        assert!((m.value - (100.0 - 10.0 - 1.5)).abs() < 1e-4);
        for _ in 0..5 {
            s.update(0.1, true, true, &mut m);
        }
        assert!(s.up());
        s.block(&mut m);
        assert_eq!(s.since_hit, 0.0);
        // held until the mana runs out
        for _ in 0..100 {
            s.update(0.1, true, true, &mut m);
        }
        assert!(!s.raised_now());
        assert_eq!(s.sounds.last(), Some(&sfx::SHIELD_DOWN));
        // not while he cannot
        let mut m = Mana::default();
        let mut s = Shield::default();
        s.update(0.1, true, false, &mut m);
        assert!(!s.raised_now());
        assert_eq!(m.value, 100.0);
    }
}
