//! What feeds a message before it is sent: staged attachments, saved drafts,
//! and the floating quick-chat window that is a composer with no thread yet.

pub(crate) mod attachments;
pub(crate) mod drafts;
pub(crate) mod quick;
