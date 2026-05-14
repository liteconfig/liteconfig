//! Integration-level sanity for the argv builder. The actual subprocess
//! paths (`pnpx skills add ...`, curl pipe installer) need hermetic mocks
//! that aren't worth the complexity; we verify here that given a known
//! `InstallMethod` the argv we'd hand to `Command::new` is exactly right.

use liteconfig_core::services::skill_cli_service::{pnpx_argv, InstallMethod};

#[test]
fn pnpx_argv_shape() {
    let (prog, args) = pnpx_argv(InstallMethod::Pnpm, "anthropic/cookbook").unwrap();
    assert_eq!(prog, "pnpx");
    assert_eq!(args, vec!["skills", "add", "anthropic/cookbook"]);
}

#[test]
fn npm_argv_shape() {
    let (prog, args) = pnpx_argv(InstallMethod::Npm, "anthropic/cookbook").unwrap();
    assert_eq!(prog, "npx");
    // -y auto-installs `skills` without prompting.
    assert_eq!(args[0], "-y");
    assert_eq!(args.last().unwrap(), "anthropic/cookbook");
}

#[test]
fn no_runtime_returns_none() {
    assert!(pnpx_argv(InstallMethod::None, "x/y").is_none());
    assert!(pnpx_argv(InstallMethod::NodeOnly, "x/y").is_none());
}
