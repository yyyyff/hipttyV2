use std::sync::{LazyLock, Mutex};
use std::time::Instant;

use hiptty_core::{AdapterError, AdapterResult, PostAction};

/// Aligned with hipda `PostHelper.POST_DELAY_IN_SECS`.
const POST_DELAY_SECS: u64 = 30;

static LAST_POST: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));

pub fn requires_throttle(action: &PostAction) -> bool {
    !matches!(
        action,
        PostAction::EditPost { .. } | PostAction::QuickDelete { .. }
    )
}

pub fn check_post_allowed() -> AdapterResult<()> {
    let guard = LAST_POST
        .lock()
        .map_err(|e| AdapterError::InvalidInput(format!("post throttle lock: {e}")))?;
    let Some(last) = *guard else {
        return Ok(());
    };
    let elapsed = last.elapsed().as_secs();
    if elapsed < POST_DELAY_SECS {
        let wait = POST_DELAY_SECS - elapsed;
        return Err(AdapterError::RateLimit(format!(
            "wait {wait} seconds before posting again"
        )));
    }
    Ok(())
}

pub fn record_post_success(action: &PostAction, delete: bool) {
    if delete {
        return;
    }
    if matches!(action, PostAction::EditPost { .. }) {
        return;
    }
    if let Ok(mut guard) = LAST_POST.lock() {
        *guard = Some(Instant::now());
    }
}

#[allow(dead_code)]
pub fn wait_seconds_to_post() -> u64 {
    let Ok(guard) = LAST_POST.lock() else {
        return 0;
    };
    let Some(last) = *guard else {
        return 0;
    };
    let elapsed = last.elapsed().as_secs();
    POST_DELAY_SECS.saturating_sub(elapsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_and_delete_skip_throttle() {
        assert!(!requires_throttle(&PostAction::EditPost {
            tid: "1".into(),
            pid: "2".into(),
            fid: 7,
            page: 1,
        }));
        assert!(!requires_throttle(&PostAction::QuickDelete {
            tid: "1".into(),
            pid: "2".into(),
            fid: 7,
        }));
        assert!(requires_throttle(&PostAction::ReplyThread {
            tid: "1".into()
        }));
    }
}
