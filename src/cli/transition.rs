use crate::state::{
    State,
    TransitionService,
    WorkflowState,
};

pub fn transition_command(
    target: WorkflowState,
) -> Result<(), String> {
    let mut state =
        State::load(".officiator/active/state.md")?;

    TransitionService::apply_and_save(
        &mut state,
        target,
        ".officiator/active/state.md",
    )?;

    Ok(())
}