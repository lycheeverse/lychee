use crate::{Status, Uri};
use std::time::Duration;

#[cfg(feature = "email-check")]
use mailify_lib::{Client, Config};

/// A utility for checking the validity of email addresses.
///
/// `EmailChecker` is responsible for validating email addresses,
/// optionally performing reachability checks when the appropriate
/// features are enabled.
#[derive(Debug, Clone)]
pub(crate) struct MailChecker {
    #[cfg(feature = "email-check")]
    client: Client,
}

#[cfg(not(feature = "email-check"))]
impl MailChecker {
    /// Creates a new `EmailChecker`.
    pub(crate) const fn new(_timeout: Option<Duration>) -> Self {
        Self {}
    }

    /// Ignore the mail check if the `email-check` feature is not enabled.
    #[allow(
        clippy::unused_async,
        reason = "Match the signature of the function with the email-check feature"
    )]
    pub(crate) async fn check_mail(&self, _uri: &Uri) -> Status {
        Status::Excluded
    }
}

#[cfg(feature = "email-check")]
impl MailChecker {
    /// Creates a new `EmailChecker`.
    pub(crate) fn new(timeout: Option<Duration>) -> Self {
        Self {
            client: Client::new(Config {
                timeout,
                ..Default::default()
            }),
        }
    }

    /// Check a mail address, or equivalently a `mailto` URI.
    ///
    /// URIs may contain query parameters (e.g. `contact@example.com?subject="Hello"`),
    /// which are ignored by this check. They are not part of the mail address
    /// and instead passed to a mail client.
    pub(crate) async fn check_mail(&self, uri: &Uri) -> Status {
        self.perform_email_check(uri).await
    }

    async fn perform_email_check(&self, uri: &Uri) -> Status {
        use crate::ErrorKind;
        use http::StatusCode;
        use mailify_lib::CheckResult;

        let address = uri.url.path().to_string();
        let result = self.client.check(&address).await;

        match result {
            CheckResult::Success => Status::Ok(StatusCode::OK),
            CheckResult::Uncertain(reason) => Status::UnknownMailStatus(reason.to_string()),
            CheckResult::Failure(reason) => {
                ErrorKind::UnreachableEmailAddress(uri.clone(), reason.to_string()).into()
            }
        }
    }
}
