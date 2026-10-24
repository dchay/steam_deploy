use crate::state::State;

pub fn status_command() -> Result<(), String> {
    let state =
        State::load(".officiator/active/state.md")?;

    state.validate()?;

    println!("Ticket: {}", state.ticket_id);

    println!(
        "State: {}",
        state.state.as_str()
    );

    println!(
        "Attempt: {}",
        state.attempt
    );

    println!(
        "Last Transition: {}",
        state.last_transition
    );

    Ok(())
}