//! Files the reviewer marked viewed, per profile, keyed by each file's blob
//! id at the head: a file that changes after it was marked reads as unviewed
//! again, and the others keep their mark.

use std::collections::HashMap;

use super::{ReviewDesk, PROVIDER};
use crate::review::types::FileBlob;
use crate::storage;

impl ReviewDesk {
    /// Which of `files` (the PR's changed files at its current head) are
    /// still viewed: marked, and unchanged since.
    pub(crate) async fn viewed_files(
        &self,
        number: i64,
        files: Vec<FileBlob>,
    ) -> Result<Vec<String>, String> {
        let slug = self.slug().await?;
        let marks: HashMap<String, String> =
            storage::read_viewed_files(&self.database, PROVIDER, &slug, number)
                .await?
                .into_iter()
                .collect();
        Ok(files
            .into_iter()
            .filter(|file| marks.get(&file.path) == Some(&file.blob_sha))
            .map(|file| file.path)
            .collect())
    }

    /// Mark `files` viewed at their blob ids, or clear their marks.
    pub(crate) async fn set_viewed(
        &self,
        number: i64,
        files: Vec<FileBlob>,
        viewed: bool,
    ) -> Result<(), String> {
        let slug = self.slug().await?;
        if viewed {
            let marks: Vec<(String, String)> = files
                .into_iter()
                .map(|file| (file.path, file.blob_sha))
                .collect();
            storage::mark_viewed_files(&self.database, PROVIDER, &slug, number, &marks).await
        } else {
            let paths: Vec<String> = files.into_iter().map(|file| file.path).collect();
            storage::unmark_viewed_files(&self.database, PROVIDER, &slug, number, &paths).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::fixture;
    use crate::review::types::FileBlob;

    fn blob(path: &str, sha: &str) -> FileBlob {
        FileBlob {
            path: path.into(),
            blob_sha: sha.into(),
        }
    }

    #[tokio::test]
    async fn a_file_that_changed_since_it_was_marked_viewed_reads_as_unviewed() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let first = vec![blob("a.rs", "a1"), blob("b.rs", "b1"), blob("c.rs", "c1")];
        fixture
            .desk
            .set_viewed(5, first.clone(), true)
            .await
            .unwrap();
        fixture
            .desk
            .set_viewed(5, vec![blob("c.rs", "c1")], false)
            .await
            .unwrap();
        assert_eq!(
            fixture.desk.viewed_files(5, first).await.unwrap(),
            ["a.rs", "b.rs"]
        );

        // The author pushes: a.rs changes, b.rs does not.
        let pushed = vec![blob("a.rs", "a2"), blob("b.rs", "b1"), blob("c.rs", "c1")];
        assert_eq!(
            fixture.desk.viewed_files(5, pushed).await.unwrap(),
            ["b.rs"]
        );
        // Another PR keeps its own marks.
        assert!(fixture
            .desk
            .viewed_files(6, vec![blob("b.rs", "b1")])
            .await
            .unwrap()
            .is_empty());
    }
}
