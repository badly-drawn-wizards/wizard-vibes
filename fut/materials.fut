-- Material ids, and how hits carry their layer.

import "sdf"


-- Left and right are the wizard's own: he faces +z, so his left is +x.

-- Material ids.
def m_robe : i32 = 1
def m_skin : i32 = 2
def m_beard : i32 = 3
def m_hat : i32 = 4
def m_gold : i32 = 5
def m_eye : i32 = 6
def m_wood : i32 = 7
def m_ground : i32 = 9
def m_leather : i32 = 10
def m_tome : i32 = 11
def m_pages : i32 = 12
def m_potion : i32 = 20 -- 20, 21, 22: one per flask
def m_rock : i32 = 13
def m_terrain : i32 = 14
def m_staff : i32 = 15
def m_cork : i32 = 16
def m_brow : i32 = 17
def m_band : i32 = 18
def m_wall : i32 = 24 -- 24 .. 28: the structure parts (part_* in world.fut)
def m_thatch : i32 = 25
def m_tower : i32 = 26
def m_slate : i32 = 27
def m_sandstone : i32 = 28
def m_water : i32 = 29

---------------------------------------------------------------------
-- Layers. Every part of the scene belongs to one of the layers_* (generated
-- from the "layers" parameter in params.json), and the scene functions take
-- a mask of the layers to include. A hit's material id carries its layer in
-- bits 8 and up (material ids are below 256).

def tag (layer: i32) (h: hit) : hit = (h.0, h.1 | (layer << 8))
def hit_material (m: i32) : i32 = m & 255
def hit_layer (m: i32) : i32 = m >> 8
def has (mask: i32) (layer: i32) : bool = mask & layer != 0
def nothing : hit = (1e9, 0) -- (finite: shadow maths squares distances)
