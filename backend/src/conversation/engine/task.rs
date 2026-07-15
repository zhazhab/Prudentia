use std::{future::Future, time::Duration};

use tokio::{sync::watch, task::JoinHandle, time::timeout};

use crate::error::{AppError, AppResult};

const GRACEFUL_CANCEL_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone)]
pub(super) struct TurnCancellation {
    receiver: watch::Receiver<bool>,
}

impl TurnCancellation {
    pub(super) fn ensure_active(&self) -> AppResult<()> {
        if self.is_cancelled() {
            Err(AppError::internal("conversation run canceled"))
        } else {
            Ok(())
        }
    }

    pub(super) fn is_cancelled(&self) -> bool {
        *self.receiver.borrow()
    }

    pub(super) async fn cancelled(&self) {
        let mut receiver = self.receiver.clone();
        if *receiver.borrow() {
            return;
        }
        let _ = receiver.changed().await;
    }
}

pub(super) struct TurnTask {
    cancel: watch::Sender<bool>,
    handle: JoinHandle<()>,
}

impl TurnTask {
    pub(super) fn spawn<F, Fut>(run: F) -> Self
    where
        F: FnOnce(TurnCancellation) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let (cancel, receiver) = watch::channel(false);
        let handle = tokio::spawn(run(TurnCancellation { receiver }));
        Self { cancel, handle }
    }

    pub(super) fn request_cancel(&self) {
        let _ = self.cancel.send(true);
    }

    pub(super) async fn cancel_and_wait(mut self) {
        self.request_cancel();
        if timeout(GRACEFUL_CANCEL_TIMEOUT, &mut self.handle)
            .await
            .is_err()
        {
            self.handle.abort();
            let _ = self.handle.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::oneshot;

    use super::*;

    #[tokio::test]
    async fn cancellation_is_delivered_before_the_task_is_aborted() {
        let (observed_tx, observed_rx) = oneshot::channel();
        let task = TurnTask::spawn(|cancellation| async move {
            cancellation.cancelled().await;
            observed_tx.send(()).expect("report cooperative stop");
        });

        task.cancel_and_wait().await;

        observed_rx.await.expect("task observed cancellation");
    }
}
