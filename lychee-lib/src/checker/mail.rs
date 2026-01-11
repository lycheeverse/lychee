use std::time::Duration;

#[cfg(feature = "email-check")]
use http::StatusCode;
use mailify_lib::Config;

#[cfg(feature = "email-check")]
use crate::ErrorKind;

use crate::{Status, Uri};

#[cfg(feature = "email-check")]
use mailify_lib::Client;

/// A utility for checking the validity of email addresses.
///
/// `EmailChecker` is responsible for validating email addresses,
/// optionally performing reachability checks when the appropriate
/// features are enabled.
#[derive(Debug, Clone)]
pub(crate) struct MailChecker {
    client: Client,
}

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
    #[cfg(feature = "email-check")]
    pub(crate) async fn check_mail(&self, uri: &Uri) -> Status {
        self.perform_email_check(uri).await
    }

    /// Ignore the mail check if the `email-check` and `native-tls` features are not enabled.
    #[cfg(not(feature = "email-check"))]
    pub(crate) async fn check_mail(&self, _uri: &Uri) -> Status {
        Status::Excluded
    }

    #[cfg(feature = "email-check")]
    async fn perform_email_check(&self, uri: &Uri) -> Status {
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
