// Focus navigation for the menus, by keyboard or controller: the controls
// are laid out as rows (a row a group: a set of choices, a slider, an
// accordion's header and its reset), up and down move between rows (past a
// whole group at once, to the next row's start), left and right along a row
// (in order, however its controls wrap on screen). Controls that cannot be
// used are passed over. The control under the cursor has egui's focus, so
// Enter (or the controller's A) clicks it, and a ring shows it; on a slider
// (or a choice of several), Enter takes hold of it instead: left and right
// then change its value, until Enter, Escape (B) or up or down lets go.
//
// Each frame the menu registers its controls as it draws them (row, then
// its items); moves made since the last frame are applied, against the
// rows as they were drawn then, as this frame begins. Only once the keys or
// the controller have been used: the mouse is left alone.

use egui::{Response, Ui};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Up,
    Down,
    Left,
    Right,
    // Enter on a slider: take hold of it, or let go
    Press,
    // Escape (B): let go of a slider
    Release,
}

#[derive(Default)]
pub struct Nav {
    // the row and the column (in it) under the cursor
    row: usize,
    col: usize,
    // the rows as last drawn (their items' counts, and which items are
    // sliders), and as this frame draws them
    rows: Vec<Vec<bool>>,
    drawing: Vec<Vec<bool>>,
    // which rows are a grid's (up and down between them keep the column),
    // as drawn and as drawing
    grid: Vec<bool>,
    drawing_grid: Vec<bool>,
    // moves since the last frame, and a slider's change this frame (-1, 0,
    // 1)
    pending: Vec<Move>,
    adjust: i32,
    // a slider held (left and right move its value)
    held: bool,
    // in use (the keys or the controller moved it): it holds the focus
    pub active: bool,
}

impl Nav {
    // A move to make (from a key, or the controller).
    pub fn push(&mut self, m: Move) {
        self.pending.push(m);
        self.active = true;
    }

    // The mouse was used: the cursor lets go of the focus.
    pub fn mouse(&mut self) {
        self.active = false;
        self.held = false;
    }

    // Whether the cursor is on a slider (as last drawn): Enter is the
    // cursor's, to take hold of it.
    pub fn on_slider(&self) -> bool {
        self.active && self.rows.get(self.row).and_then(|r| r.get(self.col)).copied().unwrap_or(false)
    }

    // Whether a slider is held (Escape, or B, is the cursor's, to let go).
    pub fn held(&self) -> bool {
        self.active && self.held
    }

    // The cursor to row `row`, column `col` (a menu opening, or its tab
    // changing).
    pub fn go(&mut self, row: usize, col: usize) {
        self.row = row;
        self.col = col;
    }

    // At the start of the frame: the moves since the last, against the
    // rows as they were drawn.
    pub fn begin(&mut self) {
        self.adjust = 0;
        self.drawing.clear();
        self.drawing_grid.clear();
        // (none drawn yet, as a menu opens: kept for when there are)
        if self.rows.is_empty() {
            return;
        }
        for m in std::mem::take(&mut self.pending) {
            let Some(row) = self.rows.get(self.row) else { break };
            let on_slider = row.get(self.col).copied().unwrap_or(false);
            let held = self.held && on_slider;
            let from = self.row;
            match m {
                Move::Up => self.row = self.row.saturating_sub(1),
                Move::Down => self.row = (self.row + 1).min(self.rows.len().saturating_sub(1)),
                Move::Left if held => self.adjust -= 1,
                Move::Right if held => self.adjust += 1,
                Move::Left => self.col = self.col.saturating_sub(1),
                Move::Right => self.col = (self.col + 1).min(row.len().saturating_sub(1)),
                Move::Press => self.held = on_slider && !self.held,
                Move::Release => self.held = false,
            }
            // (a new row, from its start, unless both are a grid's (the
            // column kept, as far as the row goes); a slider let go)
            if matches!(m, Move::Up | Move::Down) {
                let grid = |r: usize| self.grid.get(r).copied().unwrap_or(false);
                if grid(from) && grid(self.row) {
                    let len = self.rows.get(self.row).map_or(1, Vec::len);
                    self.col = self.col.min(len.saturating_sub(1));
                } else {
                    self.col = 0;
                }
                self.held = false;
            }
        }
    }

    // A new row (its items follow).
    pub fn row(&mut self) {
        self.drawing.push(Vec::new());
        self.drawing_grid.push(false);
    }

    // A new row of a grid: up and down between such rows keep the column.
    pub fn grid_row(&mut self) {
        self.drawing.push(Vec::new());
        self.drawing_grid.push(true);
    }

