//! CI-only PATH shim for the supervisor fixture's embedded `cargo build -j2`.
//! The production host and test sources remain unchanged.
use std::{
    env,
    process::{Command, ExitCode},
};

fn main() -> ExitCode {
    let cargo = env::var_os("TEZBAR_CI_REAL_CARGO").expect("absolute real Cargo path required");
    let args = env::args_os()
        .skip(1)
        .map(|arg| if arg == "-j2" { "-j1".into() } else { arg });
    let status = Command::new(cargo)
        .args(args)
        .status()
        .expect("run real Cargo");
    ExitCode::from(status.code().unwrap_or(1).try_into().unwrap_or(1))
}
