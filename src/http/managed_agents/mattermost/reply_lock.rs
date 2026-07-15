use tokio::sync::OwnedMutexGuard;

use crate::agents::locks::KeyedLockStore;

pub(super) struct MattermostPromptLock {
    _guard: OwnedMutexGuard<()>,
}

impl MattermostPromptLock {
    pub(super) async fn acquire(locks: &KeyedLockStore, session_id: &str) -> Self {
        Self {
            _guard: locks.lock(&format!("mattermost_prompt:{session_id}")).await,
        }
    }
}
