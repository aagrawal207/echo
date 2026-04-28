use std::io::{self, Write};

use crossterm::{cursor, queue, style};

pub struct Picker {
    pub cursor: usize,
    // Layout cache — recomputed every draw. The placed Vec maps word index
    // to (row, col, width); row_starts maps a row to the first word index
    // on that row so up/down navigation is cheap.
    placed: Vec<(u16, u16, u16)>,
    row_starts: Vec<usize>,
    top_row: u16,
    viewport_rows: u16,
    scroll: u16,
}

pub enum Move {
    Prev,
    Next,
    Up,
    Down,
    JumpBack(i32),
    JumpForward(i32),
    Start,
    End,
}

impl Picker {
    pub fn new(start_idx: usize) -> Self {
        Self {
            cursor: start_idx,
            placed: Vec::new(),
            row_starts: Vec::new(),
            top_row: 0,
            viewport_rows: 0,
            scroll: 0,
        }
    }

    pub fn move_cursor(&mut self, m: Move, total: usize) {
        if total == 0 {
            return;
        }
        let last = total - 1;
        self.cursor = match m {
            Move::Prev => self.cursor.saturating_sub(1),
            Move::Next => (self.cursor + 1).min(last),
            Move::JumpBack(n) => self.cursor.saturating_sub(n.unsigned_abs() as usize),
            Move::JumpForward(n) => (self.cursor + n.unsigned_abs() as usize).min(last),
            Move::Start => 0,
            Move::End => last,
            Move::Up => self.cursor_on_line_delta(-1, total),
            Move::Down => self.cursor_on_line_delta(1, total),
        };
    }

    fn cursor_on_line_delta(&self, delta: i32, total: usize) -> usize {
        if self.placed.is_empty() {
            return self.cursor;
        }
        let (cur_row, cur_col, _) = self.placed[self.cursor];
        let target_row = (cur_row as i32 + delta).max(0) as u16;
        let mut best = self.cursor;
        let mut best_dx = u16::MAX;
        for (i, &(r, c, _)) in self.placed.iter().enumerate() {
            if r != target_row {
                continue;
            }
            let dx = (c as i32 - cur_col as i32).unsigned_abs() as u16;
            if dx < best_dx {
                best_dx = dx;
                best = i;
            }
        }
        best.min(total - 1)
    }
}

pub fn draw<W: Write>(
    stdout: &mut W,
    picker: &mut Picker,
    words: &[&str],
    cols: u16,
    rows: u16,
) -> io::Result<()> {
    let margin_x: u16 = 2;
    let header_rows: u16 = 2;
    let footer_rows: u16 = 2;
    let usable_cols = cols.saturating_sub(margin_x * 2).max(1);
    let viewport_rows = rows.saturating_sub(header_rows + footer_rows).max(1);
    picker.top_row = header_rows;
    picker.viewport_rows = viewport_rows;

    // Layout: greedy wrap into rows of width usable_cols. Words wider than
    // the viewport get truncated to fit so the cursor can still land on them.
    picker.placed.clear();
    picker.row_starts.clear();
    let mut row: u16 = 0;
    let mut col: u16 = 0;
    picker.row_starts.push(0);
    for (i, w) in words.iter().enumerate() {
        let wlen = w.chars().count() as u16;
        let display_len = wlen.min(usable_cols);
        let need = if col == 0 {
            display_len
        } else {
            display_len + 1
        };
        if col + need > usable_cols {
            row += 1;
            col = 0;
            picker.row_starts.push(i);
        }
        if col > 0 {
            col += 1;
        }
        picker.placed.push((row, col, display_len));
        col += display_len;
    }

    // Keep the cursor visible: scroll so the highlighted word's row is
    // within the viewport.
    let cur_row = picker.placed[picker.cursor].0;
    if cur_row < picker.scroll {
        picker.scroll = cur_row;
    } else if cur_row >= picker.scroll + viewport_rows {
        picker.scroll = cur_row + 1 - viewport_rows;
    }

    // Header
    let total = words.len();
    let pct = ((picker.cursor + 1) * 100)
        .checked_div(total)
        .unwrap_or(100);
    let title = format!(
        " pick a word — {cur}/{total} ({pct}%) — ↵ jump · Esc cancel ",
        cur = picker.cursor + 1,
    );
    queue!(
        stdout,
        cursor::MoveTo(0, 0),
        style::SetAttribute(style::Attribute::Reverse),
        style::Print(pad_right(&title, cols as usize)),
        style::SetAttribute(style::Attribute::Reset),
    )?;

    // Body
    for (i, &(r, c, w)) in picker.placed.iter().enumerate() {
        if r < picker.scroll || r >= picker.scroll + viewport_rows {
            continue;
        }
        let y = header_rows + (r - picker.scroll);
        let x = margin_x + c;
        let word = words[i];
        let shown: String = word.chars().take(w as usize).collect();
        queue!(stdout, cursor::MoveTo(x, y))?;
        if i == picker.cursor {
            queue!(
                stdout,
                style::SetForegroundColor(style::Color::Black),
                style::SetBackgroundColor(style::Color::Yellow),
                style::Print(&shown),
                style::SetForegroundColor(style::Color::Reset),
                style::SetBackgroundColor(style::Color::Reset),
            )?;
        } else {
            queue!(stdout, style::Print(&shown))?;
        }
    }

    // Footer
    let footer = " ← → step · ↑ ↓ line · b f jump 10 · Home/End start/end · ↵ pick · Esc cancel ";
    queue!(
        stdout,
        cursor::MoveTo(0, rows.saturating_sub(1)),
        style::SetAttribute(style::Attribute::Dim),
        style::Print(pad_right(footer, cols as usize)),
        style::SetAttribute(style::Attribute::Reset),
    )?;

    Ok(())
}

fn pad_right(s: &str, width: usize) -> String {
    let visible = s.chars().count();
    if visible >= width {
        s.chars().take(width).collect()
    } else {
        let mut out = String::from(s);
        out.extend(std::iter::repeat_n(' ', width - visible));
        out
    }
}
