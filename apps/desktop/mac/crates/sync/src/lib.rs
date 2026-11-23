//! Optional non-sensitive synchronization never gates local SSH.

mod account;
mod http;

pub use account::{AccountSession, AccountStatus, AccountTokenStore, AccountTokens};
pub use http::{AccountProfile, CloudClient, FeedbackReceipt, HttpSyncTransport, UpdateCheck};

use async_trait::async_trait;
use sparrowshell_domain::{SyncChange, SyncMutation, SyncPullPage, SyncPushResult};
use sparrowshell_storage::Repository;
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("local synchronization state could not be persisted")]
    Storage(#[from] sparrowshell_storage::StorageError),
    #[error("cloud synchronization unavailable: {0}")]
    Transport(&'static str),
    #[error("{0}")]
    Remote(String),
    #[error("cloud synchronization returned an invalid or incomplete response: {0}")]
    Protocol(&'static str),
}

#[async_trait]
pub trait SyncTransport: Send + Sync {
    async fn push(
        &self,
        device_id: Uuid,
        changes: Vec<SyncMutation>,
    ) -> Result<Vec<SyncPushResult>, SyncError>;
    async fn pull(&self, cursor: u64, limit: usize) -> Result<SyncPullPage, SyncError>;
    async fn snapshot(&self, cursor: u64, limit: usize) -> Result<SyncPullPage, SyncError>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub pushed: usize,
    pub conflicts: usize,
    pub pulled: usize,
    pub snapshot_restored: bool,
}

/// One bounded synchronization pass. Transport errors never remove local mutations.
pub struct SyncEngine<'a, T> {
    repository: &'a Repository,
    transport: &'a T,
}

impl<'a, T: SyncTransport> SyncEngine<'a, T> {
    pub fn new(repository: &'a Repository, transport: &'a T) -> Self {
        Self {
            repository,
            transport,
        }
    }

    pub async fn run_once(&self) -> Result<SyncReport, SyncError> {
        let account_id = self.repository.workspace_owner()?;
        self.run_once_scoped(account_id.as_deref()).await
    }

    pub async fn run_once_for(&self, account_id: &str) -> Result<SyncReport, SyncError> {
        self.run_once_scoped(Some(account_id)).await
    }

    async fn run_once_scoped(&self, account_id: Option<&str>) -> Result<SyncReport, SyncError> {
        let mut report = SyncReport::default();
        let mutations = self.repository.outbox_batch_for(account_id, 100)?;
        if !mutations.is_empty() {
            let expected = mutations
                .iter()
                .map(|item| item.mutation_id)
                .collect::<HashSet<_>>();
            let results = self
                .transport
                .push(self.repository.sync_device_id()?, mutations)
                .await?;
            let mut seen = HashSet::new();
            for result in results {
                if !expected.contains(&result.mutation_id) || !seen.insert(result.mutation_id) {
                    return Err(SyncError::Protocol(
                        "push response contained an unknown or duplicate receipt",
                    ));
                }
                if result.accepted {
                    if result.error_code.is_some() || result.current.is_some() {
                        return Err(SyncError::Protocol(
                            "accepted push receipt contained conflict data",
                        ));
                    }
                    self.repository.acknowledge_mutation(result.mutation_id)?;
                    report.pushed += 1;
                } else {
                    let current = result.current.ok_or(SyncError::Protocol(
                        "rejected push receipt was missing the current entity",
                    ))?;
                    self.repository
                        .record_push_conflict(result.mutation_id, &current)?;
                    report.conflicts += 1;
                }
            }
            if seen.len() != expected.len() {
                return Err(SyncError::Protocol(
                    "push response did not contain every mutation receipt",
                ));
            }
        }

        let mut snapshot_cursor = None;
        let mut pull_cursor = self.repository.sync_cursor_for(account_id)?;
        let mut pending_pages = Vec::new();
        for _ in 0..64 {
            let previous_cursor = pull_cursor;
            let page = if let Some(cursor) = snapshot_cursor {
                self.transport.snapshot(cursor, 100).await?
            } else {
                let page = self.transport.pull(previous_cursor, 100).await?;
                if page.snapshot_required {
                    report.snapshot_restored = true;
                    snapshot_cursor = Some(0);
                    continue;
                }
                page
            };
            validate_page(&page)?;
            let has_more = page.has_more;
            let next_cursor = page.next_cursor;
            if has_more && next_cursor <= previous_cursor {
                return Err(SyncError::Protocol(
                    "pull pagination cursor did not advance",
                ));
            }
            report.pulled += page.changes.len();
            pull_cursor = next_cursor;
            pending_pages.push(page);
            if has_more {
                if snapshot_cursor.is_some() {
                    snapshot_cursor = Some(next_cursor);
                }
            } else if snapshot_cursor.is_some() {
                // Resume incremental pull from the last snapshot row so changes
                // made while pages were streaming cannot be skipped.
                snapshot_cursor = None;
            } else {
                self.repository
                    .apply_remote_pages_for(account_id, &pending_pages)?;
                return Ok(report);
            }
        }
        Err(SyncError::Protocol(
            "pull pagination exceeded the 64-page limit",
        ))
    }
}

/// Downloads a complete non-sensitive cloud snapshot without changing local state.
pub async fn download_cloud_snapshot<T: SyncTransport>(
    transport: &T,
) -> Result<(Vec<SyncChange>, u64), SyncError> {
    let mut changes = Vec::new();
    let mut cursor = 0;
    for _ in 0..64 {
        let page = transport.snapshot(cursor, 100).await?;
        validate_page(&page)?;
        if page
            .changes
            .first()
            .is_some_and(|change| change.change_seq <= cursor)
        {
            return Err(SyncError::Protocol(
                "snapshot page sequence did not advance across pages",
            ));
        }
        if page.has_more && page.next_cursor <= cursor {
            return Err(SyncError::Protocol(
                "snapshot pagination cursor did not advance",
            ));
        }
        cursor = page.next_cursor;
        changes.extend(page.changes);
        if !page.has_more {
            return Ok((changes, cursor));
        }
    }
    Err(SyncError::Protocol(
        "snapshot pagination exceeded the 64-page limit",
    ))
}

fn validate_page(page: &SyncPullPage) -> Result<(), SyncError> {
    if page.snapshot_required || page.changes.len() > 100 {
        return Err(SyncError::Protocol(
            "pull page unexpectedly requested a snapshot or exceeded 100 changes",
        ));
    }
    let mut previous = 0;
    for change in &page.changes {
        if change.change_seq <= previous {
            return Err(SyncError::Protocol(
                "pull page change sequence was not strictly increasing",
            ));
        }
        previous = change.change_seq;
    }
    if page.next_cursor < previous {
        return Err(SyncError::Protocol(
            "pull cursor was behind the last change",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use sparrowshell_domain::{
        AuthType, Folder, FolderId, FolderSyncDto, LocalServerRecord, ServerId, ServerInput,
        ServerSyncDto, SyncApplyOutcome, SyncChange, SyncEntityType,
    };
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeTransport {
        fail_push: bool,
        push_calls: Mutex<usize>,
        pages: Mutex<Vec<SyncPullPage>>,
        snapshot_pages: Mutex<Vec<SyncPullPage>>,
    }

    #[async_trait]
    impl SyncTransport for FakeTransport {
        async fn push(
            &self,
            _device: Uuid,
            changes: Vec<SyncMutation>,
        ) -> Result<Vec<SyncPushResult>, SyncError> {
            *self.push_calls.lock().unwrap() += 1;
            if self.fail_push {
                return Err(SyncError::Transport("offline"));
            }
            Ok(changes
                .into_iter()
                .map(|change| SyncPushResult {
                    mutation_id: change.mutation_id,
                    accepted: true,
                    revision: Some(change.base_revision + 1),
                    change_seq: None,
                    current: None,
                    error_code: None,
                })
                .collect())
        }

        async fn pull(&self, cursor: u64, _limit: usize) -> Result<SyncPullPage, SyncError> {
            Ok(self
                .pages
                .lock()
                .unwrap()
                .pop()
                .unwrap_or_else(|| SyncPullPage {
                    next_cursor: cursor,
                    ..SyncPullPage::default()
                }))
        }

        async fn snapshot(&self, _cursor: u64, _limit: usize) -> Result<SyncPullPage, SyncError> {
            Ok(self
                .snapshot_pages
                .lock()
                .unwrap()
                .pop()
                .unwrap_or_default())
        }
    }

    fn server(repository: &Repository) -> sparrowshell_domain::LocalServerRecord {
        repository
            .upsert_server(ServerInput {
                id: None,
                folder_id: None,
                name: "local".into(),
                host: "127.0.0.1".into(),
                port: 22,
                username: "tester".into(),
                auth_type: AuthType::Password,
                private_key_path_hint: None,
                jump_server_id: None,
            })
            .unwrap()
    }

    fn remote_folder(
        id: FolderId,
        parent_id: Option<FolderId>,
        name: &str,
        change_seq: u64,
    ) -> SyncChange {
        let folder = Folder {
            id,
            parent_id,
            name: name.into(),
            sort_order: 0,
            revision: 1,
            updated_at: Utc::now(),
        };
        SyncChange {
            change_seq,
            entity_type: SyncEntityType::Folder,
            entity_id: id.0,
            revision: 1,
            deleted: false,
            data: serde_json::to_value(FolderSyncDto::from_parts(&folder, None)).unwrap(),
        }
    }

    fn remote_server(
        id: ServerId,
        folder_id: Option<FolderId>,
        jump_server_id: Option<ServerId>,
        name: &str,
        change_seq: u64,
    ) -> SyncChange {
        let server = LocalServerRecord {
            id,
            folder_id,
            name: name.into(),
            host: "127.0.0.1".into(),
            port: 22,
            username: "tester".into(),
            auth_type: AuthType::Password,
            private_key_path_hint: None,
            jump_server_id,
            sort_order: 0,
            credential_ref: None,
            revision: 1,
            updated_at: Utc::now(),
            deleted_at: None,
        };
        SyncChange {
            change_seq,
            entity_type: SyncEntityType::Server,
            entity_id: id.0,
            revision: 1,
            deleted: false,
            data: serde_json::to_value(ServerSyncDto::from(&server)).unwrap(),
        }
    }

    #[test]
    fn remote_page_accepts_child_records_before_their_dependencies() {
        let repository = Repository::in_memory().unwrap();
        let parent_id = FolderId::new();
        let child_id = FolderId::new();
        let jump_id = ServerId::new();
        let target_id = ServerId::new();
        let page = SyncPullPage {
            changes: vec![
                remote_folder(child_id, Some(parent_id), "child", 1),
                remote_server(target_id, Some(parent_id), Some(jump_id), "target", 2),
                remote_server(jump_id, Some(parent_id), None, "jump", 3),
                remote_folder(parent_id, None, "parent", 4),
            ],
            next_cursor: 4,
            has_more: false,
            snapshot_required: false,
        };

        let results = repository.apply_remote_page(&page).unwrap();

        assert_eq!(results.len(), 4);
        let snapshot = repository.snapshot().unwrap();
        assert_eq!(snapshot.folders.len(), 2);
        assert_eq!(snapshot.servers.len(), 2);
        let target = snapshot
            .servers
            .iter()
            .find(|server| server.id == target_id)
            .unwrap();
        assert_eq!(target.folder_id, Some(parent_id));
        assert_eq!(target.jump_server_id, Some(jump_id));
    }

    #[tokio::test]
    async fn sync_applies_dependencies_arriving_on_later_pages_as_one_batch() {
        let repository = Repository::in_memory().unwrap();
        let parent_id = FolderId::new();
        let child_id = FolderId::new();
        let jump_id = ServerId::new();
        let target_id = ServerId::new();
        let transport = FakeTransport {
            pages: Mutex::new(vec![
                SyncPullPage {
                    changes: vec![
                        remote_server(jump_id, Some(parent_id), None, "jump", 3),
                        remote_folder(parent_id, None, "parent", 4),
                    ],
                    next_cursor: 4,
                    has_more: false,
                    snapshot_required: false,
                },
                SyncPullPage {
                    changes: vec![
                        remote_folder(child_id, Some(parent_id), "child", 1),
                        remote_server(target_id, Some(parent_id), Some(jump_id), "target", 2),
                    ],
                    next_cursor: 2,
                    has_more: true,
                    snapshot_required: false,
                },
            ]),
            ..FakeTransport::default()
        };

        let report = SyncEngine::new(&repository, &transport)
            .run_once()
            .await
            .unwrap();

        assert_eq!(report.pulled, 4);
        assert_eq!(repository.sync_cursor().unwrap(), 4);
        let snapshot = repository.snapshot().unwrap();
        assert_eq!(snapshot.folders.len(), 2);
        assert_eq!(snapshot.servers.len(), 2);
    }

    #[test]
    fn remote_batch_rejects_missing_references_and_rolls_back_every_change() {
        let repository = Repository::in_memory().unwrap();
        let pages = vec![
            SyncPullPage {
                changes: vec![remote_folder(FolderId::new(), None, "root", 1)],
                next_cursor: 1,
                has_more: true,
                snapshot_required: false,
            },
            SyncPullPage {
                changes: vec![remote_server(
                    ServerId::new(),
                    Some(FolderId::new()),
                    None,
                    "orphan",
                    2,
                )],
                next_cursor: 2,
                has_more: false,
                snapshot_required: false,
            },
        ];
        let error = repository.apply_remote_pages_for(None, &pages).unwrap_err();

        assert!(matches!(
            error,
            sparrowshell_storage::StorageError::Database(_)
        ));
        assert!(repository.snapshot().unwrap().folders.is_empty());
        assert!(repository.snapshot().unwrap().servers.is_empty());
        assert_eq!(repository.sync_cursor().unwrap(), 0);
    }

    #[tokio::test]
    async fn offline_push_keeps_the_same_stable_mutation_for_retry() {
        let repository = Repository::in_memory().unwrap();
        server(&repository);
        let before = repository.outbox_batch(10).unwrap();
        let transport = FakeTransport {
            fail_push: true,
            ..FakeTransport::default()
        };
        let error = SyncEngine::new(&repository, &transport)
            .run_once()
            .await
            .unwrap_err();
        assert!(matches!(error, SyncError::Transport("offline")));
        assert_eq!(
            repository.outbox_batch(10).unwrap()[0].mutation_id,
            before[0].mutation_id
        );
    }

    #[tokio::test]
    async fn only_acknowledged_mutations_leave_the_outbox() {
        let repository = Repository::in_memory().unwrap();
        server(&repository);
        let transport = FakeTransport::default();
        let report = SyncEngine::new(&repository, &transport)
            .run_once()
            .await
            .unwrap();
        assert_eq!(report.pushed, 1);
        assert_eq!(repository.outbox_len().unwrap(), 0);
        assert_eq!(*transport.push_calls.lock().unwrap(), 1);
    }

    #[test]
    fn remote_change_conflicts_with_pending_local_work_without_overwriting_either() {
        let repository = Repository::in_memory().unwrap();
        let local = server(&repository);
        let mut remote = sparrowshell_domain::ServerSyncDto::from(&local);
        remote.name = "cloud".into();
        remote.revision = 2;
        remote.updated_at = Utc::now();
        let change = SyncChange {
            change_seq: 1,
            entity_type: SyncEntityType::Server,
            entity_id: local.id.0,
            revision: 2,
            deleted: false,
            data: serde_json::to_value(remote).unwrap(),
        };
        let results = repository
            .apply_remote_page(&SyncPullPage {
                changes: vec![change],
                next_cursor: 1,
                has_more: false,
                snapshot_required: false,
            })
            .unwrap();
        assert_eq!(results[0].outcome, SyncApplyOutcome::Conflict);
        assert_eq!(repository.outbox_len().unwrap(), 1);
        assert_eq!(repository.server(local.id).unwrap().unwrap().name, "local");
        assert_eq!(repository.unresolved_conflicts().unwrap().len(), 1);
        assert_eq!(repository.sync_cursor().unwrap(), 1);
    }

    #[test]
    fn remote_tombstone_is_applied_without_creating_outbox_and_returns_local_secret_reference() {
        let repository = Repository::in_memory().unwrap();
        let local = server(&repository);
        repository
            .set_credential_ref(local.id, Some("ssh/local-only".into()))
            .unwrap();
        for mutation in repository.outbox_batch(10).unwrap() {
            repository
                .acknowledge_mutation(mutation.mutation_id)
                .unwrap();
        }
        let mut remote = sparrowshell_domain::ServerSyncDto::from(&local);
        remote.revision = 2;
        remote.deleted_at = Some(Utc::now());
        let result = repository
            .apply_remote_page(&SyncPullPage {
                changes: vec![SyncChange {
                    change_seq: 1,
                    entity_type: SyncEntityType::Server,
                    entity_id: local.id.0,
                    revision: 2,
                    deleted: true,
                    data: serde_json::to_value(remote).unwrap(),
                }],
                next_cursor: 1,
                has_more: false,
                snapshot_required: false,
            })
            .unwrap();
        assert_eq!(result[0].outcome, SyncApplyOutcome::Applied);
        assert_eq!(result[0].credential_ref.as_deref(), Some("ssh/local-only"));
        assert!(repository.server(local.id).unwrap().is_none());
        assert_eq!(repository.outbox_len().unwrap(), 0);
    }

    #[test]
    fn explicit_keep_local_rebases_without_losing_local_edit() {
        let repository = Repository::in_memory().unwrap();
        let local = server(&repository);
        let mut remote = sparrowshell_domain::ServerSyncDto::from(&local);
        remote.name = "cloud".into();
        remote.revision = 2;
        repository
            .apply_remote_page(&SyncPullPage {
                changes: vec![SyncChange {
                    change_seq: 1,
                    entity_type: SyncEntityType::Server,
                    entity_id: local.id.0,
                    revision: 2,
                    deleted: false,
                    data: serde_json::to_value(remote).unwrap(),
                }],
                next_cursor: 1,
                has_more: false,
                snapshot_required: false,
            })
            .unwrap();
        let conflict = repository.unresolved_conflicts().unwrap().remove(0);
        repository
            .resolve_conflict_keep_local(conflict.conflict_id)
            .unwrap();
        let pending = repository.outbox_batch(10).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].base_revision, 2);
        assert_eq!(pending[0].data["revision"], 3);
        assert_eq!(repository.server(local.id).unwrap().unwrap().name, "local");
        assert!(repository.unresolved_conflicts().unwrap().is_empty());
    }

    #[test]
    fn explicit_keep_remote_discards_only_the_conflicting_entity_mutation() {
        let repository = Repository::in_memory().unwrap();
        let local = server(&repository);
        let mut remote = sparrowshell_domain::ServerSyncDto::from(&local);
        remote.name = "cloud".into();
        remote.revision = 2;
        repository
            .apply_remote_page(&SyncPullPage {
                changes: vec![SyncChange {
                    change_seq: 1,
                    entity_type: SyncEntityType::Server,
                    entity_id: local.id.0,
                    revision: 2,
                    deleted: false,
                    data: serde_json::to_value(remote).unwrap(),
                }],
                next_cursor: 1,
                has_more: false,
                snapshot_required: false,
            })
            .unwrap();
        let conflict = repository.unresolved_conflicts().unwrap().remove(0);
        repository
            .resolve_conflict_keep_remote(conflict.conflict_id)
            .unwrap();
        assert_eq!(repository.server(local.id).unwrap().unwrap().name, "cloud");
        assert_eq!(repository.outbox_len().unwrap(), 0);
        assert!(repository.unresolved_conflicts().unwrap().is_empty());
    }

    #[tokio::test]
    async fn snapshot_restores_every_page_then_returns_to_incremental_pull() {
        fn change(sequence: u64) -> SyncChange {
            let folder = sparrowshell_domain::Folder {
                id: sparrowshell_domain::FolderId::new(),
                parent_id: None,
                name: format!("folder-{sequence}"),
                sort_order: sequence as i64,
                revision: 1,
                updated_at: Utc::now(),
            };
            SyncChange {
                change_seq: sequence,
                entity_type: SyncEntityType::Folder,
                entity_id: folder.id.0,
                revision: 1,
                deleted: false,
                data: serde_json::to_value(FolderSyncDto::from_parts(&folder, None)).unwrap(),
            }
        }
        let repository = Repository::in_memory().unwrap();
        let transport = FakeTransport {
            pages: Mutex::new(vec![SyncPullPage {
                snapshot_required: true,
                ..SyncPullPage::default()
            }]),
            snapshot_pages: Mutex::new(vec![
                SyncPullPage {
                    changes: vec![change(2)],
                    next_cursor: 2,
                    has_more: false,
                    snapshot_required: false,
                },
                SyncPullPage {
                    changes: vec![change(1)],
                    next_cursor: 1,
                    has_more: true,
                    snapshot_required: false,
                },
            ]),
            ..FakeTransport::default()
        };
        let report = SyncEngine::new(&repository, &transport)
            .run_once()
            .await
            .unwrap();
        assert!(report.snapshot_restored);
        assert_eq!(report.pulled, 2);
        assert_eq!(repository.sync_cursor().unwrap(), 2);
        assert_eq!(repository.snapshot().unwrap().folders.len(), 2);
    }

    #[test]
    fn folder_dto_uses_sync_metadata_and_rejects_secret_fields() {
        let repository = Repository::in_memory().unwrap();
        repository.create_folder(None, "root").unwrap();
        let mutation = repository.outbox_batch(10).unwrap().remove(0);
        let _: FolderSyncDto = serde_json::from_value(mutation.data.clone()).unwrap();
        let mut poisoned = mutation.data;
        poisoned["password"] = serde_json::Value::String("canary-not-a-real-secret".into());
        assert!(serde_json::from_value::<FolderSyncDto>(poisoned).is_err());
    }

    #[tokio::test]
    async fn account_scoped_sync_pushes_only_the_requested_account_outbox() {
        let repository = Repository::in_memory().unwrap();
        repository.set_workspace_owner("account-a").unwrap();
        server(&repository);
        repository.set_workspace_owner("account-b").unwrap();
        server(&repository);
        let transport = FakeTransport::default();

        let report = SyncEngine::new(&repository, &transport)
            .run_once_for("account-a")
            .await
            .unwrap();

        assert_eq!(report.pushed, 1);
        assert_eq!(
            repository
                .outbox_batch_for(Some("account-a"), 10)
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            repository
                .outbox_batch_for(Some("account-b"), 10)
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn cloud_snapshot_download_validates_pagination_without_touching_repository() {
        let repository = Repository::in_memory().unwrap();
        let local = server(&repository);
        let change = |sequence: u64| {
            let mut remote = sparrowshell_domain::ServerSyncDto::from(&local);
            remote.revision = sequence;
            remote.updated_at = Utc::now();
            SyncChange {
                change_seq: sequence,
                entity_type: SyncEntityType::Server,
                entity_id: local.id.0,
                revision: sequence,
                deleted: false,
                data: serde_json::to_value(remote).unwrap(),
            }
        };
        let transport = FakeTransport {
            snapshot_pages: Mutex::new(vec![
                SyncPullPage {
                    changes: vec![change(2)],
                    next_cursor: 2,
                    has_more: false,
                    snapshot_required: false,
                },
                SyncPullPage {
                    changes: vec![change(1)],
                    next_cursor: 1,
                    has_more: true,
                    snapshot_required: false,
                },
            ]),
            ..FakeTransport::default()
        };

        let (changes, cursor) = download_cloud_snapshot(&transport).await.unwrap();

        assert_eq!(changes.len(), 2);
        assert_eq!(cursor, 2);
        assert_eq!(repository.outbox_len().unwrap(), 1);
        assert_eq!(repository.sync_cursor().unwrap(), 0);
    }

    #[tokio::test]
    async fn cloud_snapshot_download_rejects_a_non_advancing_cursor() {
        let transport = FakeTransport {
            snapshot_pages: Mutex::new(vec![SyncPullPage {
                next_cursor: 0,
                has_more: true,
                snapshot_required: false,
                changes: vec![],
            }]),
            ..FakeTransport::default()
        };
        let error = download_cloud_snapshot(&transport).await.unwrap_err();
        assert!(matches!(error, SyncError::Protocol(_)));
    }
}
