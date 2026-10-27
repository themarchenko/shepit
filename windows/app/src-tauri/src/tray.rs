use shepit_core::dictation::ModelStatus;

#[derive(Debug, PartialEq, Eq)]
pub enum MenuAction {
    Quit,
    None,
}

pub fn action_for(id: &str) -> MenuAction {
    match id {
        "quit" => MenuAction::Quit,
        _ => MenuAction::None,
    }
}

/// Tray tooltip for the model state: the download percent while it runs, else the app name.
pub fn tooltip_for(status: ModelStatus) -> String {
    match status {
        ModelStatus::Downloading(p) => format!("Downloading model {}%", (p * 100.0).round() as u32),
        _ => "Shepit".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_shows_the_download_percent() {
        assert_eq!(
            tooltip_for(ModelStatus::Downloading(0.42)),
            "Downloading model 42%"
        );
        assert_eq!(
            tooltip_for(ModelStatus::Downloading(0.0)),
            "Downloading model 0%"
        );
    }

    #[test]
    fn tooltip_is_the_name_otherwise() {
        for s in [
            ModelStatus::Loading,
            ModelStatus::Ready,
            ModelStatus::Failed,
        ] {
            assert_eq!(tooltip_for(s), "Shepit");
        }
    }

    #[test]
    fn quit_id_quits() {
        assert_eq!(action_for("quit"), MenuAction::Quit);
    }

    #[test]
    fn other_id_is_none() {
        assert_eq!(action_for("settings"), MenuAction::None);
    }

    #[test]
    fn empty_id_is_none() {
        assert_eq!(action_for(""), MenuAction::None);
    }

    #[test]
    fn ids_are_case_sensitive() {
        assert_eq!(action_for("QUIT"), MenuAction::None);
    }
}
