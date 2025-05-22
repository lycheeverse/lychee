#[cfg(all(feature = "email-check", feature = "native-tls"))]
use http::StatusCode;

#[cfg(all(feature = "email-check", feature = "native-tls"))]
use crate::ErrorKind;

use crate::{Status, Uri};

#[cfg(all(feature = "email-check", feature = "native-tls"))]
use check_if_email_exists::{CheckEmailInput, Reachable, check_email};

#[cfg(all(feature = "email-check", feature = "native-tls"))]
use crate::types::mail;

/// A utility for checking the validity of email addresses.
///
/// `EmailChecker` is responsible for validating email addresses,
/// optionally performing reachability checks when the appropriate
/// features are enabled.
#[derive(Debug, Clone)]
pub(crate) struct MailChecker {}

impl MailChecker {
    /// Creates a new `EmailChecker`.
    pub(crate) const fn new() -> Self {
        Self {}
    }

    /// Check a mail address, or equivalently a `mailto` URI.
    ///
    /// URIs may contain query parameters (e.g. `contact@example.com?subject="Hello"`),
    /// which are ignored by this check. They are not part of the mail address
    /// and instead passed to a mail client.
    #[cfg(all(feature = "email-check", feature = "native-tls"))]
    pub(crate) async fn check_mail(&self, uri: &Uri) -> Status {
        self.perform_email_check(uri).await
    }

    /// Ignore the mail check if the `email-check` and `native-tls` features are not enabled.
    #[cfg(not(all(feature = "email-check", feature = "native-tls")))]
    pub(crate) async fn check_mail(&self, _uri: &Uri) -> Status {
        Status::Excluded
    }

    #[cfg(all(feature = "email-check", feature = "native-tls"))]
    async fn perform_email_check(&self, uri: &Uri) -> Status {
        let address = uri.url.path().to_string();
        let input = CheckEmailInput::new(address);
        let result = &(check_email(&input).await);

        if let Reachable::Invalid = result.is_reachable {
            ErrorKind::UnreachableEmailAddress(uri.clone(), mail::error_from_output(result)).into()
        } else {
            Status::Ok(StatusCode::OK)
        }
    }
}
