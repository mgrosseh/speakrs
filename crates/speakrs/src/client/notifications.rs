use notify_rust::Notification;

#[allow(unused)]
pub fn message(
    channel_name: &str,
    sender_name: &str,
    message_content: &str,
) -> notify_rust::error::Result<notify_rust::NotificationHandle> {
    let title = format!("{sender_name} [{channel_name}]");
    let body = message_content;
    Ok(Notification::new()
        .summary(&title)
        .body(body)
        .action("view", "View") // TODO: handle action using x.wait_for_response
        .icon("firefox")
        .show()?)
}
