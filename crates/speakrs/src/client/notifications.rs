use notify_rust::Notification;

// TODO: actions

pub fn message() -> notify_rust::error::Result<notify_rust::NotificationHandle> {
    Ok(Notification::new()
        .summary("News")
        .body("This will almost look like real")
        .icon("firefox")
        .show()?)
}
