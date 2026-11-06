//! Activity center tasks: dismissing and clearing finished tasks removes them on the server.

use crate::harness::*;

/// Which of `ids` the user still has. Valkey is shared between tests, so only look at our own.
async fn remaining(app: &TestApp, user: &TestUser, ids: &[&str]) -> Vec<String> {
    let tasks = app.state.task_manager.get_user_tasks(user.id).await.unwrap();
    ids.iter().filter(|id| tasks.iter().any(|t| t.id == **id)).map(|id| id.to_string()).collect()
}

async fn finished_task(app: &TestApp, user: &TestUser) -> String {
    let id = app.state.task_manager.create_task("download_episode".into(), user.id).await.unwrap();
    app.state.task_manager.complete_task(&id, None, None).await.unwrap();
    id
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dismissed_task_stays_gone() {
    let app = TestApp::new().await;
    let user = app.user("activity").await;
    let done = finished_task(&app, &user).await;
    let kept = finished_task(&app, &user).await;

    app.post(&format!("/api/tasks/{done}/dismiss")).query("user_id", user.id).key(&user).send().await.assert_ok();
    assert_eq!(remaining(&app, &user, &[&done, &kept]).await, vec![kept.clone()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn clearing_removes_finished_tasks_but_not_running_ones() {
    let app = TestApp::new().await;
    let user = app.user("activity").await;
    let done = finished_task(&app, &user).await;
    let failed = app.state.task_manager.create_task("download_episode".into(), user.id).await.unwrap();
    app.state.task_manager.fail_task(&failed, "boom".into()).await.unwrap();
    let running = app.state.task_manager.create_task("download_episode".into(), user.id).await.unwrap();

    app.post("/api/tasks/clear_finished").query("user_id", user.id).key(&user).send().await.assert_ok();
    assert_eq!(remaining(&app, &user, &[&done, &failed, &running]).await, vec![running.clone()]);

    app.post(&format!("/api/tasks/{running}/dismiss")).query("user_id", user.id).key(&user).send().await.assert_ok();
    assert_eq!(remaining(&app, &user, &[&running]).await, vec![running.clone()], "a running task can't be dismissed");
}

#[tokio::test(flavor = "multi_thread")]
async fn another_users_task_cannot_be_dismissed() {
    let app = TestApp::new().await;
    let owner = app.user("activity-owner").await;
    let other = app.user("activity-other").await;
    let done = finished_task(&app, &owner).await;

    app.post(&format!("/api/tasks/{done}/dismiss")).query("user_id", owner.id).key(&other).send().await.assert_status(403);
    app.post(&format!("/api/tasks/{done}/dismiss")).query("user_id", other.id).key(&other).send().await.assert_ok();
    app.post("/api/tasks/clear_finished").query("user_id", other.id).key(&other).send().await.assert_ok();
    assert_eq!(remaining(&app, &owner, &[&done]).await, vec![done.clone()]);
}
