use std::fs;

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
            if let Some(value) = line.strip_prefix("ticket_id:") {
                ticket_id = value.trim().to_string();
            }

            if let Some(value) = line.strip_prefix("state:") {
                workflow_state = match value.trim() {
                    "ready" => WorkflowState::Ready,
                    "implementing" => WorkflowState::Implementing,
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

        assert_eq!(state.ticket_id, "P0-002");
        assert!(!state.ticket_id.is_empty());
        assert_eq!(state.attempt, 7);

        assert_eq!(
            state.state,
            WorkflowState::Implementing
        );

        assert_eq!(
            state.last_transition,
            "2026-10-23T16:00:00Z"
        );
    }
}