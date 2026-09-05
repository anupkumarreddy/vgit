//! Single-line editing with native platform input and UTF-16 IME ranges.
use crate::{Field, Workspace};
use gpui::{prelude::*, *};
use std::cell::RefCell;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Default)]
pub struct Input {
    pub text: String,
    anchor: usize,
    caret: usize,
    marked: Option<Range<usize>>,
    layout: RefCell<Option<(ShapedLine, Bounds<Pixels>, Pixels)>>,
}

impl Input {
    fn selection(&self) -> Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret)
    }
    fn byte(&self, utf16: usize) -> usize {
        let mut count = 0;
        for (byte, ch) in self.text.char_indices() {
            if count + ch.len_utf16() > utf16 {
                return byte;
            }
            count += ch.len_utf16();
        }
        self.text.len()
    }
    fn utf16(&self, byte: usize) -> usize {
        self.text[..byte].encode_utf16().count()
    }
    fn bytes(&self, range: Range<usize>) -> Range<usize> {
        self.byte(range.start)..self.byte(range.end.max(range.start))
    }
    fn replace(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        marked: bool,
        selection: Option<Range<usize>>,
    ) {
        let range = range
            .map(|r| self.bytes(r))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection());
        let text = text.replace(['\r', '\n'], " ");
        self.text.replace_range(range.clone(), &text);
        self.caret = range.start + text.len();
        self.anchor = self.caret;
        self.marked = if marked {
            Some(range.start..self.caret)
        } else {
            None
        };
        if marked && let Some(selection) = selection {
            // IME selection offsets are relative to the replacement, not the document.
            let replacement = Input {
                text,
                ..Default::default()
            };
            self.anchor = range.start + replacement.byte(selection.start);
            self.caret = range.start + replacement.byte(selection.end);
        }
    }
    fn move_caret(&mut self, to: usize, extend: bool) {
        self.caret = to;
        if !extend {
            self.anchor = to;
        }
        self.marked = None;
    }
    fn previous(&self) -> usize {
        self.text[..self.caret]
            .grapheme_indices(true)
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }
    fn next(&self) -> usize {
        self.text[self.caret..]
            .graphemes(true)
            .next()
            .map(|s| self.caret + s.len())
            .unwrap_or(self.text.len())
    }
    pub fn key(&mut self, event: &KeyDownEvent, cx: &mut App) -> bool {
        let key = &event.keystroke;
        let command = key.modifiers.platform || key.modifiers.control;
        match key.key.as_str() {
            "a" if command => {
                self.anchor = 0;
                self.caret = self.text.len();
            }
            "c" | "x" if command => {
                let range = self.selection();
                if !range.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(self.text[range].to_string()));
                    if key.key == "x" {
                        self.replace(None, "", false, None);
                    }
                }
            }
            "v" if command => {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    self.replace(None, &text, false, None);
                }
            }
            "left" | "right" | "home" | "end" => {
                let range = self.selection();
                let to = match key.key.as_str() {
                    "home" => 0,
                    "end" => self.text.len(),
                    "left" if command => 0,
                    "right" if command => self.text.len(),
                    "left" if !key.modifiers.shift && !range.is_empty() => range.start,
                    "right" if !key.modifiers.shift && !range.is_empty() => range.end,
                    "left" => self.previous(),
                    _ => self.next(),
                };
                self.move_caret(to, key.modifiers.shift);
            }
            "backspace" | "delete" => {
                if self.selection().is_empty() {
                    self.anchor = if key.key == "backspace" {
                        self.previous()
                    } else {
                        self.next()
                    };
                }
                self.replace(None, "", false, None);
            }
            "enter" | "up" | "down" => {}
            _ => return false, // Characters and composition go through the platform handler.
        }
        true
    }
}

