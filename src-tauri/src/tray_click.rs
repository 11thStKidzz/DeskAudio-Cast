//! Interpret Windows tray gestures once, without interfering with right-click menus.
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};

pub const SHOW_MENU_ON_LEFT_CLICK: bool = false;

#[derive(Default)]
pub struct TrayClickPolicy {
    suppress_double_click_release: bool,
}

impl TrayClickPolicy {
    pub fn should_toggle(&mut self, event: &TrayIconEvent) -> bool {
        match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Down,
                ..
            } => {
                // A new ordinary gesture also clears a missing double-click
                // release (e.g. the pointer moved outside the tray icon).
                self.suppress_double_click_release = false;
                false
            }
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } => !std::mem::take(&mut self.suppress_double_click_release),
            TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => {
                // Windows sends Down, Up, DoubleClick, Up. The first Up has
                // already toggled; do not toggle again on the final release.
                self.suppress_double_click_release = true;
                false
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn click(button: MouseButton, button_state: MouseButtonState) -> TrayIconEvent {
        TrayIconEvent::Click {
            id: "test".into(),
            position: tauri::PhysicalPosition::new(0.0, 0.0),
            rect: rect(),
            button,
            button_state,
        }
    }
    fn rect() -> tauri::Rect {
        tauri::Rect {
            position: tauri::PhysicalPosition::new(0, 0).into(),
            size: tauri::PhysicalSize::new(16, 16).into(),
        }
    }
    fn double_click() -> TrayIconEvent {
        TrayIconEvent::DoubleClick {
            id: "test".into(),
            position: tauri::PhysicalPosition::new(0.0, 0.0),
            rect: rect(),
            button: MouseButton::Left,
        }
    }
    #[test]
    fn single_left_gesture_toggles_exactly_once() {
        let mut policy = TrayClickPolicy::default();
        assert!(!policy.should_toggle(&click(MouseButton::Left, MouseButtonState::Down)));
        assert!(policy.should_toggle(&click(MouseButton::Left, MouseButtonState::Up)));
        assert!(!SHOW_MENU_ON_LEFT_CLICK);
    }
    #[test]
    fn right_and_middle_buttons_never_toggle_the_window() {
        let mut policy = TrayClickPolicy::default();
        for button in [MouseButton::Right, MouseButton::Middle] {
            for state in [MouseButtonState::Down, MouseButtonState::Up] {
                assert!(!policy.should_toggle(&click(button, state)));
            }
        }
    }
    #[test]
    fn windows_double_click_sequence_toggles_only_once() {
        let mut policy = TrayClickPolicy::default();
        let events = [
            click(MouseButton::Left, MouseButtonState::Down),
            click(MouseButton::Left, MouseButtonState::Up),
            double_click(),
            click(MouseButton::Left, MouseButtonState::Up),
        ];
        assert_eq!(
            events
                .iter()
                .filter(|event| policy.should_toggle(event))
                .count(),
            1
        );
    }
    #[test]
    fn missing_double_click_release_does_not_swallow_next_gesture() {
        let mut policy = TrayClickPolicy::default();
        assert!(!policy.should_toggle(&double_click()));
        assert!(!policy.should_toggle(&click(MouseButton::Left, MouseButtonState::Down)));
        assert!(policy.should_toggle(&click(MouseButton::Left, MouseButtonState::Up)));
    }
    #[test]
    fn independent_clicks_still_toggle_show_then_hide() {
        let mut policy = TrayClickPolicy::default();
        for _ in 0..100 {
            assert!(!policy.should_toggle(&click(MouseButton::Left, MouseButtonState::Down)));
            assert!(policy.should_toggle(&click(MouseButton::Left, MouseButtonState::Up)));
        }
    }
    #[test]
    fn hover_does_not_toggle_the_window() {
        let mut policy = TrayClickPolicy::default();
        let event = TrayIconEvent::Move {
            id: "test".into(),
            position: tauri::PhysicalPosition::new(0.0, 0.0),
            rect: rect(),
        };
        assert!(!policy.should_toggle(&event));
    }
}
