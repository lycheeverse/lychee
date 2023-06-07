#![cfg(all(feature = "email-check", feature = "native-tls"))]

use check_if_email_exists::{CheckEmailOutput, Reachable};

/// A crude way to extract error details from the mail output.
/// This was added because `CheckEmailOutput` doesn't impl `Display`.
pub(crate) fn error_from_output(o: &CheckEmailOutput) -> String {
    if let Err(_e) = o.misc.as_ref() {
        return "Error occurred connecting to this email server via SMTP".to_string();
    } else if let Err(e) = &o.smtp {
        return format!("{e:?}");
    } else if let Err(e) = &o.mx {
        return format!("{e:?}");
    }
    match &o.is_reachable {
        Reachable::Safe => "Safe: The email is safe to send",
        Reachable::Risky => "Risky: The email address appears to exist, but has quality issues that may result in low engagement or a bounce",
        Reachable::Invalid => "Invalid: Email doesn't exist or is syntactically incorrect",
        Reachable::Unknown => "Unknown: We're unable to get a valid response from the recipient's email server."
    }.to_string()
}