pub fn field(field: Field, cx: &mut Context<Workspace>) -> impl IntoElement {
    let entity = cx.entity();
    div()
        .w_full()
        .h(px(22.))
        .overflow_hidden()
        .cursor_text()
        .child(
            canvas(
                move |bounds, window, cx| {
                    let workspace = entity.read(cx);
                    let colors = workspace.colors();
                    let value = workspace.inputs.get(&field);
                    let text = value.map(|v| v.text.as_str()).unwrap_or("");
                    let focused = workspace.composing == Some(field);
                    let display: SharedString = if text.is_empty() {
                        field.placeholder().into()
                    } else {
                        text.to_string().into()
                    };
                    let style = window.text_style();
                    let run = TextRun {
                        len: display.len(),
                        font: style.font(),
                        color: rgb(if text.is_empty() {
                            colors.dim
                        } else {
                            colors.text
                        })
                        .into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let line = window
                        .text_system()
                        .shape_line(display, px(13.), &[run], None);
                    let caret = value.map(|v| v.caret).unwrap_or(0);
                    let scroll = (line.x_for_index(caret) - bounds.size.width + px(3.)).max(px(0.));
                    (entity.clone(), line, scroll, focused, colors)
                },
                move |bounds, (entity, line, scroll, focused, colors), window, cx| {
                    if focused {
                        window.handle_input(
                            &entity.read(cx).focus.clone(),
                            ElementInputHandler::new(bounds, entity.clone()),
                            cx,
                        );
                        let value = entity.read(cx).inputs.get(&field);
                        let selected = value.map(|v| v.selection()).unwrap_or(0..0);
                        let left = bounds.left() + line.x_for_index(selected.start) - scroll;
                        let right = bounds.left() + line.x_for_index(selected.end) - scroll;
                        window.paint_quad(fill(
                            Bounds::from_corners(
                                point(left, bounds.top()),
                                point(
                                    if selected.is_empty() {
                                        left + px(1.)
                                    } else {
                                        right
                                    },
                                    bounds.bottom(),
                                ),
                            ),
                            rgb(if selected.is_empty() {
                                colors.text
                            } else {
                                colors.selection
                            }),
                        ));
                    }
                    let _ = line.paint(bounds.origin - point(scroll, px(0.)), px(22.), window, cx);
                    // Paint geometry is not model state. Updating the workspace
                    // entity during paint invalidates GPUI's cached event tree.
                    if let Some(value) = entity.read(cx).inputs.get(&field) {
                        value.layout.replace(Some((line, bounds, scroll)));
                    }
                },
            )
            .size_full(),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |workspace, event: &MouseDownEvent, window, cx| {
                workspace.composing = Some(field);
                workspace.focus.focus(window);
                let value = workspace.inputs.entry(field).or_default();
                let index = value
                    .layout
                    .borrow()
                    .as_ref()
                    .map(|(line, bounds, scroll)| {
                        line.closest_index_for_x(event.position.x - bounds.left() + *scroll)
                            .min(value.text.len())
                    });
                if let Some(index) = index {
                    value.move_caret(index, event.modifiers.shift);
                }
                cx.notify();
            }),
        )
}

impl EntityInputHandler for Workspace {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let value = self.inputs.entry(self.composing?).or_default();
        let range = value.bytes(range);
        *adjusted = Some(value.utf16(range.start)..value.utf16(range.end));
        Some(value.text[range].to_string())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let value = self.inputs.entry(self.composing?).or_default();
        let range = value.selection();
        Some(UTF16Selection {
            range: value.utf16(range.start)..value.utf16(range.end),
            reversed: value.caret < value.anchor,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let value = self.inputs.get(&self.composing?)?;
        value
            .marked
            .as_ref()
            .map(|r| value.utf16(r.start)..value.utf16(r.end))
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(field) = self.composing {
            self.inputs.entry(field).or_default().marked = None;
            cx.notify();
        }
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(field) = self.composing {
            self.inputs
                .entry(field)
                .or_default()
                .replace(range, text, false, None);
            cx.notify();
        }
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(field) = self.composing {
            self.inputs
                .entry(field)
                .or_default()
                .replace(range, text, true, selection);
            cx.notify();
        }
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let value = self.inputs.get(&self.composing?)?;
        let layout = value.layout.borrow();
        let (line, bounds, scroll) = layout.as_ref()?;
        let range = value.bytes(range);
        Some(Bounds::from_corners(
            point(
                bounds.left() + line.x_for_index(range.start) - *scroll,
                bounds.top(),
            ),
            point(
                bounds.left() + line.x_for_index(range.end) - *scroll,
                bounds.bottom(),
            ),
        ))
    }
    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let value = self.inputs.get(&self.composing?)?;
        let layout = value.layout.borrow();
        let (line, bounds, scroll) = layout.as_ref()?;
        Some(
            value.utf16(
                line.closest_index_for_x(point.x - bounds.left() + *scroll)
                    .min(value.text.len()),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Input;
    #[test]
    fn composition_replaces_marked_text_and_uses_relative_selection() {
        let mut value = Input::default();
        value.replace(None, "prefix 😀 ", false, None);
        value.replace(None, "日本", true, Some(1..2));
        assert_eq!(&value.text[value.selection()], "本");
        value.replace(None, "日本語", false, None);
        assert_eq!(value.text, "prefix 😀 日本語");
        assert!(value.marked.is_none());
    }
    #[test]
    fn utf16_ranges_do_not_split_emoji() {
        let mut value = Input::default();
        value.replace(None, "a😀b", false, None);
        value.replace(Some(1..3), "x", false, None);
        assert_eq!(value.text, "axb");
    }
    #[test]
    fn deletion_boundaries_cover_combining_characters() {
        let mut value = Input::default();
        value.replace(None, "ae\u{301}", false, None);
        assert_eq!(value.previous(), 1);
        value.anchor = value.previous();
        value.replace(None, "", false, None);
        assert_eq!(value.text, "a");
    }
    #[test]
    fn paste_replaces_selection_and_flattens_newlines() {
        let mut value = Input::default();
        value.replace(None, "old", false, None);
        value.anchor = 0;
        value.replace(None, "new\nmessage", false, None);
        assert_eq!(value.text, "new message");
    }
}
