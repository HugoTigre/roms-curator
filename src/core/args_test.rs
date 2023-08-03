use crate::core::args::command;

#[test]
fn verify_command() {
    command().debug_assert();
}
