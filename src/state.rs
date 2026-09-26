#[derive(Debug, Default)]
pub struct AppState {
    pub status_message: String,
    pub action_count: usize,
}

impl AppState {
    pub fn increment_action(&mut self, msg: impl Into<String>) {
        self.action_count += 1;
        self.status_message = msg.into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_increment_action() {
        let mut state = AppState::default();
        assert_eq!(state.action_count, 0);
        state.increment_action("Test action");
        assert_eq!(state.action_count, 1);
        assert_eq!(state.status_message, "Test action");
    }
}
