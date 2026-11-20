//! Atajos globales de teclado y mouse.

use eframe::egui::{self, Key};

use crate::app::{App, Page};

impl App {
    /// Atajos globales: Esc / Alt+← / botón atrás del mouse, 1..9, Espacio, F1.
    pub(crate) fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.dialogs.confirm_kill.is_some() || ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let (esc, alt_left, mouse_back, space, f1, num) = ctx.input(|i| {
            let num = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9]
                .iter()
                .position(|k| i.key_pressed(*k) && !i.modifiers.any());
            (
                i.key_pressed(Key::Escape),
                i.modifiers.alt && i.key_pressed(Key::ArrowLeft),
                i.pointer.button_clicked(egui::PointerButton::Extra1),
                i.key_pressed(Key::Space),
                i.key_pressed(Key::F1),
                num,
            )
        });
        if let Some(n) = num {
            if let Some(p) = self.pages().get(n) {
                self.go(*p);
            }
        }
        if space {
            self.paused = !self.paused;
        }
        if f1 {
            self.dialogs.keys = !self.dialogs.keys;
        }
        if self.page != Page::Home && (esc || alt_left || mouse_back) {
            self.go(Page::Home);
        } else if self.page == Page::Home && esc {
            self.select(None);
        }
    }
}