    // A control in the current row: under the cursor (and in use), it takes
    // the focus. Whether it is under the cursor.
    pub fn item(&mut self, ui: &Ui, r: &Response) -> bool {
        self.add(ui, r, false)
    }

    // A slider in the current row (or a choice of several): as item, and
    // the steps to move its value this frame (left and right, held).
    pub fn slider(&mut self, ui: &Ui, r: &Response) -> i32 {
        if self.add(ui, r, true) { self.adjust } else { 0 }
    }

    fn add(&mut self, ui: &Ui, r: &Response, slider: bool) -> bool {
        // (one that cannot be used is passed over)
        if !r.enabled() {
            return false;
        }
        if self.drawing.is_empty() {
            self.row();
        }
        let row = self.drawing.len() - 1;
        let col = self.drawing[row].len();
        self.drawing[row].push(slider);
        let here = self.active && row == self.row && col == self.col;
        if here {
            if !r.has_focus() {
                r.request_focus();
                r.scroll_to_me(None);
            }
            if self.held && slider {
                crate::game::hud::held_ring(ui, r);
            } else {
                crate::game::hud::focus_ring(ui, r);
            }
        }
        here
    }

    // At the end of the frame: the rows as drawn, the cursor kept within
    // them.
    // (a frame that drew none, as a window's first can, leaves it be)
    pub fn end(&mut self) {
        let rows = std::mem::take(&mut self.drawing);
        let grid = std::mem::take(&mut self.drawing_grid);
        let (rows, grid): (Vec<_>, Vec<_>) = rows.into_iter().zip(grid).filter(|(r, _)| !r.is_empty()).unzip();
        if rows.is_empty() {
            return;
        }
        self.rows = rows;
        self.grid = grid;
        self.row = self.row.min(self.rows.len() - 1);
        self.col = self.col.min(self.rows[self.row].len() - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // (the rows as a frame would draw them: their items' kinds)
    fn drawn(n: &mut Nav, rows: &[&[bool]]) {
        n.drawing = rows.iter().map(|r| r.to_vec()).collect();
        n.drawing_grid = vec![false; rows.len()];
        n.end();
    }

    #[test]
    fn a_grid_keeps_the_column() {
        let mut n = Nav::default();
        let draw = |n: &mut Nav| {
            n.row();
            n.drawing.last_mut().unwrap().extend([false; 3]);
            for _ in 0..3 {
                n.grid_row();
                n.drawing.last_mut().unwrap().extend([false; 2]);
            }
            n.end();
        };
        draw(&mut n);
        // into the grid: its first column; along, then down: the column kept
        n.push(Move::Down);
        n.push(Move::Right);
        n.push(Move::Down);
        n.begin();
        assert_eq!((n.row, n.col), (2, 1));
        draw(&mut n);
        // out of it, up to the tabs: from the start
        n.push(Move::Up);
        n.push(Move::Up);
        n.begin();
        assert_eq!((n.row, n.col), (0, 0));
    }

    #[test]
    fn rows_and_columns() {
        let mut n = Nav::default();
        // tabs (3), a group of five choices, a slider, one button
        let layout: &[&[bool]] = &[&[false; 3], &[false; 5], &[true], &[false]];
        drawn(&mut n, layout);
        // down past the tabs to the group, right along it to its end (not
        // past), down onto the slider: left and right move its value
        n.push(Move::Down);
        for _ in 0..7 {
            n.push(Move::Right);
        }
        n.begin();
        assert_eq!((n.row, n.col), (1, 4));
        drawn(&mut n, layout);
        n.push(Move::Down);
        n.begin();
        assert_eq!((n.row, n.col), (2, 0));
        drawn(&mut n, layout);
        // (up: the row's start)
        n.push(Move::Up);
        n.begin();
        assert_eq!((n.row, n.col), (1, 0));
        drawn(&mut n, layout);
        n.push(Move::Down);
        n.begin();
        drawn(&mut n, layout);
        // (a slider moves once held)
        n.push(Move::Right);
        n.begin();
        assert_eq!((n.row, n.col, n.adjust), (2, 0, 0));
        drawn(&mut n, layout);
        n.push(Move::Press);
        n.push(Move::Right);
        n.push(Move::Right);
        n.begin();
        assert_eq!((n.row, n.col, n.adjust), (2, 0, 2));
        drawn(&mut n, layout);
        // down and down: let go, the last row, and no further
        n.push(Move::Down);
        n.push(Move::Down);
        n.begin();
        assert_eq!((n.row, n.col, n.held), (3, 0, false));
    }
}
