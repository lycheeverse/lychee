use std::{collections::HashSet, fmt::{self, Display}};

use crate::types::Response;
use crate::types::Uri;

pub struct Stats {
    total: usize,
    successful: usize,
    failed: HashSet<Uri>,
    timeout: HashSet<Uri>,
    redirected: HashSet<Uri>,
    excluded: HashSet<Uri>,
    error: HashSet<Uri>,
}

impl Stats {
    pub fn new() -> Self {
        Stats {
            total: 0,
            successful: 0,
            failed: HashSet::new(),
            timeout: HashSet::new(),
            redirected: HashSet::new(),
            excluded: HashSet::new(),
            error: HashSet::new(),
        }
    }

    pub fn add(&mut self, response: Response) {
        self.total += 1;
        let uri = response.uri;
        match response.status {
            crate::types::Status::Ok(_) => self.successful += 1,
            crate::types::Status::Failed(_) => {
                self.failed.insert(uri);
            }
            crate::types::Status::Timeout => {
                self.timeout.insert(uri);
            }
            crate::types::Status::Redirected => {
                self.redirected.insert(uri);
            }
            crate::types::Status::Excluded => {
                self.excluded.insert(uri);
            }
            crate::types::Status::Error(_) => {
                self.error.insert(uri);
            }
        };
    }

    pub fn is_success(&self) -> bool {
        [&self.failed, &self.timeout, &self.redirected, &self.error]
            .iter()
            .all(|r| r.is_empty())
    }
}

impl Display for Stats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "📝 Summary")?;
        write!(f, "-------------------")?;
        write!(f, "🔍 Total: {}", self.total)?;
        write!(f, "✅ Successful: {}", self.successful)?;
        write!(f, "⏳ Timeout: {}", self.timeout.len())?;
        write!(f, "🔀 Redirected: {}", self.redirected.len())?;
        write!(f, "👻 Excluded: {}", self.excluded.len())?;
        write!(f, "🚫 Errors: {}", self.error.len() + self.failed.len())?;
        Ok(())
    }
}
