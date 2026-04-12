use crate::{FileType, Status};
use url::Url;

pub(crate) async fn check_text_fragments(
    _url: &Url,
    status: Status,
    _content: &str,
    _file_type: FileType,
) -> Status {
    status
}
