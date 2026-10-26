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

#[cfg(test)]
mod tests {
    use super::*;

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
