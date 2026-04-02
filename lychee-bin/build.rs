use std::process::Command;

fn main() {
    set_git_date_variable();
}

/// Set GIT_DATE as environment variable for use in the main program.
/// The value will be displayed in the manual page.
fn set_git_date_variable() {
    println!("cargo:rustc-env=GIT_DATE={}", git_date())
}

/// Get the commit date of HEAD with git
fn git_date() -> String {
    let output = Command::new("git")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["show", "--no-patch", "--format=%cs", "HEAD"])
        .output()
        .expect("Error while trying to determine latest commit date");

    String::from_utf8(output.stdout).expect("Unable to read stdout to string")
}
