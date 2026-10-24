use std::fs;

fn field_value<'a>(
    line: &'a str,
    prefix: &str,
) -> Option<&'a str> {
    line.strip_prefix(prefix)
        .map(|v| v.trim())
}

impl State {
    pub fn new(
        ticket_id: String,
        state: WorkflowState,
        attempt: u32,
        last_transition: String,
    ) -> Self {
        Self {
            ticket_id,
            state,
            attempt,
            last_transition,
        }
    }

    pub fn load(path: &str) -> Result<Self, String> {
        let contents =
            fs::read_to_string(path)
                .map_err(|e| e.to_string())?;

        assert!(!contents.is_empty());

        let mut ticket_id = String::new();

        let mut workflow_state = WorkflowState::Ready;

        let mut attempt = 1;
        let mut last_transition = String::new();

        for line in contents.lines() {
            if let Some(value) =
                field_value(line, "ticket_id:")
            {
                ticket_id = value.to_string();
            }

            if let Some(value) = line.strip_prefix("state:") {
                workflow_state = match value.trim() {
                    "ready" => WorkflowState::Ready,
                    "implementing" => WorkflowState::Implementing,
                    "reviewing" => WorkflowState::Reviewing,
                    "approved" => WorkflowState::Approved,
                    "closed" => WorkflowState::Closed,
                    _ => WorkflowState::Ready,
                };
            }

            if let Some(value) = line.strip_prefix("attempt:") {
                attempt = value
                    .trim()
                    .parse::<u32>()
                    .unwrap_or(1);
            }
            if let Some(value) = line.strip_prefix("last_transition:") {
                last_transition = value.trim().to_string();
            }
        }

        let state = Self::new(
            ticket_id,
            workflow_state,
            attempt,
            last_transition,
        );

        Ok(state)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.ticket_id.trim().is_empty() {
            return Err("missing ticket_id".to_string());
        }

        if self.attempt < 1 {
            return Err("attempt must be >= 1".to_string());
        }

        if self.last_transition.trim().is_empty() {
            return Err("missing last_transition".to_string());
        }

        if !self.last_transition.contains('T') {
            return Err(
                "last_transition must contain T".to_string()
            );
        }

        Ok(())
    }
}

pub fn can_transition(
    from: &WorkflowState,
    to: &WorkflowState,
) -> bool {
    matches!(
        (from, to),
        (WorkflowState::Ready, WorkflowState::Implementing)
            | (WorkflowState::Implementing, WorkflowState::Reviewing)
            | (WorkflowState::Reviewing, WorkflowState::Approved)
            | (WorkflowState::Reviewing, WorkflowState::Implementing)
            | (WorkflowState::Approved, WorkflowState::Implementing)
            | (WorkflowState::Approved, WorkflowState::Closed)
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowState {
    Ready,
    Implementing,
    Reviewing,
    Approved,
    Closed,
}

impl WorkflowState {
    pub fn from_str(value: &str) -> Self {
        match value {
            "ready" => WorkflowState::Ready,
            "implementing" => WorkflowState::Implementing,
            "reviewing" => WorkflowState::Reviewing,
            "approved" => WorkflowState::Approved,
            "closed" => WorkflowState::Closed,
            _ => WorkflowState::Ready,
        }
    }
}

#[derive(Debug, Clone)]
pub struct State {
    pub ticket_id: String,
    pub state: WorkflowState,
    pub attempt: u32,
    pub last_transition: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempFile {
        path: PathBuf,
    }

    impl TempFile {
        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    fn temp_state_file(contents: &str) -> TempFile {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let path = std::env::temp_dir()
            .join(format!("state_test_{unique}.md"));

        fs::write(&path, contents).unwrap();

        TempFile { path }
    }

    fn state_fixture(state: &str) -> String {
        format!(
            "\
ticket_id: P0-002
state: {state}
attempt: 1
last_transition: 2099-12-31T23:59:59Z
"
        )
    }

    #[test]
    fn ready_can_transition_to_implementing() {
        assert!(can_transition(
            &WorkflowState::Ready,
            &WorkflowState::Implementing,
        ));
    }

    #[test]
    fn ready_cannot_transition_to_approved() {
        assert!(!can_transition(
            &WorkflowState::Ready,
            &WorkflowState::Approved,
        ));
    }

    #[test]
    fn implementing_can_transition_to_reviewing() {
        assert!(can_transition(
            &WorkflowState::Implementing,
            &WorkflowState::Reviewing,
        ));
    }

    #[test]
    fn reviewing_can_transition_to_approved() {
        assert!(can_transition(
            &WorkflowState::Reviewing,
            &WorkflowState::Approved,
        ));
    }

    #[test]
    fn reviewing_can_transition_to_implementing() {
        assert!(can_transition(
            &WorkflowState::Reviewing,
            &WorkflowState::Implementing,
        ));
    }

    #[test]
    fn approved_can_transition_to_implementing() {
        assert!(can_transition(
            &WorkflowState::Approved,
            &WorkflowState::Implementing,
        ));
    }

    #[test]
    fn approved_can_transition_to_closed() {
        assert!(can_transition(
            &WorkflowState::Approved,
            &WorkflowState::Closed,
        ));
    }

    #[test]
    fn load_returns_state() {
        let state = State::load(".officiator/active/state.md")
            .expect("state should load");

        state.validate()
            .expect("state should validate");

        assert_eq!(state.ticket_id, "P0-002");
        assert!(!state.ticket_id.is_empty());

        assert_eq!(
            state.state,
            WorkflowState::Implementing
        );

        assert_eq!(state.attempt, 7);

        assert_eq!(
            state.last_transition,
            "2099-12-31T23:59:59Z"
        );
    }

    #[test]
    fn validation_fails_for_zero_attempt() {
        let state = State::new(
            "P0-002".to_string(),
            WorkflowState::Implementing,
            0,
            "2099-12-31T23:59:59Z".to_string(),
        );

        assert!(state.validate().is_err());
    }

    #[test]
    fn validation_fails_for_empty_ticket_id() {
        let state = State::new(
            "".to_string(),
            WorkflowState::Implementing,
            1,
            "2099-12-31T23:59:59Z".to_string(),
        );

        assert!(state.validate().is_err());
    }

    #[test]
    fn validation_fails_for_missing_last_transition() {
        let state = State::new(
            "P0-002".to_string(),
            WorkflowState::Implementing,
            1,
            "".to_string(),
        );

        assert!(state.validate().is_err());
    }

    #[test]
    fn validation_fails_for_invalid_last_transition() {
        let state = State::new(
            "P0-002".to_string(),
            WorkflowState::Implementing,
            1,
            "invalid".to_string(),
        );

        assert!(state.validate().is_err());
    }

    #[test]
    fn load_parses_workflow_states() {
        let cases = [
            ("approved", WorkflowState::Approved),
            ("reviewing", WorkflowState::Reviewing),
            ("closed", WorkflowState::Closed),
        ];

        for (state_text, expected_state) in cases {
            let file =
                temp_state_file(&state_fixture(state_text));

            let state = State::load(
                file.path().to_str().unwrap(),
            )
                .expect("state should load");

            assert_eq!(state.state, expected_state);
        }
    }

    #[test]
    fn load_defaults_unknown_state_to_ready() {
        let file = temp_state_file(
            &state_fixture("totally_unknown"),
        );

        let state = State::load(
            file.path().to_str().unwrap(),
        )
            .expect("state should load");

        assert_eq!(
            state.state,
            WorkflowState::Ready
        );
    }
}