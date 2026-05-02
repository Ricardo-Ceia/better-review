use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub enum ReviewStatus {
    #[default]
    Unreviewed,
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DiffLineKind {
    Add,
    Remove,
    Context,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Hunk {
    pub header: String,
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub lines: Vec<DiffLine>,
    pub review_status: ReviewStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub enum FileStatus {
    Added,
    Deleted,
    Renamed,
    Copied,
    ModeChanged,
    #[default]
    Modified,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ReviewCounts {
    pub unreviewed: usize,
    pub accepted: usize,
    pub rejected: usize,
}

impl ReviewCounts {
    pub fn bump(&mut self, status: &ReviewStatus) {
        match status {
            ReviewStatus::Unreviewed => self.unreviewed += 1,
            ReviewStatus::Accepted => self.accepted += 1,
            ReviewStatus::Rejected => self.rejected += 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct FileDiff {
    pub old_path: String,
    pub new_path: String,
    pub status: FileStatus,
    pub is_binary: bool,
    pub hunks: Vec<Hunk>,
    pub review_status: ReviewStatus,
}

pub fn count_review_statuses(files: &[FileDiff]) -> ReviewCounts {
    let mut counts = ReviewCounts::default();
    for file in files {
        if file.hunks.is_empty() {
            counts.bump(&file.review_status);
        } else {
            for hunk in &file.hunks {
                counts.bump(&hunk.review_status);
            }
        }
    }
    counts
}

impl FileDiff {
    pub fn display_path(&self) -> &str {
        if !self.new_path.is_empty() {
            &self.new_path
        } else {
            &self.old_path
        }
    }

    pub fn display_label(&self) -> String {
        match self.status {
            FileStatus::Renamed if !self.old_path.is_empty() && !self.new_path.is_empty() => {
                format!("{} → {}", self.old_path, self.new_path)
            }
            FileStatus::Copied if !self.old_path.is_empty() && !self.new_path.is_empty() => {
                format!("{} ⧉ {}", self.old_path, self.new_path)
            }
            FileStatus::ModeChanged => format!("{} mode changed", self.display_path()),
            _ => self.display_path().to_string(),
        }
    }

    pub fn set_all_hunks_status(&mut self, status: ReviewStatus) {
        for hunk in &mut self.hunks {
            hunk.review_status = status.clone();
        }
        self.review_status = status;
    }

    pub fn sync_review_status(&mut self) {
        if self.hunks.is_empty() {
            return;
        }

        let all_accepted = self
            .hunks
            .iter()
            .all(|hunk| hunk.review_status == ReviewStatus::Accepted);
        let all_rejected = self
            .hunks
            .iter()
            .all(|hunk| hunk.review_status == ReviewStatus::Rejected);

        self.review_status = if all_accepted {
            ReviewStatus::Accepted
        } else if all_rejected {
            ReviewStatus::Rejected
        } else {
            ReviewStatus::Unreviewed
        };
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DiffLine, DiffLineKind, FileDiff, FileStatus, Hunk, ReviewCounts, ReviewStatus,
        count_review_statuses,
    };

    #[test]
    fn display_path_falls_back_to_old_path_for_deleted_file() {
        let file = FileDiff {
            old_path: "removed.txt".to_string(),
            new_path: String::new(),
            ..FileDiff::default()
        };

        assert_eq!(file.display_path(), "removed.txt");
    }

    #[test]
    fn display_label_describes_path_changing_files() {
        let renamed = FileDiff {
            old_path: "src/old.rs".to_string(),
            new_path: "src/new.rs".to_string(),
            status: FileStatus::Renamed,
            ..FileDiff::default()
        };
        assert_eq!(renamed.display_label(), "src/old.rs → src/new.rs");

        let copied = FileDiff {
            old_path: "src/source.rs".to_string(),
            new_path: "src/copied.rs".to_string(),
            status: FileStatus::Copied,
            ..FileDiff::default()
        };
        assert_eq!(copied.display_label(), "src/source.rs ⧉ src/copied.rs");

        let mode_changed = FileDiff {
            new_path: "script.sh".to_string(),
            status: FileStatus::ModeChanged,
            ..FileDiff::default()
        };
        assert_eq!(mode_changed.display_label(), "script.sh mode changed");
    }

    #[test]
    fn count_review_statuses_aggregates_files_and_hunks() {
        let files = vec![
            FileDiff {
                status: FileStatus::ModeChanged,
                review_status: ReviewStatus::Accepted,
                ..FileDiff::default()
            },
            FileDiff {
                hunks: vec![
                    Hunk {
                        review_status: ReviewStatus::Unreviewed,
                        lines: vec![DiffLine {
                            kind: DiffLineKind::Add,
                            content: "new".to_string(),
                            old_line: None,
                            new_line: Some(1),
                        }],
                        ..Hunk::default()
                    },
                    Hunk {
                        review_status: ReviewStatus::Rejected,
                        ..Hunk::default()
                    },
                ],
                ..FileDiff::default()
            },
        ];

        assert_eq!(
            count_review_statuses(&files),
            ReviewCounts {
                unreviewed: 1,
                accepted: 1,
                rejected: 1,
            }
        );
    }

    #[test]
    fn sync_review_status_keeps_state_when_no_hunks() {
        let mut file = FileDiff {
            review_status: ReviewStatus::Accepted,
            ..FileDiff::default()
        };

        file.sync_review_status();
        assert_eq!(file.review_status, ReviewStatus::Accepted);
    }
}
