use crate::{error::AppResult, locale::Locale};

use super::{
    super::{
        storage,
        types::{ConversationRun, ThreadSubject},
    },
    events::ConversationEvent,
    task::TurnCancellation,
    turn_support::subject_clarification_summary,
    ConversationEngine,
};

impl ConversationEngine {
    pub(super) async fn persist_subject_clarification(
        &self,
        run: &ConversationRun,
        subject: &ThreadSubject,
        locale: Locale,
        message_id: &str,
        cancellation: &TurnCancellation,
    ) -> AppResult<()> {
        cancellation.ensure_active()?;
        self.phase(run, "persisting", None, None).await?;
        storage::insert_turn_summary(
            &self.pool,
            &run.id,
            &run.thread_id,
            subject,
            subject_clarification_summary(locale),
        )
        .await?;
        cancellation.ensure_active()?;
        let transitioned =
            storage::finish_run(&self.pool, &run.id, "completed", "completed", None, None).await?;
        if transitioned {
            self.emit(
                &run.id,
                &run.thread_id,
                ConversationEvent::RunCompleted {
                    message_id: message_id.to_string(),
                },
            )
            .await?;
        }
        Ok(())
    }
}
