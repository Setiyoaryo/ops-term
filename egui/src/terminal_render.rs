//! Paint terminal rows in color runs, keeping glyph shapes and layout work small.
use super::{appearance::Colors, line_highlights, theme, Session};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Flags;
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Vec2};

pub struct Paint<'a> {
    pub origin: Pos2,
    pub colors: Colors,
    pub font: FontId,
    pub cell: Vec2,
    pub selection: Option<(Point, Point)>,
    pub hover: Option<(Point, Point)>,
    pub rules: &'a [(String, [u8; 3], bool)],
    pub search: &'a str,
}

fn text_run(
    painter: &egui::Painter,
    run: &mut String,
    start: usize,
    y: f32,
    opts: &Paint<'_>,
    color: Color32,
) {
    let text = run.trim_end_matches(' ');
    if !text.is_empty() {
        painter.text(
            Pos2::new(opts.origin.x + start as f32 * opts.cell.x, y),
            egui::Align2::LEFT_TOP,
            text,
            opts.font.clone(),
            color,
        );
    }
    run.clear();
}

pub fn paint(ui: &egui::Ui, session: &Session, opts: Paint<'_>) {
    let viewport = Rect::from_min_size(
        opts.origin,
        Vec2::new(
            session.cols as f32 * opts.cell.x,
            session.rows as f32 * opts.cell.y,
        ),
    );
    let painter = ui.painter_at(viewport);
    let theme = theme();
    let content = session.term.renderable_content();
    let offset = content.display_offset as i32;
    let cursor = content.cursor.point;
    // Include the preceding history row as it slides into the viewport.
    let first_line = (-offset - 1).max(-(session.term.history_size() as i32));
    let mut cells = session
        .term
        .grid()
        .iter_from(Point::new(Line(first_line - 1), Column(session.cols - 1)))
        .take_while(|c| c.point.line.0 + offset < session.rows as i32)
        .peekable();
    let mut row = Vec::with_capacity(session.cols);
    let mut run = String::with_capacity(session.cols);
    while let Some(first) = cells.next() {
        row.clear();
        let line = first.point.line;
        row.push((first.point, first.cell));
        while cells.peek().is_some_and(|c| c.point.line == line) {
            let c = cells.next().unwrap();
            row.push((c.point, c.cell));
        }
        let screen_row = line.0 + offset;
        if screen_row < -1 || screen_row >= session.rows as i32 {
            continue;
        }
        let y = opts.origin.y + screen_row as f32 * opts.cell.y + session.scroll_pixels;
        if y + opts.cell.y <= viewport.top() || y >= viewport.bottom() {
            continue;
        }
        let text: String = row.iter().map(|(_, c)| c.c).collect();
        let mut marks = line_highlights(&text, opts.rules);
        if !opts.search.is_empty() {
            let lower = text.to_lowercase();
            let query = opts.search.to_lowercase();
            for (start, _) in lower.match_indices(&query) {
                marks.push((
                    start,
                    start + query.len().saturating_sub(1),
                    Color32::from_rgb(229, 192, 123),
                ));
            }
        }
        let mut foreground = Color32::TRANSPARENT;
        let mut run_start = 0;
        for (point, cell) in &row {
            let col = point.column.0;
            if col >= session.cols {
                break;
            }
            let selected = opts.selection.is_some_and(|(a, b)| {
                (point.line.0, col) >= (a.line.0, a.column.0)
                    && (point.line.0, col) <= (b.line.0, b.column.0)
            });
            let hovered = opts.hover.is_some_and(|(a, b)| {
                (point.line.0, col) >= (a.line.0, a.column.0)
                    && (point.line.0, col) <= (b.line.0, b.column.0)
            });
            let background = if selected {
                Color32::from_rgb(0x2F, 0x3E, 0x52)
            } else if hovered {
                let a = theme.accent;
                Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), 46)
            } else {
                opts.colors.resolve(cell.bg, true)
            };
            if background != opts.colors.background {
                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(opts.origin.x + col as f32 * opts.cell.x, y),
                        opts.cell,
                    ),
                    0.0,
                    background,
                );
            }
            let mut color = opts.colors.resolve(cell.fg, false);
            for (start, end, highlight) in marks.iter().rev() {
                if col >= *start && col <= *end {
                    color = *highlight;
                    break;
                }
            }
            color = opts.colors.dim(color);
            let wide = cell.flags.intersects(
                Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER,
            );
            if color != foreground || wide {
                text_run(&painter, &mut run, run_start, y, &opts, foreground);
                run_start = col;
                foreground = color;
            }
            if wide {
                if cell.flags.contains(Flags::WIDE_CHAR) {
                    painter.text(
                        Pos2::new(opts.origin.x + col as f32 * opts.cell.x, y),
                        egui::Align2::LEFT_TOP,
                        cell.c,
                        opts.font.clone(),
                        color,
                    );
                }
                run_start = col + 1;
            } else {
                run.push(if cell.c == '\0' { ' ' } else { cell.c });
            }
        }
        text_run(&painter, &mut run, run_start, y, &opts, foreground);
    }
    if offset == 0 && session.scroll_pixels == 0.0 {
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(
                    opts.origin.x + cursor.column.0 as f32 * opts.cell.x,
                    opts.origin.y + cursor.line.0 as f32 * opts.cell.y,
                ),
                opts.cell,
            ),
            0.0,
            {
                let c = theme.accent;
                Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 120)
            },
        );
    }
}
