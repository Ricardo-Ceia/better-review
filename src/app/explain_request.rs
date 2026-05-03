use anyhow::Result;

use crate::services::opencode::{WhyAnswer, WhyTarget};

use super::explain_history::{
    ExplainRun, ExplainRunStatus, find_explain_run_index_by_id, find_reusable_explain_run_index,
    next_explain_run_id,
};
use super::{
    App, KeybindingCommand, Message, current_why_target, key_status_label, resolved_why_model,
    why_model_display_label,
};

pub(super) async fn request_explain(app: &mut App) -> Result<()> {
    let Some(_opencode) = app.opencode.clone() else {
        app.status = "Explain is unavailable because opencode could not start.".to_string();
        return Ok(());
    };
    let Some(session) = app.active_session().cloned() else {
        app.status = format!(
            "No context source is linked to this repository. Press {} to choose one.",
            key_status_label(app, KeybindingCommand::ExplainContext)
        );
        return Ok(());
    };

    let Some((label, target)) = current_why_target(&app.review) else {
        app.status = "Nothing is selected to explain.".to_string();
        return Ok(());
    };

    let resolved_model = resolved_why_model(app);
    let session_id = session.id.clone();
    let session_label = format!("{} ({})", session.title, session.id);
    let model_label = why_model_display_label(app);
    request_explain_with_target(
        app,
        label,
        target,
        session_id,
        session_label,
        resolved_model,
        model_label,
    )
    .await
}

pub(super) async fn request_explain_with_target(
    app: &mut App,
    label: String,
    target: WhyTarget,
    context_source_id: String,
    context_source_label: String,
    requested_model: Option<String>,
    model_label: String,
) -> Result<()> {
    let Some(opencode) = app.opencode.clone() else {
        app.status = "Explain is unavailable because opencode could not start.".to_string();
        return Ok(());
    };

    let cache_key = target.cache_key_for_model(&context_source_id, requested_model.as_deref());
    if let Some(index) = find_reusable_explain_run_index(&app.why_this, &cache_key) {
        if let Some(run) = app.why_this.runs.get(index) {
            app.why_this.current_run_id = Some(run.id);
            app.why_this.history_cursor = index;
        }
        app.status = "Focused the existing explanation.".to_string();
        return Ok(());
    }

    if let Some(answer) = app.why_this.cache.get(&cache_key).cloned() {
        let run_id = next_explain_run_id(&mut app.why_this);
        app.why_this.runs.push(ExplainRun {
            id: run_id,
            label: label.clone(),
            target,
            context_source_id,
            context_source_label,
            requested_model,
            model_label,
            cache_key,
            status: ExplainRunStatus::Ready,
            result: Some(answer),
            error: None,
            handle: None,
        });
        app.why_this.current_run_id = Some(run_id);
        app.why_this.history_cursor = app.why_this.runs.len().saturating_sub(1);
        app.status = "Loaded the cached explanation.".to_string();
        return Ok(());
    }

    let run_id = next_explain_run_id(&mut app.why_this);
    let cache_key_for_message = cache_key.clone();
    let target_for_run = target.clone();
    let requested_model_for_task = requested_model.clone();
    let context_source_id_for_task = context_source_id.clone();
    let tx = app.tx.clone();

    app.status = format!("Running Explain for {label} with {model_label}.");

    let handle = tokio::spawn(async move {
        let result = opencode
            .ask_why(
                &context_source_id_for_task,
                &target,
                requested_model_for_task.as_deref(),
            )
            .await
            .map_err(|err| err.to_string());
        let _ = tx.send(Message::WhyThis {
            job_id: run_id,
            cache_key: cache_key_for_message,
            label,
            result,
        });
    });

    app.why_this.runs.push(ExplainRun {
        id: run_id,
        label: target_for_run.label(),
        target: target_for_run,
        context_source_id,
        context_source_label,
        requested_model,
        model_label,
        cache_key,
        status: ExplainRunStatus::Running,
        result: None,
        error: None,
        handle: Some(handle),
    });
    app.why_this.current_run_id = Some(run_id);
    app.why_this.history_cursor = app.why_this.runs.len().saturating_sub(1);

    Ok(())
}

pub(super) async fn retry_current_explain(app: &mut App) -> Result<()> {
    let Some(run_id) = app.why_this.current_run_id else {
        app.status = "No current explain run.".to_string();
        return Ok(());
    };
    retry_run_by_id(app, run_id).await
}

async fn retry_run_by_id(app: &mut App, run_id: u64) -> Result<()> {
    let Some(index) = find_explain_run_index_by_id(&app.why_this, run_id) else {
        app.status = "Explain run no longer exists.".to_string();
        return Ok(());
    };
    let Some(run) = app.why_this.runs.get(index) else {
        app.status = "Explain run no longer exists.".to_string();
        return Ok(());
    };
    if matches!(run.status, ExplainRunStatus::Running) {
        app.status = format!("Explain run #{} is already running.", run.id);
        return Ok(());
    }

    request_explain_with_target(
        app,
        run.label.clone(),
        run.target.clone(),
        run.context_source_id.clone(),
        run.context_source_label.clone(),
        run.requested_model.clone(),
        run.model_label.clone(),
    )
    .await
}

pub(super) fn handle_explain_result(
    app: &mut App,
    job_id: u64,
    cache_key: String,
    label: String,
    result: Result<WhyAnswer, String>,
) {
    if let Some(index) = find_explain_run_index_by_id(&app.why_this, job_id) {
        let is_running = matches!(
            app.why_this.runs.get(index).map(|run| &run.status),
            Some(ExplainRunStatus::Running)
        );
        if !is_running {
            return;
        }

        let retry_key = key_status_label(app, KeybindingCommand::ExplainRetry);
        if let Some(run) = app.why_this.runs.get_mut(index) {
            run.handle = None;
            match result {
                Ok(answer) => {
                    app.status = format!("Loaded explanation for {label}.");
                    app.why_this.cache.insert(cache_key, answer.clone());
                    run.status = ExplainRunStatus::Ready;
                    run.result = Some(answer);
                    run.error = None;
                }
                Err(error) => {
                    app.status = format!("Explain failed: {error}. Press {} to retry.", retry_key);
                    run.status = ExplainRunStatus::Failed;
                    run.error = Some(error);
                    run.result = None;
                }
            }
        }
        app.why_this.current_run_id = Some(job_id);
        app.why_this.history_cursor = index;
    }
}
