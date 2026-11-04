// fut/materials.fut: material ids, and how hits carry their layer.

use crate::prelude::*;

pub const m_robe: i32 = 1;
pub const m_skin: i32 = 2;
pub const m_beard: i32 = 3;
pub const m_hat: i32 = 4;
pub const m_gold: i32 = 5;
pub const m_eye: i32 = 6;
pub const m_wood: i32 = 7;
pub const m_ground: i32 = 9;
pub const m_leather: i32 = 10;
pub const m_tome: i32 = 11;
pub const m_pages: i32 = 12;
pub const m_potion: i32 = 20; // 20, 21, 22: one per flask
pub const m_rock: i32 = 13;
pub const m_terrain: i32 = 14;
pub const m_staff: i32 = 15;
pub const m_cork: i32 = 16;
pub const m_brow: i32 = 17;
pub const m_band: i32 = 18;
pub const m_wall: i32 = 24; // 24 .. 28: the structure parts (part_* in world.fut)
pub const m_thatch: i32 = 25;
pub const m_tower: i32 = 26;
pub const m_slate: i32 = 27;
pub const m_sandstone: i32 = 28;
pub const m_water: i32 = 29;
pub const m_trunk: i32 = 30;
pub const m_leaves: i32 = 31;
pub const m_needles: i32 = 32;
pub const m_ice: i32 = 33;
pub const m_carpet: i32 = 34;
pub const m_enemy: i32 = 40;
pub const enemy_parts: i32 = 10;
pub const max_enemies: i32 = 6;
pub const m_enemy_full: i32 = 100;
pub const full_parts: i32 = 25;

// Layers: a hit's material id carries its layer in bits 8 and up.
#[inline]
pub fn tag(layer: i32, h: Hit) -> Hit {
    (h.0, h.1 | (layer << 8))
}
#[inline]
pub fn hit_material(m: i32) -> i32 {
    m & 255
}
#[inline]
pub fn hit_layer(m: i32) -> i32 {
    m >> 8
}
#[inline]
pub fn has(mask: i32, layer: i32) -> bool {
    mask & layer != 0
}
pub const nothing: Hit = (1e9, 0); // (finite: shadow maths squares distances)
